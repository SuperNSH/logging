# Decisions

Each entry says what was chosen and why. `docs/goals.md` lists the goals, the error model and the edge cases.

## Open

None.

## Decided

### STEP 0: references cloned

On 2026-11-09 all five reference repositories were cloned successfully into `.refs/`: `github.com/Frezyx/talker`, `github.com/onubo/react-native-logs`, `github.com/tokio-rs/tracing`, `github.com/longbridge/nsh` and `github.com/SuperNSH/biometrics`. No canonical-repo substitution was needed: each clone came from the URL the brief names, and the org template was read for its docs shape, its error style and its CI before anything here was written.

### NSH version 0.4.0

The TypeScript package is built and tested with NSH 0.4.0 exactly, the version the org template's CI pins (`NSH_VERSION: v0.4.0` in its typescript job, installed from SuperNSH/nsh-builds). The released CLI is what CI installs and what an NSH app runs today, so parity with it is what the `typescript` job proves; native features that exist only on NSH's unreleased `main` are not used. The Rust crate's `nsh-core` dependency is `0.4.0` for the same reason.

### Library choice: tracing-appender, std::fs, and the nsh-core filesystem seam

`tracing-appender` supplies the two hard parts. Its `non_blocking` writer hands lines to a dedicated worker thread, and the `WorkerGuard` it returns flushes every buffered line when the process exits, which is the flush-on-exit edge case the brief demands. Its `RollingFileAppender` rotates `never`, `minutely`, `hourly`, `daily` and `weekly`, and its builder's `max_log_files` prunes old files. What it does not do is bound a file's size, the total bytes or the age, so `max_file_size_bytes`, `max_total_size_bytes` and `max_age` are written here on top: the size cap rolls mid-interval under two family rules — apple and android roll before the write that would outgrow the cap, the live file carrying `prefix.<date>.N`; linux and windows rename the full file to the first free `prefix.<date>.N` when a write finds it at the cap and restart the plain interval name — and the rotation tests on both sides assert both rules. The total and age caps delete the oldest files first after each rotation and each write. This library's `rotation` option takes four of the crate's five intervals — `never`, `minutely`, `hourly`, `daily`, the set `docs/parity.md` records; `weekly` stays tracing-appender's to offer, not ours. Storage is `std::fs` on native targets. Where a host provides a filesystem instead — the realistic case on the web, which has no `std::fs` — the crate goes through `nsh_core::filesystem`'s `FileSystem` capability, whose requests carry normalized relative paths under the app's private root. That seam is also where the tests script disk-full, permission-denied and not-found, none of which a healthy CI runner's disk produces on its own.

### Dependency versions

The crate manifest is written (`crates/nsh-logging/Cargo.toml`) and carries: `nsh-core` 0.4.0, `thiserror` 2.0.21, `tokio` 1.54.1 (with `sync` and `time`), `tracing-appender` 0.2.5 — held off `wasm32`, where its `symlink` dependency has no story and the web has no file sink — and, on Android only, `jni` 0.22.4 with `ndk-context` 0.1.1, where the app's log directory comes from its context over JNI. These are caret requirements, not exact pins: each names the least release that works, and any semver-compatible one resolves. `Cargo.lock` is not committed, so each CI run resolves the newest matching release — which is also why no version here counts as verified until a green run exists. `nsh-core`, `thiserror`, `tokio`, `jni` and `ndk-context` match the org template's crate, measured there on 2026-11-07.

### Platforms in scope

Apple, Android, Windows and Linux write native file sinks through `std::fs`, and the `rust` job runs the file tests on the three desktop runners, which cover the same code the mobile targets run. The web has no app-private log directory of its own, so file logs report `ERR_LOGGING_NOT_AVAILABLE` there; nothing is written on the web until a host provides the `nsh_core::filesystem` capability, which is the seam that could change that answer later — console output is not this library's anywhere, as the next entry records. Any other target gets the same unsupported fallback, so the NotAvailable path is one code path, tested on every runner rather than only where it is real.

### The API follows talker and react-native-logs

The names an app developer already knows are the names this library uses: talker's levels, its `handle()` for exceptions (here `log_exception`), its history read-back (`logs()`, stored as files, not a Dart list) and its clearing (`clean`); react-native-logs' severity filter, fixed at construction, and its file transport, which is this crate's core. talker's `LogLevel` set is `error`, `critical`, `info`, `debug`, `verbose` and `warning`; ours keeps four of the six — `debug`, `info`, `warning`, `error` — folding `critical` into `error` and `verbose` into `debug`. Console output, custom line formatting and runtime reconfiguration are deliberately not offered: the line format is the writer's, the console is the shell's, and options fixed at construction are testable where fire-and-forget logging that can silently lose lines is not — every call resolves exactly once, with success, a coded failure or a cancel. `docs/parity.md` records each of these answers row by row.
