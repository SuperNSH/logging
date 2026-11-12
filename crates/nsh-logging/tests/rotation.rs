//! Time and size rotation through a scripted writer: the roll rules the platform backends
//! follow, with the clock injected so a boundary falls exactly where the test puts it.

use std::{
    future::ready,
    sync::{
        Arc, Mutex, MutexGuard, PoisonError,
        atomic::{AtomicU64, Ordering::SeqCst},
    },
    time::{Duration, UNIX_EPOCH},
};

use nsh_logging::{
    Backend, BackendFuture, FakeBackend, FileStat, LogEntry, Logging, LoggingError, LoggingOptions,
    Rotation,
};

const SECS_PER_MINUTE: i64 = 60;
const SECS_PER_HOUR: i64 = 60 * SECS_PER_MINUTE;
const SECS_PER_DAY: i64 = 24 * SECS_PER_HOUR;

/// The defaults behind a function, so a test's field assignments are not the
/// default-then-reassign shape clippy flags; the fake never opens the directory.
fn options() -> LoggingOptions {
    LoggingOptions::default()
}

/// One file the writer rolled: its name, its bytes, the instant it was created, its lines.
struct Rolled {
    name: String,
    size: u64,
    created_ms: u64,
    lines: Vec<String>,
}

/// Which side of the size rule a platform family takes when a file outgrows the cap
/// mid-interval: apple and android roll ahead into a suffixed file; linux and windows
/// rename the full file aside and restart the plain name.
#[derive(Clone, Copy)]
enum Family {
    Predictive,
    RenameAside,
}

#[derive(Default)]
struct WriterState {
    files: Vec<Rolled>,
    base: String,
    written: u64,
    roll: u32,
}

/// The scripted writer: the roll rules of the platform backends over a file map, with the
/// clock injected (`now_ms`, epoch milliseconds) so a test decides when an interval passes.
#[derive(Clone)]
struct IntervalWriter {
    state: Arc<Mutex<WriterState>>,
    prefix: &'static str,
    rotation: Rotation,
    now_ms: Arc<AtomicU64>,
    max_bytes: Option<u64>,
    family: Family,
}

impl IntervalWriter {
    fn new(
        prefix: &'static str,
        rotation: Rotation,
        now_ms: Arc<AtomicU64>,
        max_bytes: Option<u64>,
        family: Family,
    ) -> Self {
        Self {
            state: Arc::new(Mutex::new(WriterState::default())),
            prefix,
            rotation,
            now_ms,
            max_bytes,
            family,
        }
    }

    fn lock(&self) -> MutexGuard<'_, WriterState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn append_line(&self, entry: &LogEntry) {
        let now_ms = self.now_ms.load(SeqCst);
        let mut state = self.lock();
        // Clocks run past 1970 here; the cast mirrors the crate's own naming code.
        let base = file_name_at(self.rotation, self.prefix, (now_ms / 1000) as i64);
        if base != state.base {
            state.base = base;
            state.written = 0;
            state.roll = 0;
        }
        let line = format!(
            "{} {} {}\n",
            entry.level.name(),
            entry.time_ms,
            entry.message
        );
        match self.family {
            Family::Predictive => place_predictive(&mut state, &line, now_ms, self.max_bytes),
            Family::RenameAside => place_aside(&mut state, &line, now_ms, self.max_bytes),
        }
        state.written += line.len() as u64;
    }

    /// The rolled files' names, in the order this writer created them.
    fn names(&self) -> Vec<String> {
        self.lock()
            .files
            .iter()
            .map(|file| file.name.clone())
            .collect()
    }

    fn lines_of(&self, name: &str) -> Vec<String> {
        self.lock()
            .files
            .iter()
            .find(|file| file.name == name)
            .map(|file| file.lines.clone())
            .unwrap_or_default()
    }

    /// One file's messages: the fixed format's first two fields dropped.
    fn words_of(&self, name: &str) -> Vec<String> {
        self.lines_of(name)
            .iter()
            .map(|line| {
                let message = line.strip_suffix('\n').unwrap_or(line.as_str());
                message.split(' ').skip(2).collect::<Vec<_>>().join(" ")
            })
            .collect()
    }
}

impl Backend for IntervalWriter {
    fn append(&self, entry: &LogEntry) -> BackendFuture<'_, Result<(), LoggingError>> {
        self.append_line(entry);
        Box::pin(ready(Ok(())))
    }

    fn flush(&self) -> BackendFuture<'_, Result<(), LoggingError>> {
        Box::pin(ready(Ok(())))
    }

    fn remove_all(&self, names: &[String]) -> BackendFuture<'_, Result<(), LoggingError>> {
        self.lock().files.retain(|file| !names.contains(&file.name));
        Box::pin(ready(Ok(())))
    }

    fn files(&self) -> BackendFuture<'_, Result<Vec<FileStat>, LoggingError>> {
        let answer = self
            .lock()
            .files
            .iter()
            .map(|file| FileStat {
                name: file.name.clone(),
                size: file.size,
                modified: UNIX_EPOCH + Duration::from_millis(file.created_ms),
            })
            .collect();
        Box::pin(ready(Ok(answer)))
    }
}

/// The apple and android rule: the write that would outgrow the cap starts a fresh
/// `base.N` file, which carries its suffix from its first line on.
fn place_predictive(state: &mut WriterState, line: &str, now_ms: u64, max_bytes: Option<u64>) {
    if max_bytes.is_some_and(|cap| state.written > 0 && state.written + line.len() as u64 > cap) {
        state.roll += 1;
        state.written = 0;
    }
    let name = if state.roll > 0 {
        format!("{}.{}", state.base, state.roll)
    } else {
        state.base.clone()
    };
    place(&mut state.files, &name, line, now_ms);
}

/// The linux and windows rule: once the interval file is at the cap, the full file is
/// renamed aside to the first free `base.N` and the write restarts the plain name.
fn place_aside(state: &mut WriterState, line: &str, now_ms: u64, max_bytes: Option<u64>) {
    let at_cap = max_bytes.is_some_and(|cap| state.written >= cap);
    if at_cap && state.files.iter().any(|file| file.name == state.base) {
        let mut index = 1;
        while state
            .files
            .iter()
            .any(|file| file.name == format!("{}.{}", state.base, index))
        {
            index += 1;
        }
        let aside = format!("{}.{}", state.base, index);
        if let Some(rolled) = state.files.iter_mut().find(|file| file.name == state.base) {
            rolled.name = aside;
        }
        state.written = 0;
    }
    let base = state.base.clone();
    place(&mut state.files, &base, line, now_ms);
}

/// Adds one line to the named file, creating it with this instant when it is new.
fn place(files: &mut Vec<Rolled>, name: &str, line: &str, now_ms: u64) {
    if let Some(file) = files.iter_mut().find(|file| file.name == name) {
        file.lines.push(line.to_owned());
        file.size += line.len() as u64;
    } else {
        files.push(Rolled {
            name: name.to_owned(),
            size: line.len() as u64,
            created_ms: now_ms,
            lines: vec![line.to_owned()],
        });
    }
}

/// The naming rule both families share, as tracing-appender spells it: the prefix, a dot,
/// then the UTC date rounded down to the rotation's interval.
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
                format!(
                    "{name}-{:02}",
                    time_of_day / SECS_PER_MINUTE % SECS_PER_MINUTE
                )
            } else {
                name
            }
        }
    }
}

/// The civil date of a day count since 1970-01-01, Howard Hinnant's algorithm: the
/// proleptic Gregorian calendar tracing-appender's UTC clock names files by.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = if shifted >= 0 {
        shifted
    } else {
        shifted - 146_096
    } / 146_097;
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_pattern = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_pattern + 2) / 5 + 1;
    let month = if month_pattern < 10 {
        month_pattern + 3
    } else {
        month_pattern - 9
    };
    (
        if month <= 2 { year + 1 } else { year },
        month as u32,
        day as u32,
    )
}

/// A stored file, as the directory snapshot answers it.
fn rolled(name: &str, size: u64, modified_ms: u64) -> FileStat {
    FileStat {
        name: name.to_owned(),
        size,
        modified: UNIX_EPOCH + Duration::from_millis(modified_ms),
    }
}

#[tokio::test]
async fn time_rotation_rolls_to_a_new_file_at_the_interval() {
    // 2020-02-29T13:05:05Z: the leap-day instant the crate's own naming tests spell.
    let now = Arc::new(AtomicU64::new(1_582_981_505_000));
    let writer = IntervalWriter::new(
        "log",
        Rotation::Minutely,
        Arc::clone(&now),
        None,
        Family::Predictive,
    );
    let mut options = options();
    options.rotation = Rotation::Minutely;
    let logger = Logging::with_backend(writer.clone(), &options)
        .await
        .unwrap();

    logger.info("before the boundary").await.unwrap();
    now.fetch_add(60_000, SeqCst);
    logger.info("after the boundary").await.unwrap();

    assert_eq!(
        writer.names(),
        ["log.2020-02-29-13-05", "log.2020-02-29-13-06"]
    );
    assert_eq!(
        writer.words_of("log.2020-02-29-13-05"),
        ["before the boundary"]
    );
    assert_eq!(
        writer.words_of("log.2020-02-29-13-06"),
        ["after the boundary"]
    );
    assert_eq!(logger.logs().await.unwrap().len(), 2);
}

#[tokio::test]
async fn rotation_alone_bounds_nothing_until_caps_are_set() {
    let backend = FakeBackend::new();
    backend.set_files(vec![
        rolled("log.2026-11-09-12-2", 50, 1),
        rolled("log.2026-11-09-12-3", 50, 2),
        rolled("log.2026-11-09-12-4", 50, 3),
    ]);
    let mut options = options();
    options.rotation = Rotation::Minutely;
    let logger = Logging::with_backend(backend.clone(), &options)
        .await
        .unwrap();

    logger.info("a fifth file changes nothing").await.unwrap();

    assert!(backend.removed().is_empty());
    assert_eq!(backend.removes(), 0);
    // With no caps the directory is not even read: nothing decides what goes.
    assert_eq!(backend.files_reads(), 0);
    assert_eq!(logger.logs().await.unwrap().len(), 1);
}

#[tokio::test]
async fn size_cap_rolls_a_file_mid_interval() {
    // Fixed at 2020-02-29T13:05:05Z. The entry stamps are the real clock's — 13 digits
    // this decade — so each line is 25 or 26 bytes and a 45-byte cap rolls on the second.
    let fixed: u64 = 1_582_981_505_000;
    let base = file_name_at(Rotation::Minutely, "log", (fixed / 1000) as i64);
    let mut options = options();
    options.rotation = Rotation::Minutely;
    options.max_file_size_bytes = Some(45);

    let predictive = IntervalWriter::new(
        "log",
        Rotation::Minutely,
        Arc::new(AtomicU64::new(fixed)),
        Some(45),
        Family::Predictive,
    );
    let logger = Logging::with_backend(predictive.clone(), &options)
        .await
        .unwrap();
    for message in ["first", "second", "third"] {
        logger.info(message).await.unwrap();
    }
    assert_eq!(
        predictive.names(),
        vec![base.clone(), format!("{base}.1"), format!("{base}.2")]
    );
    assert_eq!(predictive.words_of(&base), ["first"]);
    assert_eq!(predictive.words_of(&format!("{base}.1")), ["second"]);
    assert_eq!(predictive.words_of(&format!("{base}.2")), ["third"]);

    let aside = IntervalWriter::new(
        "log",
        Rotation::Minutely,
        Arc::new(AtomicU64::new(fixed)),
        Some(45),
        Family::RenameAside,
    );
    let logger = Logging::with_backend(aside.clone(), &options)
        .await
        .unwrap();
    for message in ["first", "second", "third"] {
        logger.info(message).await.unwrap();
    }
    let mut names = aside.names();
    names.sort();
    assert_eq!(names, vec![base.clone(), format!("{base}.1")]);
    assert_eq!(aside.words_of(&format!("{base}.1")), ["first", "second"]);
    assert_eq!(aside.words_of(&base), ["third"]);
}
