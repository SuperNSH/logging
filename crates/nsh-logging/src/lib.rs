//! Local log files with rotation, bounded disk use and redaction for Native Shell apps.
//!
//! [`Logging`] carries the names `talker` and `react-native-logs` made familiar: levels,
//! exception handling, history read-back and clearing, behind one fixed set of options.
//! Every call resolves exactly once: the line is written, a [`LoggingError`] names the
//! failure, or the level filter dropped it without an error.

mod backend;
mod error;
mod fake;
mod logging;
mod native;
mod options;
mod platform;
mod redact;

use nsh_core::prelude::*;

pub use backend::{Backend, BackendFuture, FileStat};
pub use error::{ErrorKind, LoggingError};
pub use fake::FakeBackend;
pub use logging::{LogEntry, Logging};
pub use native::{NativeLogEntry, NativeLogging, NativeLoggingOptions};
pub use options::{Level, LoggingOptions, Rotation};
pub use redact::redact;

/// The crate's UI set: what TSX imports from `nsh-logging/native`.
#[native_module("nsh-logging/native")]
fn native(registry: &mut Registry) {
    registry.class::<NativeLogging>();
}
