//! The levels, the rotation and the options of a logger; all fixed at construction.

use std::time::Duration;

use crate::error::{ErrorKind, LoggingError};

/// A severity. `Debug` is the least severe; writes below `LoggingOptions::level` are dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum Level {
    /// The least severe level; talker's `verbose` folds into it.
    #[default]
    Debug,
    /// Routine progress.
    Info,
    /// Something unexpected the app survived.
    Warning,
    /// A failure; talker's `critical` folds into it.
    Error,
}

impl Level {
    /// The level's name, as the fixed line format and TSX spell it.
    pub fn name(self) -> &'static str {
        match self {
            Level::Debug => "debug",
            Level::Info => "info",
            Level::Warning => "warning",
            Level::Error => "error",
        }
    }
}

/// How often the writer rolls to a new file. `weekly` stays tracing-appender's, not ours.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Rotation {
    /// One file for everything ever written.
    Never,
    /// A new file every minute, named `prefix.YYYY-MM-DD-HH-mm`.
    Minutely,
    /// A new file every hour, named `prefix.YYYY-MM-DD-HH`.
    Hourly,
    /// A new file every day, named `prefix.YYYY-MM-DD`.
    #[default]
    Daily,
}

/// The fields masked before any write, as `docs/goals.md` lists them.
const DEFAULT_REDACT: [&str; 6] = [
    "token",
    "password",
    "secret",
    "authorization",
    "email",
    "phone",
];

/// The options of a logger, fixed at construction: there is no reconfigure call, no runtime
/// severity switch and no on/off flag.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoggingOptions {
    /// The least severe level recorded. Writes below it are dropped, not errors.
    pub level: Level,
    /// The directory the log files live in. Non-empty on native targets.
    pub directory: String,
    /// The file names' prefix; the rotation schedule appends the date part.
    pub file_base_name: String,
    /// How often the writer rolls to a new file.
    pub rotation: Rotation,
    /// The size in bytes at which a file rolls mid-interval. `None` leaves the interval alone.
    pub max_file_size_bytes: Option<u64>,
    /// The total bytes the directory may hold; the oldest files go first. `None` caps nothing.
    pub max_total_size_bytes: Option<u64>,
    /// The age at which a file is deleted. `None` keeps every file.
    pub max_age: Option<Duration>,
    /// The fields whose values are masked before any write.
    pub redact: Vec<String>,
    /// The longest a write waits before it resolves with `ERR_LOGGING_TIMEOUT`. Off by
    /// default; accepted but unused on wasm32, which has no timer to wait on.
    pub write_timeout: Option<Duration>,
}

impl Default for LoggingOptions {
    /// Level debug, the temp directory, base name "log", daily rotation, no caps, the six
    /// default redaction fields and no write timeout.
    fn default() -> Self {
        Self {
            level: Level::Debug,
            directory: default_directory(),
            file_base_name: "log".to_owned(),
            rotation: Rotation::Daily,
            max_file_size_bytes: None,
            max_total_size_bytes: None,
            max_age: None,
            redact: DEFAULT_REDACT
                .iter()
                .map(|field| (*field).to_owned())
                .collect(),
            write_timeout: None,
        }
    }
}

impl LoggingOptions {
    /// Refuses what `docs/goals.md` calls out: a zero cap, a zero timeout, an empty file
    /// base name and, on native targets, an empty directory. Level and rotation are enums,
    /// so an unknown value cannot reach this check; the strings TSX sends are refused there.
    pub(crate) fn validate(&self) -> Result<(), LoggingError> {
        if self.max_file_size_bytes == Some(0)
            || self.max_total_size_bytes == Some(0)
            || self.write_timeout == Some(Duration::ZERO)
            || self.file_base_name.trim().is_empty()
        {
            return Err(LoggingError::new(ErrorKind::InvalidArgument));
        }
        #[cfg(not(target_arch = "wasm32"))]
        if self.directory.trim().is_empty() {
            return Err(LoggingError::new(ErrorKind::InvalidArgument));
        }
        Ok(())
    }
}

/// The default directory: a writable place every runner reaches without setup. The web has
/// no file storage, and an empty directory is only refused on native targets.
fn default_directory() -> String {
    #[cfg(target_arch = "wasm32")]
    {
        String::new()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::env::temp_dir()
            .join("nsh-logging")
            .to_string_lossy()
            .into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options() -> LoggingOptions {
        LoggingOptions {
            directory: "/tmp/nsh-logging-tests".to_owned(),
            ..LoggingOptions::default()
        }
    }

    #[test]
    fn the_defaults_are_the_documented_ones() {
        let options = LoggingOptions::default();
        assert_eq!(options.level, Level::Debug);
        assert_eq!(options.file_base_name, "log");
        assert_eq!(options.rotation, Rotation::Daily);
        assert_eq!(options.max_file_size_bytes, None);
        assert_eq!(options.max_total_size_bytes, None);
        assert_eq!(options.max_age, None);
        assert_eq!(options.write_timeout, None);
        assert_eq!(
            options.redact,
            DEFAULT_REDACT
                .iter()
                .map(|field| (*field).to_owned())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn zero_caps_and_timeouts_are_refused() {
        let mut options = options();
        options.max_file_size_bytes = Some(0);
        assert_eq!(
            options.validate().unwrap_err().code(),
            "ERR_LOGGING_INVALID_ARGUMENT"
        );
        options.max_file_size_bytes = None;
        options.max_total_size_bytes = Some(0);
        assert!(options.validate().is_err());
        options.max_total_size_bytes = None;
        options.write_timeout = Some(Duration::ZERO);
        assert!(options.validate().is_err());
        options.write_timeout = None;
        options.file_base_name = "  ".to_owned();
        assert!(options.validate().is_err());
        options.file_base_name = "log".to_owned();
        assert!(options.validate().is_ok());
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn an_empty_directory_is_refused_on_native_targets() {
        let options = LoggingOptions {
            directory: String::new(),
            ..options()
        };
        assert!(options.validate().is_err());
    }
}
