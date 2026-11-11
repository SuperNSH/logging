//! Windows: `tracing-appender`'s non-blocking writer over `std::fs`, in the per-user
//! roaming application-data directory the environment names.
//!
//! An empty `directory` option resolves to `%APPDATA%\nsh-logging\logs`, read from the
//! environment at use — a hard-coded drive or user path would not survive another account.

use std::{
    env,
    error::Error as _,
    ffi::OsStr,
    fs,
    future::ready,
    io,
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard, PoisonError},
    time::{SystemTime, UNIX_EPOCH},
};

use tracing_appender::{
    non_blocking::{NonBlocking, NonBlockingBuilder, WorkerGuard},
    rolling::{InitError, RollingFileAppender, Rotation as FileRotation},
};

use crate::{
    backend::{Backend, BackendFuture, FileStat},
    error::{ErrorKind, LoggingError},
    logging::{format_line, LogEntry},
    options::{LoggingOptions, Rotation},
};

const SECS_PER_MINUTE: i64 = 60;
const SECS_PER_HOUR: i64 = 60 * SECS_PER_MINUTE;
const SECS_PER_DAY: i64 = 24 * SECS_PER_HOUR;

/// The appender's handle plus the guard whose drop asks the worker to drain, within
/// tracing-appender's own timers; the drain itself is verified in `drain`.
struct WriterPair {
    writer: NonBlocking,
    _guard: WorkerGuard,
}

/// The live writer, or `None` once a flush or a roll drained it; the next append restarts it.
struct WriterState {
    pair: Option<WriterPair>,
    /// The bytes in the interval file the writer has open, queued lines included. A time
    /// roll inside the appender can only leave this higher than the file, never lower.
    written: u64,
    /// Bytes that must be on disk for a drain to succeed: the directory's total at start,
    /// plus every accepted line, minus every deleted file's size.
    handed: u64,
    /// The interval file those bytes belong to.
    file_name: String,
}

pub(crate) struct NativeBackend {
    directory: PathBuf,
    prefix: String,
    rotation: Rotation,
    max_file_size: Option<u64>,
    state: Mutex<WriterState>,
}

impl NativeBackend {
    /// Opens the directory (the platform default when the option is empty) and starts the
    /// non-blocking writer.
    pub(crate) fn new(options: &LoggingOptions) -> Result<Self, LoggingError> {
        let directory = resolve_directory(options)?;
        fs::create_dir_all(&directory).map_err(LoggingError::from_io)?;
        let backend = Self {
            directory,
            prefix: options.file_base_name.clone(),
            rotation: options.rotation,
            max_file_size: options.max_file_size_bytes,
            state: Mutex::new(WriterState {
                pair: None,
                written: 0,
                handed: 0,
                file_name: String::new(),
            }),
        };
        let mut state = backend.lock();
        backend.start_writer(&mut state)?;
        // What the directory already holds is owed from the start, so the first drain is
        // judged against the files earlier runs left too.
        state.handed = backend.bytes_on_disk()?;
        Ok(backend)
    }

    fn lock(&self) -> MutexGuard<'_, WriterState> {
        // A panic elsewhere cannot leave logging unusable; the guarded state is plain data.
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Starts a writer over the current interval file. The appender appends, so the size
    /// cap counts what earlier runs left in that file too.
    fn start_writer(&self, state: &mut WriterState) -> Result<(), LoggingError> {
        let file_name = interval_file_name(self.rotation, &self.prefix);
        let written = match fs::metadata(self.directory.join(&file_name)) {
            Ok(metadata) => metadata.len(),
            Err(error) if error.kind() == io::ErrorKind::NotFound => 0,
            Err(error) => return Err(LoggingError::from_io(error)),
        };
        let appender = RollingFileAppender::builder()
            .rotation(file_rotation(self.rotation))
            .filename_prefix(&self.prefix)
            .build(&self.directory)
            .map_err(init_error)?;
        // Lossy at the channel: a full buffer drops this one line where a blocking send
        // would hang the caller; append_line turns the counted drop into a coded failure.
        let (writer, guard) = NonBlockingBuilder::default()
            .lossy(true)
            .thread_name("nsh-logging")
            .finish(appender);
        state.pair = Some(WriterPair { writer, _guard: guard });
        state.written = written;
        state.file_name = file_name;
        Ok(())
    }

    /// Moves the full interval file aside and starts a fresh one. The pair drops first so
    /// the worker drains within its timers before the rename; if it cannot, its open
    /// handle follows the renamed file and the late lines still land there.
    fn roll_on_size(&self, state: &mut WriterState) -> Result<(), LoggingError> {
        state.pair = None;
        let from = self.directory.join(&state.file_name);
        match from.try_exists() {
            Ok(true) => {
                let aside = first_free_name(&self.directory, &state.file_name)?;
                fs::rename(&from, self.directory.join(aside)).map_err(LoggingError::from_io)?;
            }
            // The caps already deleted the file; the new writer recreates it.
            Ok(false) => {}
            Err(error) => return Err(LoggingError::from_io(error)),
        }
        self.start_writer(state)
    }

    /// Appends the entry's fixed-format line to the current interval file.
    fn append_line(&self, entry: &LogEntry) -> Result<(), LoggingError> {
        let mut state = self.lock();
        if state.pair.is_none() {
            self.start_writer(&mut state)?;
        }
        if self.max_file_size.is_some_and(|cap| state.written >= cap) {
            self.roll_on_size(&mut state)?;
        }
        let line = format!("{}\n", format_line(entry));
        let Some(pair) = state.pair.as_mut() else {
            // Every branch above leaves a live pair; this only satisfies the type.
            return Err(LoggingError::new(ErrorKind::Backend));
        };
        // The state mutex makes this backend the channel's only sender, so the counter's
        // rise across this write is this line's fate: a full buffer or a dead worker
        // drops the line and fails the append instead of blocking the caller forever.
        let dropped = pair.writer.error_counter().dropped_lines();
        pair.writer.write_all(line.as_bytes()).map_err(LoggingError::from_io)?;
        if pair.writer.error_counter().dropped_lines() > dropped {
            return Err(LoggingError::new(ErrorKind::Backend));
        }
        state.written = state.written.saturating_add(line.len() as u64);
        state.handed = state.handed.saturating_add(line.len() as u64);
        Ok(())
    }

    /// Drains the writer. Dropping the pair disconnects the channel, and a worker drains
    /// a disconnecting channel before it exits; the guard's own timers bound that wait at
    /// 1.1 seconds and the thread is never joined. The drain is then verified against the
    /// directory: bytes still missing fail with the timeout code, never a false success.
    fn drain(&self) -> Result<(), LoggingError> {
        let mut state = self.lock();
        state.pair = None;
        if self.bytes_on_disk()? >= state.handed {
            Ok(())
        } else {
            Err(LoggingError::new(ErrorKind::Timeout))
        }
    }

    /// The directory's total file size — what a drain is verified against.
    fn bytes_on_disk(&self) -> Result<u64, LoggingError> {
        Ok(self.list_files()?.iter().map(|file| file.size).sum())
    }

    /// The directory's files: every plain file, by name, size and last modification.
    fn list_files(&self) -> Result<Vec<FileStat>, LoggingError> {
        let mut files = Vec::new();
        for entry in fs::read_dir(&self.directory).map_err(LoggingError::from_io)? {
            let entry = entry.map_err(LoggingError::from_io)?;
            let Some(name) = entry.file_name().to_str() else {
                // Not a name this backend writes; not one of its files either.
                continue;
            };
            let metadata = entry.metadata().map_err(LoggingError::from_io)?;
            if !metadata.is_file() {
                continue;
            }
            files.push(FileStat {
                name: name.to_owned(),
                size: metadata.len(),
                modified: metadata.modified().map_err(LoggingError::from_io)?,
            });
        }
        Ok(files)
    }

    /// Deletes the named files, draining a pending batch first: deleted while still
    /// queued, its bytes would land in no file and no later flush could ever succeed.
    fn remove_files(&self, names: &[String]) -> Result<(), LoggingError> {
        let mut state = self.lock();
        if state.handed > self.bytes_on_disk()? {
            state.pair = None;
        }
        for name in names {
            self.remove_one(&mut state, name)?;
        }
        Ok(())
    }

    /// Deletes one named file, treating a file already gone as deleted. The bytes leave
    /// `handed` with the file, keeping the drain check truthful.
    fn remove_one(&self, state: &mut WriterState, name: &str) -> Result<(), LoggingError> {
        confined_name(name)?;
        let path = self.directory.join(name);
        let size = match fs::metadata(&path) {
            Ok(metadata) => metadata.len(),
            Err(error) if error.kind() == io::ErrorKind::NotFound => 0,
            Err(error) => return Err(LoggingError::from_io(error)),
        };
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(LoggingError::from_io(error)),
        }?;
        state.handed = state.handed.saturating_sub(size);
        Ok(())
    }
}

impl Backend for NativeBackend {
    fn append(&self, entry: &LogEntry) -> BackendFuture<'_, Result<(), LoggingError>> {
        Box::pin(ready(self.append_line(entry)))
    }

    fn flush(&self) -> BackendFuture<'_, Result<(), LoggingError>> {
        Box::pin(ready(self.drain()))
    }

    fn remove_all(&self, names: &[String]) -> BackendFuture<'_, Result<(), LoggingError>> {
        Box::pin(ready(self.remove_files(names)))
    }

    fn files(&self) -> BackendFuture<'_, Result<Vec<FileStat>, LoggingError>> {
        Box::pin(ready(self.list_files()))
    }
}

/// The directory the options name, or the platform default when they name none.
fn resolve_directory(options: &LoggingOptions) -> Result<PathBuf, LoggingError> {
    if !options.directory.trim().is_empty() {
        return Ok(PathBuf::from(&options.directory));
    }
    logs_dir(env::var_os("APPDATA").as_deref())
}

/// `%APPDATA%\nsh-logging\logs`: the roaming directory travels with the user's profile.
/// A session with no `APPDATA` (a bare service) has no per-user directory to offer.
fn logs_dir(appdata: Option<&OsStr>) -> Result<PathBuf, LoggingError> {
    let appdata = appdata
        .filter(|appdata| !appdata.is_empty())
        .ok_or_else(|| LoggingError::new(ErrorKind::InvalidArgument))?;
    Ok(Path::new(appdata).join("nsh-logging").join("logs"))
}

/// The builder's error carries the io error that caused it; that error keeps its kind.
fn init_error(error: InitError) -> LoggingError {
    match error.source().and_then(|source| source.downcast_ref::<io::Error>()) {
        Some(error) => LoggingError::from_io(error.clone()),
        None => LoggingError::new(ErrorKind::Backend),
    }
}

fn file_rotation(rotation: Rotation) -> FileRotation {
    match rotation {
        Rotation::Never => FileRotation::NEVER,
        Rotation::Minutely => FileRotation::MINUTELY,
        Rotation::Hourly => FileRotation::HOURLY,
        Rotation::Daily => FileRotation::DAILY,
    }
}

/// The name of the interval file a writer opens now.
fn interval_file_name(rotation: Rotation, prefix: &str) -> String {
    file_name_at(rotation, prefix, unix_seconds())
}

/// Seconds since the epoch; a clock set before 1970 reads as the epoch, like `now_ms`. The
/// cast saturates, so a clock far past 1970 cannot wrap either.
fn unix_seconds() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |passed| passed.as_secs() as i64)
}

/// The naming rule: the prefix, a dot, then the UTC date rounded down to the rotation's
/// interval — the exact spelling `tracing-appender` writes, which also uses UTC.
fn file_name_at(rotation: Rotation, prefix: &str, secs: i64) -> String {
    let (year, month, day) = civil_from_days(secs.div_euclid(SECS_PER_DAY));
    match rotation {
        Rotation::Never => prefix.to_owned(),
        Rotation::Daily => format!("{prefix}.{year:04}-{month:02}-{day:02}"),
        Rotation::Hourly | Rotation::Minutely => {
            let time_of_day = secs.rem_euclid(SECS_PER_DAY);
            let name = format!(
                "{prefix}.{year:04}-{month:02}-{day:02}-{:02}",
                time_of_day / SECS_PER_HOUR
            );
            if rotation == Rotation::Minutely {
                format!("{name}-{:02}", time_of_day / SECS_PER_MINUTE % SECS_PER_MINUTE)
            } else {
                name
            }
        }
    }
}

/// The civil date of a day count since 1970-01-01, Howard Hinnant's algorithm: the proleptic
/// Gregorian calendar the `time` crate names `tracing-appender`'s files by, leap days included.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = if shifted >= 0 { shifted } else { shifted - 146_096 } / 146_097;
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_pattern = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_pattern + 2) / 5 + 1;
    let month = if month_pattern < 10 { month_pattern + 3 } else { month_pattern - 9 };
    (if month <= 2 { year + 1 } else { year }, month as u32, day as u32)
}

/// `{name}.{index}` for the first index no file in the directory holds.
fn first_free_name(directory: &Path, name: &str) -> Result<String, LoggingError> {
    for index in 1..=u64::MAX {
        let candidate = format!("{name}.{index}");
        match directory.join(&candidate).try_exists() {
            Ok(false) => return Ok(candidate),
            Ok(true) => {}
            Err(error) => return Err(LoggingError::from_io(error)),
        }
    }
    // No directory holds u64-many files; this resolves rather than panics if one could.
    Err(LoggingError::new(ErrorKind::Backend))
}

/// Names come from this backend's own listing; this keeps a name from pointing a delete
/// outside the log directory.
fn confined_name(name: &str) -> Result<(), LoggingError> {
    if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\']) {
        return Err(LoggingError::new(ErrorKind::InvalidArgument));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interval_names_spell_dates_as_tracing_appender_does() {
        assert_eq!(file_name_at(Rotation::Never, "log", 0), "log");
        assert_eq!(file_name_at(Rotation::Daily, "log", 0), "log.1970-01-01");
        assert_eq!(file_name_at(Rotation::Hourly, "log", 0), "log.1970-01-01-00");
        assert_eq!(file_name_at(Rotation::Minutely, "log", 0), "log.1970-01-01-00-00");
        assert_eq!(file_name_at(Rotation::Minutely, "log", 1_582_981_500), "log.2020-02-29-13-05");
        assert_eq!(file_name_at(Rotation::Hourly, "log", 1_582_981_500), "log.2020-02-29-13");
    }

    #[test]
    fn february_twenty_ninth_exists_only_in_leap_years() {
        assert_eq!(file_name_at(Rotation::Daily, "log", 951_782_400), "log.2000-02-29");
        // 2100 is not a leap year: the day after its February 28 is March 1.
        assert_eq!(file_name_at(Rotation::Daily, "log", 4_107_456_000), "log.2100-02-28");
        assert_eq!(file_name_at(Rotation::Daily, "log", 4_107_542_400), "log.2100-03-01");
    }

    #[test]
    fn the_default_directory_sits_under_roaming_appdata() {
        let appdata = OsStr::new(r"C:\Users\u\AppData\Roaming");
        let directory = logs_dir(Some(appdata)).unwrap();
        assert_eq!(directory, PathBuf::from(r"C:\Users\u\AppData\Roaming\nsh-logging\logs"));
        let refused = logs_dir(Some(OsStr::new(""))).unwrap_err();
        assert_eq!(refused.code(), "ERR_LOGGING_INVALID_ARGUMENT");
        let refused = logs_dir(None).unwrap_err();
        assert_eq!(refused.code(), "ERR_LOGGING_INVALID_ARGUMENT");
    }

    #[test]
    fn a_name_that_could_leave_the_directory_is_refused() {
        assert_eq!(confined_name("log.2026-11-09"), Ok(()));
        for escaping in ["..", ".", "", "sub/log", "sub\\log", "../log"] {
            let refused = confined_name(escaping).unwrap_err();
            assert_eq!(refused.code(), "ERR_LOGGING_INVALID_ARGUMENT");
        }
    }

    #[test]
    fn appended_lines_drain_to_the_directory_and_survive_deletion() {
        use crate::options::Level;

        let directory = env::temp_dir().join(format!("nsh-logging-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        let options = LoggingOptions {
            directory: directory.to_string_lossy().into_owned(),
            ..LoggingOptions::default()
        };
        let backend = NativeBackend::new(&options).unwrap();
        let entry = LogEntry {
            time_ms: 0,
            level: Level::Debug,
            message: "drained".to_owned(),
        };
        backend.append_line(&entry).unwrap();
        backend.drain().unwrap();
        let files = backend.list_files().unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].name, interval_file_name(Rotation::Daily, "log"));
        let written = fs::read_to_string(directory.join(&files[0].name)).unwrap();
        assert_eq!(written, "debug 0 drained\n");
        // Deletion updates what a drain is judged against, so draining still succeeds.
        backend.remove_files(&[files[0].name.clone()]).unwrap();
        assert!(backend.list_files().unwrap().is_empty());
        backend.drain().unwrap();
        let _ = fs::remove_dir_all(&directory);
    }
}
