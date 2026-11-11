//! The transport seam: the fixed level names and line format, failures through
//! `log_exception`, and the structured entries a backend receives.

use std::{
    future::ready,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
};

use nsh_logging::{
    Backend, BackendFuture, FileStat, Level, LogEntry, Logging, LoggingError, LoggingOptions,
};

/// The defaults behind a function, so a test's field assignments are not the
/// default-then-reassign shape clippy flags; the fake never opens the directory.
fn options() -> LoggingOptions {
    LoggingOptions::default()
}

/// A backend that keeps the entries it was given, so a test sees the structured write.
#[derive(Clone, Default)]
struct RecordingBackend {
    received: Arc<Mutex<Vec<LogEntry>>>,
}

impl RecordingBackend {
    fn lock(&self) -> MutexGuard<'_, Vec<LogEntry>> {
        self.received.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Backend for RecordingBackend {
    fn append(&self, entry: &LogEntry) -> BackendFuture<'_, Result<(), LoggingError>> {
        self.lock().push(entry.clone());
        Box::pin(ready(Ok(())))
    }

    fn flush(&self) -> BackendFuture<'_, Result<(), LoggingError>> {
        Box::pin(ready(Ok(())))
    }

    fn remove_all(&self, _names: &[String]) -> BackendFuture<'_, Result<(), LoggingError>> {
        Box::pin(ready(Ok(())))
    }

    fn files(&self) -> BackendFuture<'_, Result<Vec<FileStat>, LoggingError>> {
        Box::pin(ready(Ok(Vec::new())))
    }
}

#[tokio::test]
async fn each_level_names_itself_and_the_filter_drops_lower_levels() {
    let backend = FakeBackend::new();
    let logger = Logging::with_backend(backend.clone(), &options()).await.unwrap();

    logger.debug("d").await.unwrap();
    logger.info("i").await.unwrap();
    logger.warning("w").await.unwrap();
    logger.error("e").await.unwrap();
    let lines = backend.lines();
    assert_eq!(lines.len(), 4);
    for (line, name) in lines.iter().zip(["debug", "info", "warning", "error"]) {
        assert_eq!(line.split(' ').next().unwrap(), name);
    }

    let strict_backend = FakeBackend::new();
    let mut strict_options = options();
    strict_options.level = Level::Warning;
    let strict = Logging::with_backend(strict_backend.clone(), &strict_options).await.unwrap();
    strict.debug("dropped").await.unwrap();
    strict.info("dropped").await.unwrap();
    assert_eq!(strict_backend.appends(), 0);
    strict.warning("kept").await.unwrap();
    strict.error("kept").await.unwrap();
    let kept = strict_backend.lines();
    assert_eq!(kept.len(), 2);
    for (line, name) in kept.iter().zip(["warning", "error"]) {
        assert_eq!(line.split(' ').next().unwrap(), name);
    }

    // talker's `verbose` and `critical` fold into the fixed set's ends: an unknown level
    // cannot be spelled here, and the four order themselves from debug to error.
    assert!(Level::Debug < Level::Info);
    assert!(Level::Info < Level::Warning);
    assert!(Level::Warning < Level::Error);
    let named = [
        (Level::Debug, "debug"),
        (Level::Info, "info"),
        (Level::Warning, "warning"),
        (Level::Error, "error"),
    ];
    for (level, name) in named {
        assert_eq!(level.name(), name);
    }
}

#[tokio::test]
async fn log_exception_records_the_failure_and_its_stack() {
    let backend = FakeBackend::new();
    let logger = Logging::with_backend(backend.clone(), &options()).await.unwrap();

    logger.log_exception("payment failed", Some("at checkout")).await.unwrap();
    logger.log_exception("bare failure", None).await.unwrap();

    let entries = logger.logs().await.unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].level, Level::Error);
    assert_eq!(entries[1].level, Level::Error);
    assert_eq!(entries[0].message, "payment failed\nat checkout");
    assert_eq!(entries[1].message, "bare failure");
    assert!(backend.lines()[0].contains("payment failed\nat checkout"));
}

#[tokio::test]
async fn the_written_line_carries_the_level_the_timestamp_and_the_message() {
    let backend = FakeBackend::new();
    let logger = Logging::with_backend(backend.clone(), &options()).await.unwrap();

    logger.info("ordered").await.unwrap();

    let entries = logger.logs().await.unwrap();
    assert_eq!(backend.lines(), [format!("info {} ordered", entries[0].time_ms)]);
}

#[tokio::test]
async fn the_backend_receives_structured_writes() {
    let backend = RecordingBackend::default();
    let mut options = options();
    options.redact = vec!["card".to_owned()];
    let logger = Logging::with_backend(backend.clone(), &options).await.unwrap();

    logger.info("card: 4242424242424242").await.unwrap();
    logger.error("burst").await.unwrap();

    let received = backend.lock().clone();
    assert_eq!(received.len(), 2);
    assert_eq!(received[0].level, Level::Info);
    assert_eq!(received[0].message, "card: [REDACTED]");
    assert!(received[0].time_ms > 0);
    assert_eq!(received[1].level, Level::Error);
    assert_eq!(received[1].message, "burst");
}
