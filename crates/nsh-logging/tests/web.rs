//! A target with no file sink: every storage call answers that file logs are not
//! available here, exactly as the web backend does, through the public seam.

use std::future::ready;

use nsh_logging::{
    Backend, BackendFuture, ErrorKind, FileStat, LogEntry, Logging, LoggingError, LoggingOptions,
};

/// The defaults behind a function, so a test's field assignments are not the
/// default-then-reassign shape clippy flags; the fake never opens the directory.
fn options() -> LoggingOptions {
    LoggingOptions::default()
}

/// The one answer a target without a file sink has, as the crate's own web backend gives it.
#[derive(Clone, Copy, Default)]
struct NoFileSink;

impl Backend for NoFileSink {
    fn append(&self, _entry: &LogEntry) -> BackendFuture<'_, Result<(), LoggingError>> {
        unavailable()
    }

    fn flush(&self) -> BackendFuture<'_, Result<(), LoggingError>> {
        unavailable()
    }

    fn remove_all(&self, _names: &[String]) -> BackendFuture<'_, Result<(), LoggingError>> {
        unavailable()
    }

    fn files(&self) -> BackendFuture<'_, Result<Vec<FileStat>, LoggingError>> {
        unavailable()
    }
}

fn unavailable<T>() -> BackendFuture<'static, Result<T, LoggingError>> {
    Box::pin(ready(Err(LoggingError::new(ErrorKind::NotAvailable))))
}

#[tokio::test]
async fn a_target_without_a_file_sink_reports_not_available() {
    let logger = Logging::with_backend(NoFileSink, &options()).await.unwrap();

    let error = logger.info("nowhere to write").await.unwrap_err();
    assert_eq!(error.code(), "ERR_LOGGING_NOT_AVAILABLE");
    assert_eq!(logger.clean().await.unwrap_err().code(), "ERR_LOGGING_NOT_AVAILABLE");
    assert_eq!(logger.flush().await.unwrap_err().code(), "ERR_LOGGING_NOT_AVAILABLE");

    // History needs no storage, so reading it back stays an answer, an empty one.
    assert!(logger.logs().await.unwrap().is_empty());
}
