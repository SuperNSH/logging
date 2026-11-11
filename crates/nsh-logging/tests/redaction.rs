//! Redaction happens before anything is written: the values of the listed fields, emails
//! and long random-looking tokens are masked in the entry the backend receives.

use nsh_logging::{FakeBackend, Logging, LoggingOptions};

/// The defaults behind a function, so a test's field assignments are not the
/// default-then-reassign shape clippy flags; the fake never opens the directory.
fn options() -> LoggingOptions {
    LoggingOptions::default()
}

#[tokio::test]
async fn tokens_and_personal_fields_are_masked_before_anything_is_written() {
    let backend = FakeBackend::new();
    let logger = Logging::with_backend(backend.clone(), &options()).await.unwrap();

    logger.info("password: hunter2").await.unwrap();
    logger.error("the session token was eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9x9").await.unwrap();
    logger.warning("secret: shhh").await.unwrap();
    logger.info("authorization: sesame").await.unwrap();
    logger.debug("email: ada@example.com phone: 555-0100").await.unwrap();

    let entries = logger.logs().await.unwrap();
    let expected = [
        "password: [REDACTED]",
        "the session token was [REDACTED]",
        "secret: [REDACTED]",
        "authorization: [REDACTED]",
        "email: [REDACTED] phone: [REDACTED]",
    ];
    for (entry, message) in entries.iter().zip(expected) {
        assert_eq!(entry.message, message);
    }
    // The lines carry the same redacted messages: masking happened before the write, not
    // on read-back.
    let lines: Vec<String> = entries
        .iter()
        .map(|entry| format!("{} {} {}", entry.level.name(), entry.time_ms, entry.message))
        .collect();
    assert_eq!(backend.lines(), lines);
    let joined = backend.lines().join("\n");
    assert!(!joined.contains("hunter2"));
    assert!(!joined.contains("ada@example.com"));

    // The field list is the option: with it emptied, the same message passes through.
    let open_backend = FakeBackend::new();
    let mut open_options = options();
    open_options.redact = Vec::new();
    let open = Logging::with_backend(open_backend.clone(), &open_options).await.unwrap();
    open.info("password: hunter2").await.unwrap();
    let entries = open.logs().await.unwrap();
    assert_eq!(entries[0].message, "password: hunter2");
}
