//! The web: no file sink of its own, so every storage call answers `NotAvailable`.
//!
//! A plain wasm32 crate reaches no local disk, and this module uses no filesystem API. For
//! file logging to work here a host would have to inject the `nsh_core::filesystem`
//! `FileSystem` capability — one `execute` seam covering write, list, stat and remove for
//! normalized relative paths under the app's private root, the web platform's OPFS-backed
//! implementation being one such host. Until that seam exists, each call resolves
//! `ERR_LOGGING_NOT_AVAILABLE`, the wording `docs/goals.md` gives the web.

use super::NoFileSink;

pub(crate) type NativeBackend = NoFileSink;
