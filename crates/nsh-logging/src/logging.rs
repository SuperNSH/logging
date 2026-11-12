//! `Logging`: the calls an app makes. One write at a time, each resolved exactly once:
//! written, a coded failure, or dropped by the level filter without an error.

use std::{
    collections::VecDeque,
    fmt,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    task::{Context, Waker},
    time::{SystemTime, UNIX_EPOCH},
};

use tokio::sync::Mutex as WriteGate;

use crate::{
    backend::Backend,
    error::{ErrorKind, LoggingError},
    options::{Level, LoggingOptions},
    redact,
};

/// How many written entries `logs` keeps in memory. The files themselves are bounded by the
/// size and age caps, not by this.
const HISTORY_LIMIT: usize = 1000;

/// One written entry: what a backend receives and what `logs` reads back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogEntry {
    /// When the entry was written, in milliseconds since the epoch.
    pub time_ms: u64,
    /// The entry's level.
    pub level: Level,
    /// The message, already redacted.
    pub message: String,
}

/// The writer's fixed line format — `level timestamp message` — is not an option.
pub(crate) fn format_line(entry: &LogEntry) -> String {
    format!("{} {} {}", entry.level.name(), entry.time_ms, entry.message)
}

struct Shared {
    options: LoggingOptions,
    gate: WriteGate<()>,
    history: Mutex<VecDeque<LogEntry>>,
    backend: Arc<dyn Backend>,
}

impl Shared {
    fn history(&self) -> MutexGuard<'_, VecDeque<LogEntry>> {
        self.history.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The core's policy: delete the oldest files first until the directory is under both
    /// caps. With neither cap set there is nothing to prune and no call out to the backend.
    async fn enforce_caps(&self) -> Result<(), LoggingError> {
        let (max_total, max_age) = (self.options.max_total_size_bytes, self.options.max_age);
        if max_total.is_none() && max_age.is_none() {
            return Ok(());
        }
        let mut files = self.backend.files().await?;
        // Oldest first, ties by name, so what goes and what stays is deterministic.
        files.sort_by(|a, b| {
            a.modified
                .cmp(&b.modified)
                .then_with(|| a.name.cmp(&b.name))
        });
        let mut total: u64 = files.iter().map(|file| file.size).sum();
        let mut doomed = Vec::new();
        for file in &files {
            let over = max_total.is_some_and(|cap| total > cap);
            let expired =
                max_age.is_some_and(|limit| file.modified.elapsed().unwrap_or_default() > limit);
            if !over && !expired {
                // Sorted oldest first: no later file is over either cap.
                break;
            }
            total = total.saturating_sub(file.size);
            doomed.push(file.name.clone());
        }
        if doomed.is_empty() {
            Ok(())
        } else {
            self.backend.remove_all(&doomed).await
        }
    }
}

/// Local log files behind one API, with the names `talker` and `react-native-logs` made
/// familiar: levels, exception handling, history read-back and clearing. Clones share one
/// writer, and options are fixed at construction.
#[derive(Clone)]
pub struct Logging {
    shared: Arc<Shared>,
}

impl Logging {
    /// The platform's logger: validates the options, opens the directory, prunes what the
    /// caps refuse and starts the writer. On a target with no file sink, the calls that
    /// need storage resolve `ERR_LOGGING_NOT_AVAILABLE`.
    pub async fn new(options: &LoggingOptions) -> Result<Self, LoggingError> {
        options.validate()?;
        let backend = crate::platform::default_backend(options)?;
        Self::start(backend, options.clone()).await
    }

    /// A logger through `backend`, with its own gate and history. Tests use this.
    pub async fn with_backend(
        backend: impl Backend + 'static,
        options: &LoggingOptions,
    ) -> Result<Self, LoggingError> {
        options.validate()?;
        Self::start(Arc::new(backend), options.clone()).await
    }

    async fn start(
        backend: Arc<dyn Backend>,
        options: LoggingOptions,
    ) -> Result<Self, LoggingError> {
        let shared = Arc::new(Shared {
            options,
            gate: WriteGate::new(()),
            history: Mutex::new(VecDeque::new()),
            backend,
        });
        // Nothing can write yet: the construction prune needs no gate.
        shared.enforce_caps().await?;
        Ok(Self { shared })
    }

    /// Writes one message at `level`. A level below `LoggingOptions::level` is dropped and
    /// resolves `Ok`: the filter is not an error.
    pub async fn log(&self, message: &str, level: Level) -> Result<(), LoggingError> {
        if level < self.shared.options.level {
            return Ok(());
        }
        let entry = LogEntry {
            time_ms: now_ms(),
            level,
            message: redact::redact(message, &self.shared.options.redact),
        };
        self.write(entry).await
    }

    /// Writes at the debug level.
    pub async fn debug(&self, message: &str) -> Result<(), LoggingError> {
        self.log(message, Level::Debug).await
    }

    /// Writes at the info level.
    pub async fn info(&self, message: &str) -> Result<(), LoggingError> {
        self.log(message, Level::Info).await
    }

    /// Writes at the warning level.
    pub async fn warning(&self, message: &str) -> Result<(), LoggingError> {
        self.log(message, Level::Warning).await
    }

    /// Writes at the error level.
    pub async fn error(&self, message: &str) -> Result<(), LoggingError> {
        self.log(message, Level::Error).await
    }

    /// Records a failure and its stack, as talker's `handle` takes them, at the error level.
    pub async fn log_exception(
        &self,
        error: &(impl fmt::Display + ?Sized),
        stack: Option<&str>,
    ) -> Result<(), LoggingError> {
        let message = match stack {
            Some(stack) => format!("{error}\n{stack}"),
            None => format!("{error}"),
        };
        self.log(&message, Level::Error).await
    }

    /// The written entries, oldest first, up to the history bound.
    pub async fn logs(&self) -> Result<Vec<LogEntry>, LoggingError> {
        Ok(self.shared.history().iter().cloned().collect())
    }

    /// Empties the stored logs and deletes the files.
    pub async fn clean(&self) -> Result<(), LoggingError> {
        let _gate = self.shared.gate.lock().await;
        let names: Vec<String> = self
            .shared
            .backend
            .files()
            .await?
            .into_iter()
            .map(|file| file.name)
            .collect();
        self.shared.backend.remove_all(&names).await?;
        self.shared.history().clear();
        Ok(())
    }

    /// Drains the non-blocking writer, so every appended line is on disk. Call it before
    /// exit; dropping the last `Logging` flushes best-effort.
    pub async fn flush(&self) -> Result<(), LoggingError> {
        let _gate = self.shared.gate.lock().await;
        self.shared.backend.flush().await
    }

    async fn write(&self, entry: LogEntry) -> Result<(), LoggingError> {
        // tokio's timer needs a driver wasm32 has no runtime for; there the option is
        // accepted and unused, as the web writes nothing.
        #[cfg(not(target_arch = "wasm32"))]
        {
            if let Some(limit) = self.shared.options.write_timeout {
                return match tokio::time::timeout(limit, self.write_serialized(entry)).await {
                    Ok(result) => result,
                    Err(_elapsed) => Err(LoggingError::new(ErrorKind::Timeout)),
                };
            }
        }
        self.write_serialized(entry).await
    }

    /// One write at a time: the gate orders the append, the history and the cleanup, so
    /// concurrent writers cannot interleave them.
    async fn write_serialized(&self, entry: LogEntry) -> Result<(), LoggingError> {
        let _gate = self.shared.gate.lock().await;
        self.shared.backend.append(&entry).await?;
        self.record(entry);
        self.shared.enforce_caps().await
    }

    fn record(&self, entry: LogEntry) {
        let mut history = self.shared.history();
        history.push_back(entry);
        while history.len() > HISTORY_LIMIT {
            history.pop_front();
        }
    }
}

impl Drop for Shared {
    fn drop(&mut self) {
        // Best-effort and never panicking: one poll of the backend's flush, with no waker
        // and no waiting. The backend's own drop — its writer guard — flushes the rest.
        let mut flush = self.backend.flush();
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        let _ = flush.as_mut().poll(&mut context);
    }
}

/// When an entry was written. `SystemTime::now` can trap on wasm32, and the web never
/// writes, so entries there carry the epoch instead.
fn now_ms() -> u64 {
    #[cfg(target_arch = "wasm32")]
    let now = SystemTime::UNIX_EPOCH;
    #[cfg(not(target_arch = "wasm32"))]
    let now = SystemTime::now();
    now.duration_since(UNIX_EPOCH)
        .map(|passed| passed.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FakeBackend;

    fn options() -> LoggingOptions {
        LoggingOptions {
            directory: "/tmp/nsh-logging-tests".to_owned(),
            ..LoggingOptions::default()
        }
    }

    #[tokio::test]
    async fn a_write_below_the_level_is_dropped_without_reaching_the_backend() {
        let backend = FakeBackend::new();
        let strict = LoggingOptions {
            level: Level::Error,
            ..options()
        };
        let logger = Logging::with_backend(backend.clone(), &strict)
            .await
            .unwrap();
        logger.debug("quiet").await.unwrap();
        logger.error("loud").await.unwrap();
        assert_eq!(backend.appends(), 1);
        assert_eq!(backend.lines().len(), 1);
        let logs = logger.logs().await.unwrap();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].message, "loud");
        assert_eq!(logs[0].level, Level::Error);
    }

    #[tokio::test]
    async fn a_failure_is_recorded_with_its_stack_and_clean_empties_the_logs() {
        let backend = FakeBackend::new();
        let logger = Logging::with_backend(backend.clone(), &options())
            .await
            .unwrap();
        logger
            .log_exception(&LoggingError::new(ErrorKind::NotFound), Some("at payment"))
            .await
            .unwrap();
        let logs = logger.logs().await.unwrap();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].level, Level::Error);
        assert!(logs[0].message.contains("ERR_LOGGING_NOT_FOUND"));
        assert!(logs[0].message.contains("at payment"));

        logger.clean().await.unwrap();
        assert!(logger.logs().await.unwrap().is_empty());
        logger.flush().await.unwrap();
        assert_eq!(backend.flushes(), 1);
    }
}
