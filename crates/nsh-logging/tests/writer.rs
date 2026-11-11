//! The write path's promises: one write at a time, each resolving exactly once, a deadline
//! that frees a stuck write, and a drain when the last owner goes.

use std::{
    sync::{Arc, atomic::{AtomicBool, AtomicUsize, Ordering::SeqCst}},
    time::Duration,
};

use nsh_logging::{
    Backend, BackendFuture, ErrorKind, FakeBackend, FileStat, LogEntry, Logging, LoggingError,
    LoggingOptions,
};
use tokio::sync::Notify;

/// The defaults behind a function, so a test's field assignments are not the
/// default-then-reassign shape clippy flags; the fake never opens the directory.
fn options() -> LoggingOptions {
    LoggingOptions::default()
}

/// Yields until `ready` holds, so a spawned write has reached the backend.
async fn settle(ready: impl Fn() -> bool) {
    for _ in 0..1000 {
        if ready() {
            return;
        }
        tokio::task::yield_now().await;
    }
    panic!("the write never reached the backend");
}

/// A file service that is stuck: appends park until released, as a writer wedged behind a
/// full queue would be. Everything else answers through the fake.
#[derive(Clone)]
struct StuckBackend {
    inner: FakeBackend,
    wedge: Arc<Wedge>,
}

struct Wedge {
    stuck: AtomicBool,
    attempts: AtomicUsize,
    release: Notify,
}

impl StuckBackend {
    fn wedged() -> Self {
        Self {
            inner: FakeBackend::new(),
            wedge: Arc::new(Wedge {
                stuck: AtomicBool::new(true),
                attempts: AtomicUsize::new(0),
                release: Notify::new(),
            }),
        }
    }

    fn release(&self) {
        self.wedge.stuck.store(false, SeqCst);
        self.wedge.release.notify_waiters();
    }

    fn attempts(&self) -> usize {
        self.wedge.attempts.load(SeqCst)
    }

    fn lines(&self) -> Vec<String> {
        self.inner.lines()
    }
}

impl Backend for StuckBackend {
    fn append(&self, entry: &LogEntry) -> BackendFuture<'_, Result<(), LoggingError>> {
        let inner = self.inner.clone();
        let wedge = Arc::clone(&self.wedge);
        let entry = entry.clone();
        Box::pin(async move {
            wedge.attempts.fetch_add(1, SeqCst);
            if wedge.stuck.load(SeqCst) {
                wedge.release.notified().await;
            }
            inner.append(&entry).await
        })
    }

    fn flush(&self) -> BackendFuture<'_, Result<(), LoggingError>> {
        self.inner.flush()
    }

    fn remove_all(&self, names: &[String]) -> BackendFuture<'_, Result<(), LoggingError>> {
        self.inner.remove_all(names)
    }

    fn files(&self) -> BackendFuture<'_, Result<Vec<FileStat>, LoggingError>> {
        self.inner.files()
    }
}

/// An append that yields mid-flight, counting how many are in flight at once: without the
/// core's gate, concurrent writers would overlap inside the backend.
#[derive(Clone, Default)]
struct ProbingBackend {
    inner: FakeBackend,
    in_flight: Arc<AtomicUsize>,
    peak: Arc<AtomicUsize>,
}

impl ProbingBackend {
    fn peak(&self) -> usize {
        self.peak.load(SeqCst)
    }

    fn lines(&self) -> Vec<String> {
        self.inner.lines()
    }

    fn appends(&self) -> usize {
        self.inner.appends()
    }
}

impl Backend for ProbingBackend {
    fn append(&self, entry: &LogEntry) -> BackendFuture<'_, Result<(), LoggingError>> {
        let inner = self.inner.clone();
        let in_flight = Arc::clone(&self.in_flight);
        let peak = Arc::clone(&self.peak);
        let entry = entry.clone();
        Box::pin(async move {
            let now = in_flight.fetch_add(1, SeqCst) + 1;
            peak.fetch_max(now, SeqCst);
            for _ in 0..4 {
                tokio::task::yield_now().await;
            }
            in_flight.fetch_sub(1, SeqCst);
            inner.append(&entry).await
        })
    }

    fn flush(&self) -> BackendFuture<'_, Result<(), LoggingError>> {
        self.inner.flush()
    }

    fn remove_all(&self, names: &[String]) -> BackendFuture<'_, Result<(), LoggingError>> {
        self.inner.remove_all(names)
    }

    fn files(&self) -> BackendFuture<'_, Result<Vec<FileStat>, LoggingError>> {
        self.inner.files()
    }
}

#[tokio::test]
async fn dropping_the_guard_flushes_every_buffered_write() {
    let backend = FakeBackend::new();
    let logger = Logging::with_backend(backend.clone(), &options()).await.unwrap();
    logger.debug("first").await.unwrap();
    logger.info("second").await.unwrap();
    logger.warning("third").await.unwrap();
    assert_eq!(backend.flushes(), 0);

    let keeper = logger.clone();
    drop(logger);
    assert_eq!(backend.flushes(), 0, "a remaining owner keeps the writer open");

    // The last owner's drop polls the flush once: every buffered line has been handed over.
    drop(keeper);
    assert_eq!(backend.flushes(), 1);
    assert_eq!(backend.lines().len(), 3);
}

#[tokio::test]
async fn concurrent_writes_are_serialized_and_each_resolves_once() {
    let backend = ProbingBackend::default();
    let logger = Logging::with_backend(backend.clone(), &options()).await.unwrap();

    let writes: Vec<_> = (0..25)
        .map(|index| {
            let logger = logger.clone();
            tokio::spawn(async move { logger.info(&format!("line {index}")).await })
        })
        .collect();
    for handle in writes {
        handle.await.unwrap().unwrap();
    }

    let expected: Vec<String> = (0..25).map(|index| format!("line {index}")).collect();
    assert_eq!(backend.appends(), 25);
    let lines = backend.lines();
    let messages: Vec<String> = lines
        .iter()
        .map(|line| line.split(' ').skip(2).collect::<Vec<_>>().join(" "))
        .collect();
    assert_eq!(messages, expected, "one line landed per call, in order");
    assert_eq!(backend.peak(), 1, "no two appends were in flight at once");
    let history: Vec<String> = logger
        .logs()
        .await
        .unwrap()
        .into_iter()
        .map(|entry| entry.message)
        .collect();
    assert_eq!(history, expected);
}

#[tokio::test(start_paused = true)]
async fn a_write_past_its_deadline_resolves_with_the_timeout_code() {
    let backend = StuckBackend::wedged();
    let mut options = options();
    options.write_timeout = Some(Duration::from_millis(5));
    let logger = Logging::with_backend(backend.clone(), &options).await.unwrap();

    let stuck = {
        let logger = logger.clone();
        tokio::spawn(async move { logger.info("never lands").await })
    };
    settle(|| backend.attempts() == 1).await;
    tokio::time::advance(Duration::from_millis(6)).await;
    let error = stuck.await.unwrap().unwrap_err();
    assert_eq!(error.code(), "ERR_LOGGING_TIMEOUT");
    assert!(
        logger.logs().await.unwrap().is_empty(),
        "the timed-out write recorded nothing"
    );

    // The deadline freed the queue: the write behind a stuck one is not hostage to it.
    backend.release();
    logger.info("after the deadline").await.unwrap();
    assert_eq!(backend.lines().len(), 1);
    assert!(backend.lines()[0].contains("after the deadline"));
}

#[tokio::test]
async fn cancel_resolves_the_pending_write_exactly_once_as_cancelled() {
    let backend = FakeBackend::new();
    let logger = Logging::with_backend(backend.clone(), &options()).await.unwrap();

    backend.fail_next_append(LoggingError::new(ErrorKind::Cancelled));
    let error = logger.warning("dropped before the disk").await.unwrap_err();
    assert_eq!(error.code(), "ERR_LOGGING_CANCELLED");
    assert!(
        logger.logs().await.unwrap().is_empty(),
        "a cancelled write records nothing"
    );

    // One answer, no retry: the next write is the only one that lands.
    logger.info("after the cancel").await.unwrap();
    assert_eq!(backend.appends(), 1);
    assert_eq!(backend.lines().len(), 1);
}
