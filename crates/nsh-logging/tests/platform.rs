//! The platform path itself: `Logging::new` over the target's own backend, real files in
//! a real directory. The scripted-writer tests prove the rules; these prove the plumbing
//! — construction, the exit flush and both size-roll families on the writers that ship.

use std::{
    fs,
    path::{Path, PathBuf},
};

use nsh_logging::{Level, Logging, LoggingOptions, Rotation};

/// The defaults behind a function, so a test's field assignments are not the
/// default-then-reassign shape clippy flags; each test points it at its own directory.
fn options(directory: &Path) -> LoggingOptions {
    let mut options = LoggingOptions::default();
    options.directory = directory.to_string_lossy().into_owned();
    options
}

/// A per-test directory (test name + process id, so parallel runs never collide), empty
/// before the test starts and removed again when it ends — even when an assert fails.
struct DirectoryGuard(PathBuf);

impl DirectoryGuard {
    fn new(test: &str) -> Self {
        let path = std::env::temp_dir().join(format!("nsh-logging-{test}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for DirectoryGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// The directory's file names, sorted, so an assertion sees one order.
fn file_names(directory: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// The named file's lines, without the newline.
fn lines_of(directory: &Path, name: &str) -> Vec<String> {
    fs::read_to_string(directory.join(name))
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect()
}

/// One file's messages: the fixed format's level and timestamp fields dropped.
fn messages_of(directory: &Path, name: &str) -> Vec<String> {
    lines_of(directory, name)
        .iter()
        .map(|line| line.splitn(3, ' ').nth(2).unwrap_or_default().to_owned())
        .collect()
}

#[tokio::test]
async fn logging_new_writes_real_files_and_clean_removes_them() {
    let directory = DirectoryGuard::new("new-writes");
    let logger = Logging::new(&options(directory.path())).await.unwrap();
    let message = "a round trip through the platform's own writer";
    logger.info(message).await.unwrap();
    logger.flush().await.unwrap();

    let names = file_names(directory.path());
    assert_eq!(names.len(), 1, "one write in one interval makes one file");
    let lines = lines_of(directory.path(), &names[0]);
    assert_eq!(lines.len(), 1);
    let mut fields = lines[0].splitn(3, ' ');
    assert_eq!(
        fields.next(),
        Some("info"),
        "the line starts with the level"
    );
    assert!(
        fields
            .next()
            .is_some_and(|stamp| stamp.chars().all(|digit| digit.is_ascii_digit())),
        "the stamp follows: {}",
        lines[0]
    );
    assert_eq!(fields.next(), Some(message));

    let logs = logger.logs().await.unwrap();
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].message, message);
    assert_eq!(logs[0].level, Level::Info);

    logger.clean().await.unwrap();
    assert!(
        file_names(directory.path()).is_empty(),
        "clean deletes every file"
    );
}

#[tokio::test]
async fn dropping_the_logger_flushes_the_real_writer() {
    let directory = DirectoryGuard::new("drop-flushes");
    let logger = Logging::new(&options(directory.path())).await.unwrap();
    let messages = [
        "first line left to the exit flush",
        "second line left to the exit flush",
        "third line left to the exit flush",
    ];
    for message in messages {
        logger.info(message).await.unwrap();
    }
    // No flush call: the lines must reach the disk through the drop alone.
    drop(logger);

    let all_lines: Vec<String> = file_names(directory.path())
        .iter()
        .flat_map(|name| lines_of(directory.path(), name))
        .collect();
    for message in messages {
        assert!(
            all_lines.iter().any(|line| line.ends_with(message)),
            "{message} reached the disk only through the exit flush: {all_lines:?}"
        );
    }
}

/// The size cap rolls mid-interval by the platform family's rule on the writer that
/// ships: apple rolls ahead into `prefix.N`; linux and windows rename the full file
/// aside and restart the plain name. Never rotation keeps the names date-free.
#[tokio::test]
#[cfg(any(target_vendor = "apple", target_os = "linux", target_os = "windows"))]
async fn the_real_size_cap_rolls_mid_interval() {
    let directory = DirectoryGuard::new("size-rolls");
    let mut options = options(directory.path());
    options.rotation = Rotation::Never;
    options.max_file_size_bytes = Some(50);
    let logger = Logging::new(&options).await.unwrap();

    // A line is 24 bytes this decade — "info ", a 13-digit stamp, a space, a 4-character
    // message, a newline — so a 50-byte cap holds two lines and the third rolls.
    // The rows differ in length between the families, so the expected set is a Vec.
    #[cfg(target_vendor = "apple")]
    let expected: Vec<(&str, Vec<&str>)> = vec![
        ("log", vec!["m-01", "m-02"]),
        ("log.1", vec!["m-03", "m-04"]),
        ("log.2", vec!["m-05", "m-06"]),
    ];
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    let expected: Vec<(&str, Vec<&str>)> = vec![
        ("log", vec!["m-07"]),
        ("log.1", vec!["m-01", "m-02", "m-03"]),
        ("log.2", vec!["m-04", "m-05", "m-06"]),
    ];

    // A flush between writes puts the previous line in its file before the next roll
    // decision, so the distribution is deterministic despite the async worker.
    let writes: usize = expected.iter().map(|(_, lines)| lines.len()).sum();
    for index in 1..=writes {
        logger.info(&format!("m-{index:02}")).await.unwrap();
        logger.flush().await.unwrap();
    }

    assert_eq!(
        file_names(directory.path()),
        expected
            .iter()
            .map(|(name, _)| (*name).to_owned())
            .collect::<Vec<_>>(),
        "the rolled names follow the family rule"
    );
    for (name, messages) in expected {
        assert_eq!(
            messages_of(directory.path(), name),
            messages,
            "lines of {name}"
        );
    }
}
