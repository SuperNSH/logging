//! The error model: eight kinds, each with its stable `ERR_LOGGING_` code and its
//! one-sentence message, and the storage failures that reach a caller with their code.

use nsh_logging::{ErrorKind, FakeBackend, Logging, LoggingError, LoggingOptions};

/// The defaults behind a function, so a test's field assignments are not the
/// default-then-reassign shape clippy flags; the fake never opens the directory.
fn options() -> LoggingOptions {
    LoggingOptions::default()
}

/// The eight kinds and their codes and messages, as docs/goals.md's table spells them.
const ALL_KINDS: [(ErrorKind, &str, &str); 8] = [
    (
        ErrorKind::NotAvailable,
        "ERR_LOGGING_NOT_AVAILABLE",
        "File logs are not available on this target.",
    ),
    (
        ErrorKind::InvalidArgument,
        "ERR_LOGGING_INVALID_ARGUMENT",
        "An option is empty or out of range, such as a zero timeout or an empty directory.",
    ),
    (
        ErrorKind::Cancelled,
        "ERR_LOGGING_CANCELLED",
        "The write was cancelled before it reached the disk.",
    ),
    (
        ErrorKind::Timeout,
        "ERR_LOGGING_TIMEOUT",
        "The write timed out, so the line may not be on disk.",
    ),
    (
        ErrorKind::DiskFull,
        "ERR_LOGGING_DISK_FULL",
        "The disk is full. Free space or lower `max_total_size_bytes`.",
    ),
    (
        ErrorKind::PermissionDenied,
        "ERR_LOGGING_PERMISSION_DENIED",
        "The log directory is not writable by this app.",
    ),
    (
        ErrorKind::NotFound,
        "ERR_LOGGING_NOT_FOUND",
        "The log file is gone: it was rotated away or cleared.",
    ),
    (ErrorKind::Backend, "ERR_LOGGING_BACKEND", "The platform's file service failed."),
];

#[test]
fn every_kind_carries_its_code_and_its_message() {
    for (kind, code, message) in ALL_KINDS {
        let error = LoggingError::new(kind);
        assert_eq!(error.kind(), kind);
        assert_eq!(error.code(), code);
        assert_eq!(kind.code(), code);
        assert_eq!(kind.message(), message);
        assert_eq!(error.to_string(), code, "Display prints the stable code");
        assert_eq!(error.status(), None);
    }
}

#[tokio::test]
async fn filesystem_failures_map_to_their_codes() {
    let backend = FakeBackend::new();
    let logger = Logging::with_backend(backend.clone(), &options()).await.unwrap();

    // A scripted disk answers full, denied and missing; each write keeps its code and
    // records nothing.
    for kind in [ErrorKind::DiskFull, ErrorKind::PermissionDenied, ErrorKind::NotFound] {
        backend.fail_next_append(LoggingError::new(kind));
        let error = logger.error("burst").await.unwrap_err();
        assert_eq!(error.code(), kind.code());
        assert!(
            logger.logs().await.unwrap().is_empty(),
            "a failed write records nothing"
        );
    }
    assert_eq!(backend.appends(), 0);
    logger.error("written").await.unwrap();
    assert_eq!(backend.lines().len(), 1);

    // The cleanup and the drain answer through the same codes.
    backend.fail_next_files(LoggingError::new(ErrorKind::DiskFull));
    let error = logger.clean().await.unwrap_err();
    assert_eq!(error.code(), "ERR_LOGGING_DISK_FULL");
    backend.fail_next_remove(LoggingError::new(ErrorKind::PermissionDenied));
    let error = logger.clean().await.unwrap_err();
    assert_eq!(error.code(), "ERR_LOGGING_PERMISSION_DENIED");
    backend.fail_next_flush(LoggingError::new(ErrorKind::NotFound));
    let error = logger.flush().await.unwrap_err();
    assert_eq!(error.code(), "ERR_LOGGING_NOT_FOUND");

    // A platform's own status code rides along on a backend failure, as a raw OS code would.
    backend.fail_next_append(LoggingError::new(ErrorKind::Backend).with_status(28));
    let error = logger.error("burst").await.unwrap_err();
    assert_eq!(error.code(), "ERR_LOGGING_BACKEND");
    assert_eq!(error.status(), Some(28));
}
