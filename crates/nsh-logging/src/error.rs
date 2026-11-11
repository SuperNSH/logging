//! The errors a log call reports. Each has a stable code; a message says what to do.

use std::{fmt, io};

/// What went wrong, as a stable code; see [`LoggingError::code`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorKind {
    /// File logs do not exist here: the web, or a target with no file sink.
    NotAvailable,
    /// An option is empty or out of range, such as a zero timeout or an empty directory.
    InvalidArgument,
    /// The write was dropped before it reached the disk.
    Cancelled,
    /// The write passed its `write_timeout` deadline.
    Timeout,
    /// The storage the logs go to is full.
    DiskFull,
    /// The log directory is not writable by this app.
    PermissionDenied,
    /// The log file is gone: it was rotated away or cleared.
    NotFound,
    /// The platform's file service failed; `status` holds its own code where it gives one.
    Backend,
}

/// A failed log operation.
///
/// `Display` prints the stable code. The platform's own status code, when it gave one,
/// rides along in `status` and neither field holds message text.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{}", .kind.code())]
pub struct LoggingError {
    kind: ErrorKind,
    status: Option<i64>,
}

impl LoggingError {
    /// An error of `kind`, with no platform status.
    pub fn new(kind: ErrorKind) -> Self {
        Self { kind, status: None }
    }

    /// The same error, carrying the platform's own status code.
    #[must_use]
    pub fn with_status(mut self, status: i64) -> Self {
        self.status = Some(status);
        self
    }

    /// What went wrong.
    pub fn kind(&self) -> ErrorKind {
        self.kind
    }

    /// The platform's status code, when the platform gave one.
    pub fn status(&self) -> Option<i64> {
        self.status
    }

    /// The stable code, such as `ERR_LOGGING_TIMEOUT`. Apps match on this.
    pub fn code(&self) -> &'static str {
        self.kind.code()
    }

    /// Maps one filesystem error, as every platform backend does: a missing file, denied
    /// access and a full disk keep their kind; anything else is `Backend` carrying the raw
    /// OS error code as the status.
    pub(crate) fn from_io(error: io::Error) -> Self {
        match error.kind() {
            io::ErrorKind::NotFound => Self::new(ErrorKind::NotFound),
            io::ErrorKind::PermissionDenied => Self::new(ErrorKind::PermissionDenied),
            io::ErrorKind::StorageFull => Self::new(ErrorKind::DiskFull),
            _ if disk_full(&error) => Self::new(ErrorKind::DiskFull),
            _ => match error.raw_os_error() {
                Some(code) => Self::new(ErrorKind::Backend).with_status(i64::from(code)),
                None => Self::new(ErrorKind::Backend),
            },
        }
    }
}

impl ErrorKind {
    /// The stable code of this kind.
    pub fn code(self) -> &'static str {
        match self {
            ErrorKind::NotAvailable => "ERR_LOGGING_NOT_AVAILABLE",
            ErrorKind::InvalidArgument => "ERR_LOGGING_INVALID_ARGUMENT",
            ErrorKind::Cancelled => "ERR_LOGGING_CANCELLED",
            ErrorKind::Timeout => "ERR_LOGGING_TIMEOUT",
            ErrorKind::DiskFull => "ERR_LOGGING_DISK_FULL",
            ErrorKind::PermissionDenied => "ERR_LOGGING_PERMISSION_DENIED",
            ErrorKind::NotFound => "ERR_LOGGING_NOT_FOUND",
            ErrorKind::Backend => "ERR_LOGGING_BACKEND",
        }
    }

    /// What to do about it, in one sentence.
    pub fn message(self) -> &'static str {
        match self {
            ErrorKind::NotAvailable => "File logs are not available on this target.",
            ErrorKind::InvalidArgument => {
                "An option is empty or out of range, such as a zero timeout or an empty directory."
            }
            ErrorKind::Cancelled => "The write was cancelled before it reached the disk.",
            ErrorKind::Timeout => "The write timed out, so the line may not be on disk.",
            ErrorKind::DiskFull => "The disk is full. Free space or lower `max_total_size_bytes`.",
            ErrorKind::PermissionDenied => "The log directory is not writable by this app.",
            ErrorKind::NotFound => "The log file is gone: it was rotated away or cleared.",
            ErrorKind::Backend => "The platform's file service failed.",
        }
    }
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

/// ENOSPC on unix and ERROR_DISK_FULL on windows: std's own mapping has not caught up.
fn disk_full(error: &io::Error) -> bool {
    match error.raw_os_error() {
        #[cfg(unix)]
        Some(28) => true,
        #[cfg(windows)]
        Some(112) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn io_errors_map_to_their_kinds() {
        assert_eq!(
            LoggingError::from_io(io::Error::from(io::ErrorKind::NotFound)).kind(),
            ErrorKind::NotFound
        );
        assert_eq!(
            LoggingError::from_io(io::Error::from(io::ErrorKind::PermissionDenied)).kind(),
            ErrorKind::PermissionDenied
        );
        #[cfg(unix)]
        assert_eq!(
            LoggingError::from_io(io::Error::from_raw_os_error(28)).kind(),
            ErrorKind::DiskFull
        );
        #[cfg(windows)]
        assert_eq!(
            LoggingError::from_io(io::Error::from_raw_os_error(112)).kind(),
            ErrorKind::DiskFull
        );
        let unclassified = LoggingError::from_io(io::Error::other("unwritable"));
        assert_eq!(unclassified.kind(), ErrorKind::Backend);
        assert_eq!(unclassified.status(), None);
    }
}
