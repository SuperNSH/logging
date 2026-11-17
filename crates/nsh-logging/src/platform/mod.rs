//! The platform backends. Each target gets one `NativeBackend`; a target with none, the web
//! included, reports that file logs are not available.
//!
//! A bridge module declares `NativeBackend` with
//! `pub(crate) fn new(&LoggingOptions) -> Result<Self, LoggingError>` — which opens the
//! directory and starts the writer — and implements [`Backend`], mapping filesystem errors
//! through `LoggingError::from_io`.

#[cfg(target_os = "android")]
mod android;
#[cfg(target_vendor = "apple")]
mod apple;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(not(any(
    target_vendor = "apple",
    target_os = "android",
    target_os = "linux",
    target_os = "windows",
    target_arch = "wasm32"
)))]
mod unsupported;
#[cfg(target_arch = "wasm32")]
mod web;
#[cfg(target_os = "windows")]
mod windows;

use std::{future::ready, sync::Arc};

use crate::{
    backend::{Backend, BackendFuture, FileStat},
    error::{ErrorKind, LoggingError},
    logging::LogEntry,
    options::LoggingOptions,
};

#[cfg(target_os = "android")]
use android::NativeBackend;
#[cfg(target_vendor = "apple")]
use apple::NativeBackend;
#[cfg(target_os = "linux")]
use linux::NativeBackend;
#[cfg(not(any(
    target_vendor = "apple",
    target_os = "android",
    target_os = "linux",
    target_os = "windows",
    target_arch = "wasm32"
)))]
use unsupported::NativeBackend;
#[cfg(target_arch = "wasm32")]
use web::NativeBackend;
#[cfg(target_os = "windows")]
use windows::NativeBackend;

/// The backend of the target this build runs on.
pub(crate) fn default_backend(options: &LoggingOptions) -> Result<Arc<dyn Backend>, LoggingError> {
    Ok(Arc::new(NativeBackend::new(options)?))
}

/// The backend of a target with no file sink: nothing opens, and every storage call
/// answers `NotAvailable` — the web and the unsupported targets share this one answer.
// Constructed only through those targets' aliases; a native target builds it for the
// test below alone, so the dead-code lint is quieted exactly there.
#[cfg_attr(
    any(
        target_vendor = "apple",
        target_os = "android",
        target_os = "linux",
        target_os = "windows"
    ),
    allow(dead_code)
)]
pub(crate) struct NoFileSink;

#[cfg_attr(
    any(
        target_vendor = "apple",
        target_os = "android",
        target_os = "linux",
        target_os = "windows"
    ),
    allow(dead_code)
)]
impl NoFileSink {
    /// Nothing to open, so construction always succeeds.
    pub(crate) fn new(_options: &LoggingOptions) -> Result<Self, LoggingError> {
        Ok(Self)
    }
}

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

/// The one answer a target without a file sink has: file logs are not available here.
#[cfg_attr(
    any(
        target_vendor = "apple",
        target_os = "android",
        target_os = "linux",
        target_os = "windows"
    ),
    allow(dead_code)
)]
fn unavailable<T: Send + 'static>() -> BackendFuture<'static, Result<T, LoggingError>> {
    Box::pin(ready(Err(LoggingError::new(ErrorKind::NotAvailable))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::Level;

    #[tokio::test]
    async fn the_no_file_sink_answers_not_available_for_every_call() {
        let sink = NoFileSink::new(&LoggingOptions::default()).unwrap();
        let entry = LogEntry {
            time_ms: 0,
            level: Level::Debug,
            message: String::new(),
        };

        let error = sink.append(&entry).await.unwrap_err();
        assert_eq!(error.kind(), ErrorKind::NotAvailable);
        assert_eq!(error.code(), "ERR_LOGGING_NOT_AVAILABLE");

        let error = sink.flush().await.unwrap_err();
        assert_eq!(error.kind(), ErrorKind::NotAvailable);
        assert_eq!(error.code(), "ERR_LOGGING_NOT_AVAILABLE");

        let error = sink
            .remove_all(&["log.2026-11-17".to_owned()])
            .await
            .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::NotAvailable);
        assert_eq!(error.code(), "ERR_LOGGING_NOT_AVAILABLE");

        let error = sink.files().await.unwrap_err();
        assert_eq!(error.kind(), ErrorKind::NotAvailable);
        assert_eq!(error.code(), "ERR_LOGGING_NOT_AVAILABLE");
    }
}
