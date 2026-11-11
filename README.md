# nsh-logging

Local log files with rotation, bounded disk use and redaction.

Part of the Native Shell (NSH) capability set. Same layout as the other SuperNSH repos: a Rust crate and a TypeScript package, docs in `docs/`, CI on GitHub Actions, and a web status page on GitHub Pages at https://supernsh.github.io/logging/.

## Status

**Implemented, pending this run's first CI check.** This run writes the Rust crate, the TypeScript package and their tests into the tree, and the run's final report discloses any area that failed. No green CI run exists, so nothing on this page is proven: every claim waits for the `rust`, `cross-check` and `typescript` jobs to pass on `master`. Read [docs/goals.md](docs/goals.md) for the goals, the error model and the edge cases, [docs/decisions.md](docs/decisions.md) for why each choice was made, and [docs/parity.md](docs/parity.md) for what is done.

## What it does

- Writes log lines to local files through a non-blocking writer, rotated by time (`never`, `minutely`, `hourly`, `daily`).
- Bounds disk use with `max_file_size_bytes`, `max_total_size_bytes` and `max_age`: an oversized file rolls mid-interval — on apple and android before the write that would outgrow the cap, on linux and windows by renaming the full file aside once it reaches the cap — and the oldest files are deleted first.
- Redacts tokens and personal fields before anything is written.
- Resolves every call exactly once: success, a coded failure such as `ERR_LOGGING_DISK_FULL`, or a cancel.
- One API with the same names in Rust (`crates/nsh-logging`) and TypeScript (`packages/nsh-logging`), following the two references: `talker`'s levels, history and clearing, and `react-native-logs`' severity filter and file transport. What is deliberately not offered — console output, custom formatting, runtime reconfiguration — is recorded row by row in [docs/parity.md](docs/parity.md).

## Layout

- `crates/nsh-logging/`: the Rust crate. `src/` is the library, `tests/` the edge-case tests named in [docs/goals.md](docs/goals.md).
- `packages/nsh-logging/`: the TypeScript package, run with NSH. `src/` mirrors the Rust API in camelCase, `tests/` mirrors each Rust test.
- `docs/brief.md`: options, recommended path and edge cases from the research.
- `docs/goals.md`: goals and acceptance criteria, the error model, and the edge-case list with its tests.
- `docs/decisions.md`: decisions, each with its reason.
- `docs/parity.md`: feature status.
- `site/`: static status page, published by `.github/workflows/pages.yml`.
- `.github/workflows/ci.yml`: the gate set described below.

## Checks

Every compile and test runs in GitHub Actions. Nothing is built on a developer machine. `.github/workflows/ci.yml` runs on every push to `master` and on pull requests:

- **format**: `cargo fmt --all -- --check`.
- **rust**: a matrix of ubuntu-latest, macos-latest and windows-latest running `cargo clippy --all-targets --all-features -- -D warnings` and `cargo test --all-features --no-fail-fast`. The file-writing tests run on the runners' real filesystems, which is the same code iOS and Android run; CI has no device.
- **cross-check**: Clippy for `aarch64-apple-ios` and `aarch64-apple-ios-sim` on macOS, `aarch64-linux-android` on Ubuntu with the NDK toolchain, and `wasm32-unknown-unknown` on Ubuntu, each with `--all-features -- -D warnings`.
- **typescript**: installs NSH 0.4.0 and, in `packages/nsh-logging`, runs `nsh check`, `nsh build` failing on any warning, and `nsh test`.

There is no demo or publish job: this repo has no demo app. `site/` is published by the separate `.github/workflows/pages.yml`. Until a run of these jobs is green, every status claim above is unverified.

## Licence

MIT. See [LICENSE](LICENSE).

Repository: https://github.com/SuperNSH/logging
