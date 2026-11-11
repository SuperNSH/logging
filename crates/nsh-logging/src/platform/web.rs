//! The web: no file sink of its own, so every storage call answers `NotAvailable`.
//!
//! A plain wasm32 crate reaches no local disk, and this module uses no filesystem API. For
//! file logging to work here a host would have to inject the `nsh_core::filesystem`
//! `FileSystem` capability — one `execute` seam covering write, list, stat and remove for
//! normalized relative paths under the app's private root, the web platform's OPFS-backed
//! implementation being one such host. Until that seam exists, each call resolves
//! `ERR_LOGGING_NOT_AVAILABLE`, the wording `docs/goals.md` gives the web.

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
