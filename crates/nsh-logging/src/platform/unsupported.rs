//! A target with no file sink: every storage call answers that file logs are not
//! available.

use std::future::ready;

use crate::{
    backend::{Backend, BackendFuture, FileStat},
    error::{ErrorKind, LoggingError},
    logging::LogEntry,
    options::LoggingOptions,
};

pub(crate) struct NativeBackend;

impl NativeBackend {
    /// Nothing to open, so construction always succeeds; the calls answer NotAvailable.
    pub(crate) fn new(_options: &LoggingOptions) -> Result<Self, LoggingError> {
        Ok(Self)
    }
}

impl Backend for NativeBackend {
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

/// The one answer this backend has: file logs are not available here.
fn unavailable<T>() -> BackendFuture<'static, Result<T, LoggingError>> {
    Box::pin(ready(Err(LoggingError::new(ErrorKind::NotAvailable))))
}
