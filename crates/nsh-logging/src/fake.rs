//! A backend with scripted answers. Tests use it, and so can an app that logs into memory.

use std::{
    future::ready,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
};

use crate::{
    backend::{Backend, BackendFuture, FileStat},
    error::LoggingError,
    logging::{format_line, LogEntry},
};

/// A backend whose failures the test scripts, whose lines the test reads, and whose calls
/// the test counts. Clones share one state.
#[derive(Clone)]
pub struct FakeBackend {
    state: Arc<Mutex<FakeState>>,
}

#[derive(Default)]
struct FakeState {
    next_append_error: Option<LoggingError>,
    next_flush_error: Option<LoggingError>,
    next_remove_error: Option<LoggingError>,
    next_files_error: Option<LoggingError>,
    files: Vec<FileStat>,
    lines: Vec<String>,
    removed: Vec<String>,
    appends: usize,
    flushes: usize,
    removes: usize,
    files_reads: usize,
}

impl Default for FakeBackend {
    /// A backend that succeeds at everything, over an empty directory.
    fn default() -> Self {
        Self::new()
    }
}

impl FakeBackend {
    /// A backend with no scripted failures, an empty directory and empty buffers.
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(FakeState::default())),
        }
    }

    fn lock(&self) -> MutexGuard<'_, FakeState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Makes the next `append` fail with `error`, once.
    pub fn fail_next_append(&self, error: LoggingError) {
        self.lock().next_append_error = Some(error);
    }

    /// Makes the next `flush` fail with `error`, once.
    pub fn fail_next_flush(&self, error: LoggingError) {
        self.lock().next_flush_error = Some(error);
    }

    /// Makes the next `remove_all` fail with `error`, once.
    pub fn fail_next_remove(&self, error: LoggingError) {
        self.lock().next_remove_error = Some(error);
    }

    /// Makes the next `files` fail with `error`, once.
    pub fn fail_next_files(&self, error: LoggingError) {
        self.lock().next_files_error = Some(error);
    }

    /// Sets what `files` answers: the directory snapshot the core's caps read.
    pub fn set_files(&self, files: Vec<FileStat>) {
        self.lock().files = files;
    }

    /// The lines the backend received, in the crate's fixed format, oldest first.
    pub fn lines(&self) -> Vec<String> {
        self.lock().lines.clone()
    }

    /// The names the core asked to remove, in the order it asked for them.
    pub fn removed(&self) -> Vec<String> {
        self.lock().removed.clone()
    }

    /// How many lines were appended.
    pub fn appends(&self) -> usize {
        self.lock().appends
    }

    /// How many times the writer was flushed.
    pub fn flushes(&self) -> usize {
        self.lock().flushes
    }

    /// How many times files were removed.
    pub fn removes(&self) -> usize {
        self.lock().removes
    }

    /// How many times the directory was asked for.
    pub fn files_reads(&self) -> usize {
        self.lock().files_reads
    }
}

impl Backend for FakeBackend {
    fn append(&self, entry: &LogEntry) -> BackendFuture<'_, Result<(), LoggingError>> {
        let mut state = self.lock();
        if let Some(error) = state.next_append_error.take() {
            return Box::pin(ready(Err(error)));
        }
        state.lines.push(format_line(entry));
        state.appends += 1;
        Box::pin(ready(Ok(())))
    }

    fn flush(&self) -> BackendFuture<'_, Result<(), LoggingError>> {
        let mut state = self.lock();
        let answer: Result<(), LoggingError> = state.next_flush_error.take().map_or(Ok(()), Err);
        state.flushes += 1;
        Box::pin(ready(answer))
    }

    fn remove_all(&self, names: &[String]) -> BackendFuture<'_, Result<(), LoggingError>> {
        let mut state = self.lock();
        if let Some(error) = state.next_remove_error.take() {
            return Box::pin(ready(Err(error)));
        }
        state.removed.extend(names.iter().cloned());
        state.removes += 1;
        Box::pin(ready(Ok(())))
    }

    fn files(&self) -> BackendFuture<'_, Result<Vec<FileStat>, LoggingError>> {
        let mut state = self.lock();
        if let Some(error) = state.next_files_error.take() {
            return Box::pin(ready(Err(error)));
        }
        state.files_reads += 1;
        Box::pin(ready(Ok(state.files.clone())))
    }
}
