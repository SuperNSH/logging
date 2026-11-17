//! A target with no file sink: every storage call answers that file logs are not
//! available.

use super::NoFileSink;

pub(crate) type NativeBackend = NoFileSink;
