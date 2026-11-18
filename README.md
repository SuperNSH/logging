# nsh-logging

Local log files with rotation, bounded disk use and redaction.

Part of the Native Shell (NSH) capability set. Same layout as the other SuperNSH repos: a Rust crate and a TypeScript package, docs in `docs/`, CI on GitHub Actions, and a demo app shipped as nine downloadable builds and a web page on every push to `master`.

## Status

**Implemented, and proven by CI on `master`.** The `format`, `rust`, `cross-check` and `typescript` jobs build and test the crate, the package and the demo app on every push; the `demo`, `publish` and `pages` jobs ship fresh builds with every push to `master`. What the runners prove: the edge-case tests in both languages, and the real-file platform tests — `crates/nsh-logging/tests/platform.rs` drives the real `Logging::new` through writes, flushes, the drop-flush and the mid-interval size roll on ubuntu, macos and windows. What they do not: iOS and Android are compile-checked only (Clippy per target — CI has no device, and the desktop runners run the same `std::fs` file code), and no runner fills its disk or refuses a directory; those failures are scripted through `FakeBackend` and `memoryBackend`. Read [docs/goals.md](docs/goals.md) for the goals, the error model and the edge cases, [docs/decisions.md](docs/decisions.md) for why each choice was made, and [docs/parity.md](docs/parity.md) for what is done.

## What it does

- Writes log lines to local files through a non-blocking writer, rotated by time (`never`, `minutely`, `hourly`, `daily`).
- Bounds disk use with `max_file_size_bytes`, `max_total_size_bytes` and `max_age`: an oversized file rolls mid-interval — on apple and android before the write that would outgrow the cap, on linux and windows by renaming the full file aside once it reaches the cap — and the oldest files are deleted first.
- Redacts tokens and personal fields before anything is written.
- Resolves every call exactly once: success, a coded failure such as `ERR_LOGGING_DISK_FULL`, or a cancel.
- One API with the same names in Rust (`crates/nsh-logging`) and TypeScript (`packages/nsh-logging`), following the two references: `talker`'s levels, history and clearing, and `react-native-logs`' severity filter and file transport. What is deliberately not offered — console output, custom formatting, runtime reconfiguration — is recorded row by row in [docs/parity.md](docs/parity.md).

## Try the demo

The demo in `examples/demo` shows the surface end to end: a memory logger with the four levels, a redaction write, a thrown exception logged with its stack, a newest-first history and `clean`; a logger built at `warning` dropping a debug write; the Rust crate's file logger writing and flushing, with error codes surfaced where no file sink exists; and a logger built with a zero timeout whose every write answers `ERR_LOGGING_INVALID_ARGUMENT`. Open it on the web at https://supernsh.github.io/logging/ (the web has no file sink, so the file section answers `ERR_LOGGING_NOT_AVAILABLE` there), or download a build of the latest `master`:

| Platform | Download |
|---|---|
| macOS (Apple silicon) | [logging-demo-macos-arm64.zip](https://github.com/SuperNSH/logging/releases/latest/download/logging-demo-macos-arm64.zip) |
| macOS (Intel) | [logging-demo-macos-x86_64.zip](https://github.com/SuperNSH/logging/releases/latest/download/logging-demo-macos-x86_64.zip) |
| Linux x86_64 | [logging-demo-linux-x86_64.tar.gz](https://github.com/SuperNSH/logging/releases/latest/download/logging-demo-linux-x86_64.tar.gz) |
| Linux arm64 | [logging-demo-linux-arm64.tar.gz](https://github.com/SuperNSH/logging/releases/latest/download/logging-demo-linux-arm64.tar.gz) |
| Windows x86_64 | [logging-demo-windows-x86_64.zip](https://github.com/SuperNSH/logging/releases/latest/download/logging-demo-windows-x86_64.zip) |
| Windows arm64 | [logging-demo-windows-arm64.zip](https://github.com/SuperNSH/logging/releases/latest/download/logging-demo-windows-arm64.zip) |
| iOS simulator | [logging-demo-ios-simulator.zip](https://github.com/SuperNSH/logging/releases/latest/download/logging-demo-ios-simulator.zip) |
| Android (arm64) | [logging-demo-android.apk](https://github.com/SuperNSH/logging/releases/latest/download/logging-demo-android.apk) |
| Web (static site) | [logging-demo-web.tar.gz](https://github.com/SuperNSH/logging/releases/latest/download/logging-demo-web.tar.gz) |

- **macOS:** the app is signed ad hoc, not notarized, and needs macOS 14 or later. macOS quarantines what a browser downloads, so clear that before opening it: `xattr -dr com.apple.quarantine <the unzipped .app>`.
- **iOS simulator:** an arm64 simulator build, so it needs Xcode with an iOS simulator runtime on an Apple silicon Mac. Unzip it and install it on a booted simulator with `xcrun simctl install booted <the unzipped .app>`, or drag the `.app` onto the simulator window.

## Layout

- `crates/nsh-logging/`: the Rust crate. `src/` is the library, `tests/` the edge-case tests named in [docs/goals.md](docs/goals.md) plus `platform.rs`, which drives the real `Logging::new` on the host runner's own filesystem.
- `packages/nsh-logging/`: the TypeScript package, run with NSH. `src/` mirrors the Rust API in camelCase, `tests/` mirrors each Rust test, including `rust.test.ts` over the real Rust backend.
- `examples/demo/`: the demo of [Try the demo](#try-the-demo), an NSH app in TSX with the headless tests CI runs.
- `docs/brief.md`: options, recommended path and edge cases from the research.
- `docs/goals.md`: goals and acceptance criteria, the error model, and the edge-case list with its tests.
- `docs/decisions.md`: decisions, each with its reason.
- `docs/parity.md`: feature status.
- `.github/workflows/ci.yml`: the whole pipeline — the gates below and the demo, publish and pages jobs.

## What CI does, and what it does not

CI runs on every push to `master` and on pull requests. Every compile and test runs on GitHub's runners; nothing is built on a developer machine.

- **format**: `cargo fmt --all -- --check`.
- **rust** on ubuntu, macos and windows: Clippy with `-D warnings` over all targets and features, then `cargo test --all-features --no-fail-fast`. The tests write real files on each runner's own filesystem through the real `Logging::new`, which is the same `std::fs` code iOS and Android run.
- **cross-check**: Clippy for `aarch64-apple-ios` and `aarch64-apple-ios-sim` on macOS, `aarch64-linux-android` on Ubuntu with the NDK toolchain, and `wasm32-unknown-unknown` on Ubuntu, each with `--all-features -- -D warnings`.
- **typescript**: installs NSH 0.6.0 from nsh.dev and, for both `packages/nsh-logging` and `examples/demo`, runs `nsh check`, `nsh build` failing on any warning, and `nsh test`.
- **demo**, on `master` only: packages `examples/demo` for macOS, Linux and Windows on both x86_64 and arm64, plus the iOS simulator, Android (arm64-v8a) and the web, and uploads the nine builds as artifacts named `logging-demo-<name>`.
- **publish**, on `master` only: replaces the `latest` GitHub release with those nine builds (the downloads in [Try the demo](#try-the-demo)).
- **pages**, on `master` only: deploys the demo's web build to GitHub Pages at https://supernsh.github.io/logging/, from the same artifact the release takes.

CI does **not**:

- run the iOS or Android build on a device or an emulator: both targets are compile-checked, and the desktop runners run the same `std::fs` file code they run;
- fill a runner's disk or refuse a directory's permissions: `ERR_LOGGING_DISK_FULL`, `ERR_LOGGING_PERMISSION_DENIED` and `ERR_LOGGING_NOT_FOUND` are scripted through `FakeBackend` (Rust) and `memoryBackend` (TS), with the `io::Error`-to-kind mapping proven by a unit test;
- run the wasm binary: the web build is compile-checked, and its no-file answer is unit-tested on the desktop runners and shown in the web demo, but no headless browser drives it.

## Licence

MIT. See [LICENSE](LICENSE).

Repository: https://github.com/SuperNSH/logging
