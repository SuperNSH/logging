//! Apple (iOS and macOS): real files under the app's Application Support directory, written
//! through tracing-appender's non-blocking rolling writer.

use std::{
    env,
    error::Error as _,
    fs,
    future::ready,
    io::{self, Write as _},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use tracing_appender::{
    non_blocking::{ErrorCounter, NonBlocking, NonBlockingBuilder, WorkerGuard},
    rolling::{InitError, RollingFileAppender, Rotation as FileRotation},
};

use crate::{
    backend::{Backend, BackendFuture, FileStat},
    error::{ErrorKind, LoggingError},
    logging::{LogEntry, format_line},
    options::{LoggingOptions, Rotation},
};

/// The backend's own directory under the platform's storage root, so the directory it
/// lists and clears holds only its files.
const DIRECTORY_NAME: &str = "nsh-logging";
/// How long a flush waits for the worker to drain; tracing-appender's own shutdown flush
/// waits one second, so this matches it.
const DRAIN_TIMEOUT: Duration = Duration::from_secs(1);
/// How often a waiting flush rechecks the worker's progress.
const DRAIN_POLL: Duration = Duration::from_millis(2);

/// The platform's file sink: one directory, one non-blocking writer, and the deletes and
/// the snapshot the core's caps ask for.
pub(crate) struct NativeBackend {
    directory: PathBuf,
    state: Mutex<SinkState>,
    signal: Arc<DrainSignal>,
}

/// The writer's state. `non_blocking` hands lines to tracing-appender's worker thread and
/// `_guard` keeps that worker draining until the backend is dropped; it is held, never read.
struct SinkState {
    non_blocking: NonBlocking,
    _guard: WorkerGuard,
    /// The writer's drop counter, watched by [`SinkState::send`] so a full queue fails
    /// the write with a coded error instead of blocking or losing the line silently.
    dropped: ErrorCounter,
    appender: Arc<Mutex<RollingFileAppender>>,
    prefix: String,
    rotation: Rotation,
    bucket: u64,
    roll: u32,
    written: u64,
    max_file_size: Option<u64>,
    /// Set when a removal deleted the writer's own file, whose open handle would then
    /// write to a deleted inode; the next append swaps in a fresh appender.
    reopen_pending: bool,
}

impl SinkState {
    /// Sends `bytes` without ever blocking: the lossy writer drops the line when its
    /// bounded queue is full, and the drop counter turns that into a coded failure —
    /// never a silent loss, and never the unblockable wait of a full-queue send.
    fn send(&mut self, bytes: &[u8]) -> Result<(), LoggingError> {
        let before = self.dropped.dropped_lines();
        self.non_blocking
            .write_all(bytes)
            .map_err(LoggingError::from_io)?;
        if self.dropped.dropped_lines() > before {
            return Err(LoggingError::new(ErrorKind::Backend));
        }
        Ok(())
    }
}

impl NativeBackend {
    /// Opens the directory, creating it on demand, and starts the writer.
    pub(crate) fn new(options: &LoggingOptions) -> Result<Self, LoggingError> {
        let directory = resolve(&options.directory);
        fs::create_dir_all(&directory).map_err(LoggingError::from_io)?;
        let signal = Arc::new(DrainSignal::default());
        let state = start(options, &directory, &signal)?;
        Ok(Self {
            directory,
            state: Mutex::new(state),
            signal,
        })
    }

    fn lock(&self) -> MutexGuard<'_, SinkState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Appends one line, rolling mid-interval when the size cap says so.
    fn append_line(&self, entry: &LogEntry) -> Result<(), LoggingError> {
        let mut state = self.lock();
        let mut line = format_line(entry);
        line.push('\n');
        // The appender rolls by its own UTC clock; the bucket only spots the interval
        // change so the roll counter and the byte count start the new file fresh.
        let bucket = interval_bucket(state.rotation);
        if bucket != state.bucket {
            state.bucket = bucket;
            state.roll = 0;
            state.written = 0;
        }
        // A line longer than the cap still lands, so rolling cannot loop on one line.
        if state
            .max_file_size
            .is_some_and(|cap| state.written > 0 && state.written + line.len() as u64 > cap)
        {
            state.roll = state.roll.saturating_add(1);
            reopen(&mut state, &self.directory)?;
        } else if state.reopen_pending {
            reopen(&mut state, &self.directory)?;
        }
        state.send(line.as_bytes())?;
        state.written += line.len() as u64;
        Ok(())
    }

    /// Enqueues a marker on the worker's queue and waits for the worker to pass it; the
    /// queue is FIFO, so every line before the marker is in the file by then. The wait is
    /// bounded: a stuck worker answers Timeout instead of hanging.
    fn drain(&self) -> Result<(), LoggingError> {
        let mut state = self.lock();
        let deadline = Instant::now() + DRAIN_TIMEOUT;
        let marker = self.signal.enqueue();
        state.send(&[])?;
        while !self.signal.observed(marker) {
            if Instant::now() >= deadline {
                return Err(LoggingError::new(ErrorKind::Timeout));
            }
            std::thread::sleep(DRAIN_POLL);
        }
        Ok(())
    }

    /// The directory's regular files in `read_dir`'s order; the core sorts them.
    fn stored_files(&self) -> Result<Vec<FileStat>, LoggingError> {
        let entries = fs::read_dir(&self.directory).map_err(LoggingError::from_io)?;
        let mut files = Vec::new();
        for entry in entries {
            let entry = entry.map_err(LoggingError::from_io)?;
            let metadata = entry.metadata().map_err(LoggingError::from_io)?;
            if !metadata.is_file() {
                continue;
            }
            files.push(FileStat {
                name: entry.file_name().to_string_lossy().into_owned(),
                size: metadata.len(),
                // A file whose times the platform will not tell reads as oldest.
                modified: metadata.modified().unwrap_or(UNIX_EPOCH),
            });
        }
        Ok(files)
    }

    /// Removes the named files. The writer's own file may be among them, so its removal
    /// is remembered: the next append reopens instead of writing on to a deleted file's
    /// inode.
    fn remove_files(&self, names: &[String]) -> Result<(), LoggingError> {
        if names.is_empty() {
            return Ok(());
        }
        let mut state = self.lock();
        let dotted = format!("{}.", state.prefix);
        for name in names {
            if name.as_str() == state.prefix || name.starts_with(&dotted) {
                state.reopen_pending = true;
            }
            match fs::remove_file(self.directory.join(name)) {
                Ok(()) => {}
                // Already gone satisfies the removal.
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(LoggingError::from_io(error)),
            }
        }
        Ok(())
    }
}

impl Backend for NativeBackend {
    fn append(&self, entry: &LogEntry) -> BackendFuture<'_, Result<(), LoggingError>> {
        let answer = self.append_line(entry);
        Box::pin(ready(answer))
    }

    fn flush(&self) -> BackendFuture<'_, Result<(), LoggingError>> {
        let answer = self.drain();
        Box::pin(ready(answer))
    }

    fn remove_all(&self, names: &[String]) -> BackendFuture<'_, Result<(), LoggingError>> {
        let answer = self.remove_files(names);
        Box::pin(ready(answer))
    }

    fn files(&self) -> BackendFuture<'_, Result<Vec<FileStat>, LoggingError>> {
        let answer = self.stored_files();
        Box::pin(ready(answer))
    }
}

/// The directory the backend writes to when the app chose none: Application Support under
/// `HOME`, which the sandbox points at the app's own container, with the per-app temporary
/// directory the OS still names when `HOME` is unset as the fallback.
fn resolve(directory: &str) -> PathBuf {
    if directory == LoggingOptions::default().directory {
        return root().join(DIRECTORY_NAME);
    }
    let chosen = Path::new(directory);
    if chosen.is_absolute() {
        chosen.to_path_buf()
    } else {
        // A relative choice is anchored at the app's storage, not at an undefined cwd.
        root().join(chosen)
    }
}

fn root() -> PathBuf {
    match env::var_os("HOME") {
        Some(home) if !home.is_empty() => Path::new(&home).join("Library/Application Support"),
        _ => env::temp_dir(),
    }
}

/// Opens the writer over the directory's current interval file. `written` counts this
/// session's bytes, so a file carried over from an earlier run rolls on new bytes alone.
fn start(
    options: &LoggingOptions,
    directory: &Path,
    signal: &Arc<DrainSignal>,
) -> Result<SinkState, LoggingError> {
    let appender = Arc::new(Mutex::new(build(
        directory,
        &options.file_base_name,
        options.rotation,
        None,
    )?));
    // Lossy so a full queue never blocks: `send` checks the drop counter and answers a
    // coded error for a dropped line instead of wedging a poll no timeout can preempt.
    let (non_blocking, guard) = NonBlockingBuilder::default()
        .lossy(true)
        .finish(DrainWriter {
            appender: Arc::clone(&appender),
            signal: Arc::clone(signal),
        });
    let dropped = non_blocking.error_counter();
    Ok(SinkState {
        non_blocking,
        _guard: guard,
        dropped,
        appender,
        prefix: options.file_base_name.clone(),
        rotation: options.rotation,
        bucket: interval_bucket(options.rotation),
        roll: 0,
        written: 0,
        max_file_size: options.max_file_size_bytes,
        reopen_pending: false,
    })
}

/// Builds the rolling appender for the current interval. The suffix names a mid-interval
/// size-rolled file `prefix.<date>.N`, or `prefix.N` with no rotation.
fn build(
    directory: &Path,
    prefix: &str,
    rotation: Rotation,
    roll: Option<u32>,
) -> Result<RollingFileAppender, LoggingError> {
    let mut builder = RollingFileAppender::builder()
        .rotation(file_rotation(rotation))
        .filename_prefix(prefix);
    if let Some(roll) = roll {
        builder = builder.filename_suffix(roll.to_string());
    }
    builder.build(directory).map_err(init_error)
}

/// Swaps in a fresh appender for `state.roll`; the worker's next line opens the new file
/// and the old handle drops closed.
fn reopen(state: &mut SinkState, directory: &Path) -> Result<(), LoggingError> {
    let roll = (state.roll > 0).then_some(state.roll);
    let fresh = build(directory, &state.prefix, state.rotation, roll)?;
    *locked_appender(&state.appender) = fresh;
    state.written = 0;
    state.reopen_pending = false;
    Ok(())
}

fn file_rotation(rotation: Rotation) -> FileRotation {
    match rotation {
        Rotation::Never => FileRotation::NEVER,
        Rotation::Minutely => FileRotation::MINUTELY,
        Rotation::Hourly => FileRotation::HOURLY,
        Rotation::Daily => FileRotation::DAILY,
    }
}

/// The builder wraps the io error that says why the file could not open; the shared
/// mapping reads it back out.
fn init_error(error: InitError) -> LoggingError {
    match error
        .source()
        .and_then(|source| source.downcast_ref::<io::Error>())
    {
        // `io::Error` is not `Clone`; rebuild one carrying the kind and OS code `from_io` reads.
        Some(error) => LoggingError::from_io(match error.raw_os_error() {
            Some(code) => io::Error::from_raw_os_error(code),
            None => io::Error::from(error.kind()),
        }),
        None => LoggingError::new(ErrorKind::Backend),
    }
}

/// The interval's bucket number. The appender's own UTC clock names the files; this only
/// spots boundaries, so one number per interval is enough.
fn interval_bucket(rotation: Rotation) -> u64 {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |passed| passed.as_secs());
    match rotation {
        Rotation::Never => 0,
        Rotation::Daily => seconds / 86_400,
        Rotation::Hourly => seconds / 3_600,
        Rotation::Minutely => seconds / 60,
    }
}

/// The writer tracing-appender's worker owns. An empty buffer is this backend's flush
/// marker: the worker drains the queue in order, so reaching the marker means every line
/// before it is in the file.
struct DrainWriter {
    appender: Arc<Mutex<RollingFileAppender>>,
    signal: Arc<DrainSignal>,
}

impl io::Write for DrainWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            self.signal.mark();
            return Ok(0);
        }
        locked_appender(&self.appender)
            .write_all(buf)
            .map(|()| buf.len())
    }

    // std's write_all skips `write` for empty buffers, which would swallow the marker.
    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        if buf.is_empty() {
            self.signal.mark();
            return Ok(());
        }
        locked_appender(&self.appender).write_all(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        locked_appender(&self.appender).flush()
    }
}

/// Flush agreement between the backend and the worker: the markers enqueued and the
/// markers observed.
#[derive(Default)]
struct DrainSignal {
    markers: Mutex<(u64, u64)>,
}

impl DrainSignal {
    /// The marker that rides the queue next. Only the writer's holder enqueues, so
    /// markers cannot overtake each other.
    fn enqueue(&self) -> u64 {
        let mut markers = self.lock();
        markers.0 += 1;
        markers.0
    }

    /// The worker, on an empty write: everything enqueued before now is in the file.
    fn mark(&self) {
        let mut markers = self.lock();
        markers.1 = markers.0;
    }

    fn observed(&self, marker: u64) -> bool {
        self.lock().1 >= marker
    }

    fn lock(&self) -> MutexGuard<'_, (u64, u64)> {
        self.markers.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn locked_appender(
    appender: &Arc<Mutex<RollingFileAppender>>,
) -> MutexGuard<'_, RollingFileAppender> {
    appender.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shared_default_moves_home_and_chosen_directories_stay() {
        let home = resolve(&LoggingOptions::default().directory);
        assert!(home.ends_with(DIRECTORY_NAME));

        let chosen = resolve("/tmp/nsh-logging-apple-tests");
        assert_eq!(chosen, PathBuf::from("/tmp/nsh-logging-apple-tests"));

        let anchored = resolve("logs");
        assert!(anchored.is_absolute() && anchored.ends_with("logs"));
    }

    #[test]
    fn the_faster_the_rotation_the_larger_the_bucket() {
        assert_eq!(interval_bucket(Rotation::Never), 0);
        let daily = interval_bucket(Rotation::Daily);
        let hourly = interval_bucket(Rotation::Hourly);
        let minutely = interval_bucket(Rotation::Minutely);
        assert!(daily > 0 && hourly >= daily && minutely >= hourly);
    }
}
