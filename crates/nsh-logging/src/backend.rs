//! The seam between `Logging` and a platform sink: one line in, one flush, the deletes the
//! core's caps ask for, and the snapshot those caps read. Storage belongs to the backends;
//! policy belongs to the core.

use std::{future::Future, pin::Pin, time::SystemTime};

use crate::{error::LoggingError, logging::LogEntry};

/// A future a backend returns. Each call's future borrows the backend.
pub type BackendFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;

/// One file of the directory snapshot, as the core's cleanup policy sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileStat {
    /// The file's base name.
    pub name: String,
    /// Its size in bytes.
    pub size: u64,
    /// When it was last modified.
    pub modified: SystemTime,
}

/// What a platform provides. [`crate::Logging`] adds the rules: the severity filter, the
/// redaction, the write timeout and the size and age caps.
pub trait Backend: Send + Sync {
    /// Appends one line for `entry`, in the crate's fixed line format. Resolves once the
    /// line has been handed to the writer.
    fn append(&self, entry: &LogEntry) -> BackendFuture<'_, Result<(), LoggingError>>;

    /// Drains the writer, so every appended line is on disk.
    fn flush(&self) -> BackendFuture<'_, Result<(), LoggingError>>;

    /// Removes every named file. An empty list removes nothing.
    fn remove_all(&self, names: &[String]) -> BackendFuture<'_, Result<(), LoggingError>>;

    /// The stored files, in any order; the core sorts them oldest first.
    fn files(&self) -> BackendFuture<'_, Result<Vec<FileStat>, LoggingError>>;
}
