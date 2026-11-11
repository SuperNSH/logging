//! The logger as TSX sees it: `NativeLogging`, a class whose async members reject with a
//! `code` on failure. Results are plain values; the files stay on the Rust side.

use std::{sync::Arc, time::Duration};

use nsh_core::prelude::*;

use crate::{
    error::ErrorKind,
    logging::LogEntry,
    options::{Level, LoggingOptions, Rotation},
    Logging, LoggingError,
};

/// The options of one logger, from TSX. Unset fields keep the defaults.
#[derive(JsObject, Clone, Debug, Default)]
pub struct NativeLoggingOptions {
    /// `"debug"`, `"info"`, `"warning"` or `"error"`. The default is `debug`.
    pub level: Option<String>,
    /// Where the log files live.
    pub directory: Option<String>,
    /// The file names' prefix. The default is `"log"`.
    pub file_base_name: Option<String>,
    /// `"never"`, `"minutely"`, `"hourly"` or `"daily"`. The default is `daily`.
    pub rotation: Option<String>,
    /// The size in bytes at which a file rolls mid-interval.
    pub max_file_size_bytes: Option<u64>,
    /// The total bytes the directory may hold.
    pub max_total_size_bytes: Option<u64>,
    /// The age in milliseconds at which a file is deleted.
    pub max_age_ms: Option<u64>,
    /// The fields whose values are masked before any write.
    pub redact: Option<Vec<String>>,
    /// The longest a write waits, in milliseconds. Off unless set; unused on the web.
    pub write_timeout_ms: Option<u64>,
}

/// One written entry, as `logs` returns it to TSX.
#[derive(JsObject, Clone, Debug)]
pub struct NativeLogEntry {
    /// When the entry was written, in milliseconds since the epoch.
    pub time_ms: u64,
    /// The entry's level: `"debug"`, `"info"`, `"warning"` or `"error"`.
    pub level: String,
    /// The message, already redacted.
    pub message: String,
}

/// The handle's state: the options TSX gave, and the logger once they have built it.
struct InitState {
    options: NativeLoggingOptions,
    logging: tokio::sync::Mutex<Option<Logging>>,
}

/// A logging handle. Create one and keep it; every handle of an app shares the platform's
/// writer.
#[class]
pub struct NativeLogging {
    state: Arc<InitState>,
}

#[class]
impl NativeLogging {
    /// The platform's logger. Invalid options surface as a rejection on the first call,
    /// where a constructor answer cannot.
    pub fn new(options: Option<NativeLoggingOptions>) -> NativeLogging {
        NativeLogging {
            state: Arc::new(InitState {
                options: options.unwrap_or_default(),
                logging: tokio::sync::Mutex::new(None),
            }),
        }
    }

    /// Writes one message at `level`.
    pub async fn log(this: Instance<Self>, message: String, level: String) -> Result<(), JsError> {
        let level = parse_level(&level).ok_or_else(|| {
            invalid("level must be \"debug\", \"info\", \"warning\" or \"error\"")
        })?;
        let logging = ensure(&this).await?;
        logging.log(&message, level).await.map_err(into_js)
    }

    /// Writes at the debug level.
    pub async fn debug(this: Instance<Self>, message: String) -> Result<(), JsError> {
        let logging = ensure(&this).await?;
        logging.debug(&message).await.map_err(into_js)
    }

    /// Writes at the info level.
    pub async fn info(this: Instance<Self>, message: String) -> Result<(), JsError> {
        let logging = ensure(&this).await?;
        logging.info(&message).await.map_err(into_js)
    }

    /// Writes at the warning level.
    pub async fn warning(this: Instance<Self>, message: String) -> Result<(), JsError> {
        let logging = ensure(&this).await?;
        logging.warning(&message).await.map_err(into_js)
    }

    /// Writes at the error level.
    pub async fn error(this: Instance<Self>, message: String) -> Result<(), JsError> {
        let logging = ensure(&this).await?;
        logging.error(&message).await.map_err(into_js)
    }

    /// Records a failure and its stack, at the error level.
    pub async fn log_exception(
        this: Instance<Self>,
        error: String,
        stack: Option<String>,
    ) -> Result<(), JsError> {
        let logging = ensure(&this).await?;
        logging
            .log_exception(&error, stack.as_deref())
            .await
            .map_err(into_js)
    }

    /// The written entries, oldest first.
    pub async fn logs(this: Instance<Self>) -> Result<Vec<NativeLogEntry>, JsError> {
        let logging = ensure(&this).await?;
        let entries = logging.logs().await.map_err(into_js)?;
        Ok(entries.into_iter().map(native_entry).collect())
    }

    /// Empties the stored logs and deletes the files.
    pub async fn clean(this: Instance<Self>) -> Result<(), JsError> {
        let logging = ensure(&this).await?;
        logging.clean().await.map_err(into_js)
    }

    /// Drains the writer, so every appended line is on disk. Call it before exit.
    pub async fn flush(this: Instance<Self>) -> Result<(), JsError> {
        let logging = ensure(&this).await?;
        logging.flush().await.map_err(into_js)
    }
}

impl Default for NativeLogging {
    fn default() -> Self {
        Self::new(None)
    }
}

/// The logger behind this handle, built once from the options TSX gave. A failed build is
/// retried by the next call, so a rejection is never sticky.
async fn ensure(this: &Instance<NativeLogging>) -> Result<Logging, JsError> {
    let state = this.with(|native| Arc::clone(&native.state));
    let mut started = state.logging.lock().await;
    if let Some(logging) = started.as_ref() {
        return Ok(logging.clone());
    }
    let options = to_logging_options(state.options.clone())?;
    let logging = Logging::new(&options).await.map_err(into_js)?;
    *started = Some(logging.clone());
    Ok(logging)
}

/// The error a rejection carries: its message, and the `code` apps match on.
fn into_js(error: LoggingError) -> JsError {
    JsError::Error(error.kind().message().to_owned()).with_code(error.code())
}

fn invalid(message: &str) -> JsError {
    JsError::Error(message.to_owned()).with_code(ErrorKind::InvalidArgument.code())
}

fn parse_level(name: &str) -> Option<Level> {
    match name {
        "debug" => Some(Level::Debug),
        "info" => Some(Level::Info),
        "warning" => Some(Level::Warning),
        "error" => Some(Level::Error),
        _ => None,
    }
}

fn parse_rotation(name: &str) -> Option<Rotation> {
    match name {
        "never" => Some(Rotation::Never),
        "minutely" => Some(Rotation::Minutely),
        "hourly" => Some(Rotation::Hourly),
        "daily" => Some(Rotation::Daily),
        _ => None,
    }
}

fn native_entry(entry: LogEntry) -> NativeLogEntry {
    NativeLogEntry {
        time_ms: entry.time_ms,
        level: entry.level.name().to_owned(),
        message: entry.message,
    }
}

/// Turns the TSX options into the crate's, refusing a level or a rotation it does not know.
fn to_logging_options(options: NativeLoggingOptions) -> Result<LoggingOptions, JsError> {
    let defaults = LoggingOptions::default();
    let level = match options.level.as_deref() {
        None => defaults.level,
        Some(name) => parse_level(name).ok_or_else(|| {
            invalid("level must be \"debug\", \"info\", \"warning\" or \"error\"")
        })?,
    };
    let rotation = match options.rotation.as_deref() {
        None => defaults.rotation,
        Some(name) => parse_rotation(name).ok_or_else(|| {
            invalid("rotation must be \"never\", \"minutely\", \"hourly\" or \"daily\"")
        })?,
    };
    Ok(LoggingOptions {
        level,
        directory: options.directory.unwrap_or(defaults.directory),
        file_base_name: options.file_base_name.unwrap_or(defaults.file_base_name),
        rotation,
        max_file_size_bytes: options.max_file_size_bytes,
        max_total_size_bytes: options.max_total_size_bytes,
        max_age: options.max_age_ms.map(Duration::from_millis),
        redact: options.redact.unwrap_or(defaults.redact),
        write_timeout: options.write_timeout_ms.map(Duration::from_millis),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_level_and_rotation_has_the_name_tsx_uses() {
        assert_eq!(parse_level("debug"), Some(Level::Debug));
        assert_eq!(parse_level("info"), Some(Level::Info));
        assert_eq!(parse_level("warning"), Some(Level::Warning));
        assert_eq!(parse_level("error"), Some(Level::Error));
        assert_eq!(parse_level("critical"), None);
        assert_eq!(parse_level("verbose"), None);
        assert_eq!(parse_rotation("never"), Some(Rotation::Never));
        assert_eq!(parse_rotation("minutely"), Some(Rotation::Minutely));
        assert_eq!(parse_rotation("hourly"), Some(Rotation::Hourly));
        assert_eq!(parse_rotation("daily"), Some(Rotation::Daily));
        assert_eq!(parse_rotation("weekly"), None);
    }

    #[test]
    fn unset_options_keep_the_defaults_and_unknown_ones_are_refused() {
        let options = to_logging_options(NativeLoggingOptions::default()).unwrap();
        assert_eq!(options, LoggingOptions::default());

        let refused = to_logging_options(NativeLoggingOptions {
            level: Some("verbose".to_owned()),
            ..NativeLoggingOptions::default()
        });
        assert!(refused.is_err());

        let refused = to_logging_options(NativeLoggingOptions {
            rotation: Some("weekly".to_owned()),
            ..NativeLoggingOptions::default()
        });
        assert!(refused.is_err());
    }
}
