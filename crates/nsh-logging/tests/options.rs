//! Options are fixed at construction: what is out of range is refused before the sink is
//! ever asked, and the construction level decides what is recorded from then on.

use std::time::Duration;

use nsh_logging::{ErrorKind, FakeBackend, Level, Logging, LoggingOptions};

/// The defaults behind a function, so a test's field assignments are not the
/// default-then-reassign shape clippy flags; the fake never opens the directory.
fn options() -> LoggingOptions {
    LoggingOptions::default()
}

#[tokio::test]
async fn invalid_options_are_refused_with_the_invalid_argument_code() {
    let mut zero_file_cap = options();
    zero_file_cap.max_file_size_bytes = Some(0);
    let mut zero_total_cap = options();
    zero_total_cap.max_total_size_bytes = Some(0);
    let mut zero_timeout = options();
    zero_timeout.write_timeout = Some(Duration::ZERO);
    let mut blank_base_name = options();
    blank_base_name.file_base_name = "   ".to_owned();
    let mut empty_directory = options();
    empty_directory.directory = String::new();

    // TypeScript's negative cap is unrepresentable here: the field is an unsigned count.
    let refused = [zero_file_cap, zero_total_cap, zero_timeout, blank_base_name, empty_directory];
    for invalid in refused {
        let backend = FakeBackend::new();
        let error = Logging::with_backend(backend.clone(), &invalid).await.unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidArgument);
        assert_eq!(error.code(), "ERR_LOGGING_INVALID_ARGUMENT");
        // The refusal happened before the sink was ever asked.
        assert_eq!(backend.appends(), 0);
        assert_eq!(backend.flushes(), 0);
        assert_eq!(backend.removes(), 0);
        assert_eq!(backend.files_reads(), 0);
    }

    let backend = FakeBackend::new();
    Logging::with_backend(backend.clone(), &options()).await.unwrap();
    assert_eq!(backend.files_reads(), 0);
}

#[tokio::test]
async fn a_higher_construction_level_drops_lower_writes() {
    let backend = FakeBackend::new();
    let mut strict = options();
    strict.level = Level::Error;
    let logger = Logging::with_backend(backend.clone(), &strict).await.unwrap();

    // Dropped writes resolve: the filter is not an error, and the backend sees nothing.
    logger.debug("quiet").await.unwrap();
    logger.info("quiet").await.unwrap();
    logger.warning("quiet").await.unwrap();
    assert_eq!(backend.appends(), 0);
    assert!(logger.logs().await.unwrap().is_empty());

    logger.error("loud").await.unwrap();
    assert_eq!(backend.lines().len(), 1);
    let entries = logger.logs().await.unwrap();
    assert_eq!(entries[0].level, Level::Error);
    assert_eq!(entries[0].message, "loud");
}
