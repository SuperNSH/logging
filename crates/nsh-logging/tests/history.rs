//! Reading back and clearing: `logs` answers the written entries oldest first up to the
//! bound, and `clean` empties the history and deletes the files.

use std::time::UNIX_EPOCH;

use nsh_logging::{FakeBackend, FileStat, Logging, LoggingOptions};

/// The defaults behind a function, so a test's field assignments are not the
/// default-then-reassign shape clippy flags; the fake never opens the directory.
fn options() -> LoggingOptions {
    LoggingOptions::default()
}

/// A stored file, as the directory snapshot answers it.
fn stored(name: &str) -> FileStat {
    FileStat {
        name: name.to_owned(),
        size: 30,
        modified: UNIX_EPOCH,
    }
}

#[tokio::test]
async fn logs_returns_the_recent_entries_in_order() {
    let backend = FakeBackend::new();
    let logger = Logging::with_backend(backend.clone(), &options())
        .await
        .unwrap();

    logger.debug("first").await.unwrap();
    logger.info("second").await.unwrap();
    logger.error("third").await.unwrap();

    let entries = logger.logs().await.unwrap();
    for (entry, message) in entries.iter().zip(["first", "second", "third"]) {
        assert_eq!(entry.message, message);
    }
    for (entry, level) in entries.iter().zip(["debug", "info", "error"]) {
        assert_eq!(entry.level.name(), level);
    }
    assert!(entries[1].time_ms >= entries[0].time_ms);
    assert!(entries[2].time_ms >= entries[1].time_ms);

    // The history keeps the last 1000 entries, so a flood sheds its oldest tail.
    let flood: Vec<_> = (0..1010)
        .map(|index| {
            let logger = logger.clone();
            tokio::spawn(async move { logger.debug(&format!("m{index}")).await })
        })
        .collect();
    for handle in flood {
        handle.await.unwrap().unwrap();
    }
    let recent = logger.logs().await.unwrap();
    assert_eq!(recent.len(), 1000);
    assert_eq!(recent[0].message, "m10");
    assert_eq!(recent[999].message, "m1009");
}

#[tokio::test]
async fn clean_empties_the_logs_and_the_files() {
    let backend = FakeBackend::new();
    backend.set_files(vec![stored("log.2026-11-08"), stored("log.2026-11-09")]);
    let logger = Logging::with_backend(backend.clone(), &options())
        .await
        .unwrap();

    logger.info("one").await.unwrap();
    logger.info("two").await.unwrap();
    assert_eq!(logger.logs().await.unwrap().len(), 2);

    logger.clean().await.unwrap();

    assert_eq!(backend.removed(), ["log.2026-11-08", "log.2026-11-09"]);
    assert_eq!(backend.removes(), 1);
    assert!(logger.logs().await.unwrap().is_empty());

    // Clearing is not closing: a write after clean lands and is read back.
    logger.info("after the clean").await.unwrap();
    let entries = logger.logs().await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].message, "after the clean");
}
