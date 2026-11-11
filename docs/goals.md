# Goals: nsh-logging

Local log files with rotation, bounded disk use and redaction, behind one API with the same names and behavior in the Rust crate (`crates/nsh-logging`) and the TypeScript package (`packages/nsh-logging`). The API takes what an app developer would recognize from the two references: `talker`'s levels, exception handling, history read-back and clearing, and `react-native-logs`' severity filter and file transport. What the references offer that this library deliberately declines — console output, custom line formatting, runtime reconfiguration — is recorded row by row in [parity.md](parity.md). The writing itself is `tracing-appender`'s non-blocking writer and time rotation (see [decisions.md](decisions.md)).

This run writes the crate, the package, the tests named below and the CI workflow, so at the end of the run all of it is in the tree, pending CI; the run's final report discloses any area that failed. Each acceptance criterion names the CI job that will prove it, and nothing counts as proven until that job is green on `master`. The CI jobs are the ones in `.github/workflows/ci.yml`: `format`, `rust` (a matrix of ubuntu-latest, macos-latest and windows-latest), `cross-check` (aarch64-apple-ios, aarch64-apple-ios-sim, aarch64-linux-android, wasm32-unknown-unknown) and `typescript`.

## Goals and acceptance criteria

| Goal | Acceptance criterion | Proven by |
|---|---|---|
| One API, two languages | Every name below exists with the same meaning in Rust and in TypeScript, and each TypeScript test in the edge-case list mirrors its Rust test | `typescript` job (`nsh check`, `nsh build` with no warnings, `nsh test`) and `rust` job |
| Local log files with time rotation | A `rotation` of `never`, `minutely`, `hourly` or `daily` rolls to a new file at the interval, named `prefix.YYYY-MM-DD-HH-mm` and its shorter forms: apple and android let `tracing-appender`'s appender name the files, linux and windows compute the same spelling in their backends | `rust` job on all three desktop runners; `typescript` job |
| Bounded disk use | `max_file_size_bytes` rolls mid-interval under two family rules, both asserted by the tests named below: apple and android roll before the write that would outgrow the cap, the live file becoming `prefix.<date>.N`; linux and windows roll when a write finds the file at the cap, renaming the full file to the first free `prefix.<date>.N` and restarting the plain interval name; `max_total_size_bytes` and `max_age` delete the oldest files first, after rotation and after each write; a logger with no caps deletes nothing | `rust` job; `typescript` job |
| Redaction before writing | Token values and personal fields (`token`, `password`, `secret`, `authorization`, `email`, `phone`) are masked before anything is written | `rust` job; `typescript` job |
| Writes that resolve exactly once | Every log call resolves once: `Ok(())` once the writer has written the line, a coded failure, or a cancel. The non-blocking writer flushes on exit through its worker guard | `rust` job; `typescript` job |
| A coded failure for every failure mode | `LoggingError` carries one of the eight kinds below, each with a stable `ERR_LOGGING_` code and a one-sentence message | `rust` job; `typescript` job |
| Native sinks, and an honest web | Apple, Android, Windows and Linux write real files through `std::fs`; the web reports `ERR_LOGGING_NOT_AVAILABLE` for file logs; any other target gets the same unsupported fallback | `cross-check` job (each target compiles) and `rust` + `typescript` jobs (the fallback path runs on every runner) |
| Formatting and lint gates | `cargo fmt --all -- --check` passes | `format` job |

The desktop runners exercise the real file paths iOS and Android use: `std::fs` is the same there, and CI has no device. The wasm target is compile-checked only; its runtime behavior is the NotAvailable fallback, which the desktop runners do run.

## The API

`Logging` in both languages; TypeScript spells the same names in camelCase (`logException` for `log_exception`). The sink is pluggable: `with_backend` takes a `Backend` in Rust (`FakeBackend` for tests), and TypeScript picks `memoryBackend` or `rustBackend`.

```rust
let logger = Logging::new(&LoggingOptions::default()).await?;  // opens the directory, prunes, starts the writer
logger.log("ordered", Level::Info).await?;                    // one call, one message, one chosen level
logger.debug("thinking about it").await?;                     // each level resolves once written
logger.info("ordered").await?;
logger.warning("the pizza is over").await?;
logger.error("payment failed").await?;
logger.log_exception(&error).await?;                          // the failure and its stack, as talker's handle() takes them
logger.logs().await?;                                         // Vec<LogEntry>: what was written, oldest first
logger.clean().await?;                                        // empties the stored logs and deletes the files
logger.flush().await?;                                        // drains the non-blocking writer before exit
```

`LoggingOptions` is `#[non_exhaustive]` with a `Default`: `level` (the least severe level recorded, `debug`), `directory`, `file_base_name` (`"log"`), `rotation` (`daily`), `max_file_size_bytes`, `max_total_size_bytes`, `max_age`, `redact` (the field list above), and `write_timeout` (off by default; when it passes, the write resolves `ERR_LOGGING_TIMEOUT`). Options are fixed at construction: there is no reconfigure call, no runtime severity switch and no on/off flag — [parity.md](parity.md) records those refusals against talker's `configure` and react-native-logs' `setSeverity`, `enable` and `disable`.

## The error model

`LoggingError` has a `kind()` of `ErrorKind`, a stable `code()` apps match on, and a one-sentence `message()`, modeled on `nsh-biometrics`' `error.rs`. No operation panics and none hangs.

| `ErrorKind` | Code | Message |
|---|---|---|
| `NotAvailable` | `ERR_LOGGING_NOT_AVAILABLE` | File logs are not available on this target. |
| `InvalidArgument` | `ERR_LOGGING_INVALID_ARGUMENT` | An option is empty or out of range, such as a zero timeout or an empty directory. |
| `Cancelled` | `ERR_LOGGING_CANCELLED` | The write was cancelled before it reached the disk. |
| `Timeout` | `ERR_LOGGING_TIMEOUT` | The write timed out, so the line may not be on disk. |
| `DiskFull` | `ERR_LOGGING_DISK_FULL` | The disk is full. Free space or lower `max_total_size_bytes`. |
| `PermissionDenied` | `ERR_LOGGING_PERMISSION_DENIED` | The log directory is not writable by this app. |
| `NotFound` | `ERR_LOGGING_NOT_FOUND` | The log file is gone: it was rotated away or cleared. |
| `Backend` | `ERR_LOGGING_BACKEND` | The platform's file service failed. |

These eight kinds are the whole enum. The references demand no more: talker and react-native-logs report invalid configuration and transport failure, the platform adds disk full, permission and not-found, and the capability adds not-available, cancel and timeout. A filesystem error maps to the kind that says what happened — storage full to `DiskFull`, access denied to `PermissionDenied`, a missing file to `NotFound` — and anything else is `Backend`.

## Edge cases

Every case maps to one named Rust test in `crates/nsh-logging/tests/` and one named TypeScript test in `packages/nsh-logging/tests/`. A test counts only once it has been seen to fail.

| Edge case | Rust test | TypeScript test |
|---|---|---|
| Time rotation rolls at the interval | `rotation.rs` :: `time_rotation_rolls_to_a_new_file_at_the_interval` | `rotation.test.ts` :: "time rotation rolls to a new file at the interval" |
| Time rotation alone bounds nothing: with no caps, every rolled file stays | `rotation.rs` :: `rotation_alone_bounds_nothing_until_caps_are_set` | `rotation.test.ts` :: "rotation alone bounds nothing until caps are set" |
| The `max_file_size_bytes` roll is one rule per family: apple and android roll before the write that would outgrow the cap, the live file becoming `prefix.<date>.N`; linux and windows rename the full file to the first free `prefix.<date>.N` when a write finds it at the cap and restart the plain interval name — each named test runs both families | `rotation.rs` :: `size_cap_rolls_a_file_mid_interval` | `rotation.test.ts` :: "size cap rolls a file mid interval" |
| Unbounded growth: `max_total_size_bytes` deletes the oldest files first | `cleanup.rs` :: `max_total_size_bytes_deletes_the_oldest_files_first` | `cleanup.test.ts` :: "max total size bytes deletes the oldest files first" |
| Unbounded growth: `max_age` deletes files older than the limit | `cleanup.rs` :: `max_age_deletes_files_older_than_the_limit` | `cleanup.test.ts` :: "max age deletes files older than the limit" |
| The non-blocking writer flushes on exit | `writer.rs` :: `dropping_the_guard_flushes_every_buffered_write` | `writer.test.ts` :: "flush on exit resolves every buffered write" |
| Concurrent writes are serialized and each resolves exactly once | `writer.rs` :: `concurrent_writes_are_serialized_and_each_resolves_once` | `writer.test.ts` :: "concurrent writes are serialized and each resolves once" |
| A write timeout | `writer.rs` :: `a_write_past_its_deadline_resolves_with_the_timeout_code` | `writer.test.ts` :: "a write past its deadline rejects with the timeout code" |
| A cancel path | `writer.rs` :: `cancel_resolves_the_pending_write_exactly_once_as_cancelled` | `writer.test.ts` :: "cancel resolves the pending write exactly once as cancelled" |
| Redaction of tokens and personal fields before writing | `redaction.rs` :: `tokens_and_personal_fields_are_masked_before_anything_is_written` | `redaction.test.ts` :: "tokens and personal fields are masked before anything is written" |
| Invalid options | `options.rs` :: `invalid_options_are_refused_with_the_invalid_argument_code` | `options.test.ts` :: "invalid options are refused with the invalid argument code" |
| Options are fixed at construction: a higher construction level drops lower writes (react-native-logs' `setSeverity` is not offered) | `options.rs` :: `a_higher_construction_level_drops_lower_writes` | `options.test.ts` :: "a higher construction level drops lower writes" |
| Levels debug, info, warning, error, and the severity filter (talker's `critical` folds into `error`, `verbose` into `debug`) | `transport.rs` :: `each_level_names_itself_and_the_filter_drops_lower_levels` | `transport.test.ts` :: "each level names itself and the filter drops lower levels" |
| Failures go through `log_exception` with their stack (talker's `handle`) | `transport.rs` :: `log_exception_records_the_failure_and_its_stack` | `transport.test.ts` :: "logException records the failure and its stack" |
| The line format is the writer's fixed one (react-native-logs' `formatFunc` is not offered) | `transport.rs` :: `the_written_line_carries_the_level_the_timestamp_and_the_message` | `transport.test.ts` :: "the written line carries the level, the timestamp and the message" |
| The transport seam is the backend: react-native-logs injects an FS, here the crate writes and tests swap the backend | `transport.rs` :: `the_backend_receives_structured_writes` | `transport.test.ts` :: "the backend receives structured writes" |
| Reading history back | `history.rs` :: `logs_returns_the_recent_entries_in_order` | `history.test.ts` :: "logs returns the recent entries in order" |
| Clearing logs | `history.rs` :: `clean_empties_the_logs_and_the_files` | `history.test.ts` :: "clean empties the logs and the files" |
| The web target without a file sink | `web.rs` :: `a_target_without_a_file_sink_reports_not_available` | `web.test.ts` :: "a target without a file sink rejects with the not available code" |
| Every kind carries its code and its message | `errors.rs` :: `every_kind_carries_its_code_and_its_message` | `errors.test.ts` :: "every kind carries its code and its message" |
| Filesystem failures map to their codes | `errors.rs` :: `filesystem_failures_map_to_their_codes` | `errors.test.ts` :: "filesystem failures map to their codes" |

The `filesystem_failures_map_to_their_codes` tests script `FakeBackend` with already-built `LoggingError` values through the public write, clean and flush paths, proving that each mapped kind surfaces through the API with its code. The `io::Error`-to-kind mapping itself is proven where it lives, by the crate-internal unit test `io_errors_map_to_their_kinds` in `src/error.rs`, which exercises `LoggingError::from_io` — that is how `DiskFull`, `PermissionDenied` and `NotFound` are proven on runners whose disks are not full.
