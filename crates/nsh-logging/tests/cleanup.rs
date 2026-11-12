//! The caps: `max_total_size_bytes` and `max_age` delete the oldest files first — after
//! construction and after each write — and keep everything the caps still allow.

use std::time::{Duration, SystemTime};

use nsh_logging::{FakeBackend, FileStat, Logging, LoggingOptions};

/// The defaults behind a function, so a test's field assignments are not the
/// default-then-reassign shape clippy flags; the fake never opens the directory.
fn options() -> LoggingOptions {
    LoggingOptions::default()
}

/// A stored file `age_ms` old, as the directory snapshot answers it.
fn stored(name: &str, size: u64, age_ms: u64) -> FileStat {
    FileStat {
        name: name.to_owned(),
        size,
        modified: SystemTime::now() - Duration::from_millis(age_ms),
    }
}

#[tokio::test]
async fn max_total_size_bytes_deletes_the_oldest_files_first() {
    let backend = FakeBackend::new();
    let mut options = options();
    options.max_total_size_bytes = Some(120);
    let logger = Logging::with_backend(backend.clone(), &options)
        .await
        .unwrap();
    // The construction prune read an empty directory: one read, nothing to remove yet.
    assert_eq!(backend.files_reads(), 1);

    // Deliberately shuffled: the core sorts oldest first before it chooses. 150 bytes are
    // stored, the cap is 120, so the oldest 100 go and 50 stay under it.
    backend.set_files(vec![
        stored("log.2026-11-09-21", 40, 2_000),
        stored("log.2026-11-08", 100, 3_000),
        stored("log.2026-11-09-22", 10, 1_000),
    ]);

    logger.info("one more line").await.unwrap();

    assert_eq!(backend.removed(), ["log.2026-11-08"]);
    assert_eq!(backend.removes(), 1);
    assert_eq!(
        backend.files_reads(),
        2,
        "the caps ran again after the write"
    );
    assert_eq!(logger.logs().await.unwrap().len(), 1);
}

#[tokio::test]
async fn max_age_deletes_files_older_than_the_limit() {
    let backend = FakeBackend::new();
    let mut options = options();
    options.max_age = Some(Duration::from_millis(1_500));
    let logger = Logging::with_backend(backend.clone(), &options)
        .await
        .unwrap();

    backend.set_files(vec![
        stored("log.2026-11-09", 20, 500),
        stored("log.2026-11-07", 20, 5_000),
    ]);

    logger.info("the fresh file stays").await.unwrap();

    assert_eq!(backend.removed(), ["log.2026-11-07"]);
    assert_eq!(backend.removes(), 1);
    assert_eq!(logger.logs().await.unwrap().len(), 1);
}
