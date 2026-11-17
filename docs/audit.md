# Audit

Every claim in the planning documents, checked against the code and CI at commit `80fd950` (the pushed `master` HEAD audited from a detached worktree; line numbers are from that snapshot).

CI evidence: run 43998696850 (push to `master`, 2026-11-17) — all nine jobs success: `format`, `rust (ubuntu-latest)`, `rust (macos-latest)`, `rust (windows-latest)`, `check (aarch64-apple-ios)`, `check (aarch64-apple-ios-sim)`, `check (aarch64-linux-android)`, `check (wasm32-unknown-unknown)`, `typescript`. All 21 named Rust edge-case tests plus every unit test ran `ok` on all three desktop runners; the `typescript` job printed "21 tests: 21 passed, 0 failed". Earlier run 43147876579 (2026-11-12) failed: the crate did not compile (E0308, E0599, E0505, E0310, E0277 on the rust/cross-check jobs and inside the `typescript` job's `nsh check`), so the tests could not run at all that day.

Conventions: "green" means run 43998696850. Rows about a reference feature this library refuses ("not planned") are claims of absence; absence is verified by enumerating the public API surface (`crates/nsh-logging/src/lib.rs` exports, `packages/nsh-logging/src/index.ts` exports) — no automated test can pin an absence, so the evidence column says "inspection" and the row is Done only when the doc's replacement behavior is also tested.

Statuses: **Done** (code exists, a test exercises it), **Untested** (code exists, no test proves it), **Partial**, **Missing**, **Blocked** (needs a change outside this repo), **Stale** (doc wrong, code right).

## 1. docs/goals.md — goals

| # | Claim (quoted, with doc and line) | Where it should live in code | Evidence it works (test name or command) | Status |
|---|---|---|---|---|
| 1 | "Every name below exists with the same meaning in Rust and in TypeScript, and each TypeScript test in the edge-case list mirrors its Rust test" (goals.md:11) | `crates/nsh-logging/src/logging.rs`, `packages/nsh-logging/src/logging.ts` | Every name checked pair by pair (rows 12-33); 21 TS tests mirror the 21 Rust tests by name and content; `typescript` + `rust` jobs green | **Done** |
| 2 | "A `rotation` of `never`, `minutely`, `hourly` or `daily` rolls to a new file at the interval, named `prefix.YYYY-MM-DD-HH-mm` and its shorter forms: apple and android let `tracing-appender`'s appender name the files, linux and windows compute the same spelling in their backends" (goals.md:12) | `src/options.rs:34-45` (4 values), `src/platform/apple.rs` (appender names files), `src/platform/linux.rs:335`, `src/platform/windows.rs:321` (local naming) | `time_rotation_rolls_to_a_new_file_at_the_interval` green on 3 OS + TS; `interval_names_spell_dates_as_tracing_appender_does` green (ubuntu/windows); apple delegates naming to tracing-appender — no apple runtime naming test (see row 60) | **Done** |
| 3 | "Bounded disk use … apple and android roll before the write that would outgrow the cap, the live file becoming `prefix.<date>.N`; linux and windows roll when a write finds the file at the cap, renaming the full file to the first free `prefix.<date>.N` and restarting the plain interval name; `max_total_size_bytes` and `max_age` delete the oldest files first, after rotation and after each write; a logger with no caps deletes nothing" (goals.md:13) | `src/logging.rs:58-88` (`enforce_caps`, called after each append and at construction), `src/platform/apple.rs:112-123` (pre-write roll, suffix N), `src/platform/linux.rs:128-141` + `windows.rs:128-141` (rename-aside, `first_free_name`) | `size_cap_rolls_a_file_mid_interval` (both families), `max_total_size_bytes_deletes_the_oldest_files_first`, `max_age_deletes_files_older_than_the_limit`, `rotation_alone_bounds_nothing_until_caps_are_set` — all green on 3 OS + TS. Family rules asserted against a test-local model writer, not the platform backends (row 60) | **Done** |
| 4 | "Token values and personal fields (`token`, `password`, `secret`, `authorization`, `email`, `phone`) are masked before anything is written" (goals.md:14) | `src/redact.rs:48-55`, `src/logging.rs:151`; `packages/.../src/redact.ts` | `tokens_and_personal_fields_are_masked_before_anything_is_written` green on 3 OS + TS (covers all six fields) | **Done** |
| 5 | "Every log call resolves once … The non-blocking writer flushes on exit through its worker guard" (goals.md:15) | `src/logging.rs:217-248`, `Drop for Shared` at 250-259; `WorkerGuard` held in all four native backends | Serialization/timeout/cancel tests green; the drop-flush test exercises the core's one-poll flush against `FakeBackend`, not tracing-appender's `WorkerGuard` (row 44) | **Partial** |
| 6 | "`LoggingError` carries one of the eight kinds below, each with a stable `ERR_LOGGING_` code and a one-sentence message" (goals.md:16) | `src/error.rs:86-115`; `packages/.../src/errors.ts` | `every_kind_carries_its_code_and_its_message` green on 3 OS + TS; all eight codes and messages byte-identical to the goals table | **Done** |
| 7 | "Apple, Android, Windows and Linux write real files through `std::fs`; the web reports `ERR_LOGGING_NOT_AVAILABLE` … any other target gets the same unsupported fallback" proven by "cross-check (each target compiles) and rust + typescript (the fallback path runs on every runner)" (goals.md:17) | `src/platform/*.rs` | cross-check 4/4 green; `a_target_without_a_file_sink_reports_not_available` green on 3 OS + TS — but through a lookalike backend. The crate's own `web.rs` runs nowhere (wasm32 compile-check only) and `unsupported.rs` is compiled by no CI job at all (row 61) | **Partial** |
| 8 | "`cargo fmt --all -- --check` passes" (goals.md:18) | `.github/workflows/ci.yml:36` | `format` job green in 43998696850 | **Done** |
| 9 | "at the end of the run all of it is in the tree, pending CI … nothing counts as proven until that job is green on `master`" (goals.md:5) | CI history | Green `master` run exists (43998696850 on 80fd950); the "pending CI" framing is outdated | **Stale** |
| 10 | "The wasm target is compile-checked only; its runtime behavior is the NotAvailable fallback, which the desktop runners do run" (goals.md:20) | `src/platform/web.rs`, `tests/web.rs` | wasm32 cross-check green; what the desktop runners run is `tests/web.rs`'s own `NoFileSink` copy of the fallback, not `web.rs` itself | **Partial** |

## 2. docs/goals.md — the API (lines 24-39)

| # | Claim | Where | Evidence | Status |
|---|---|---|---|---|
| 11 | "`Logging::new` … opens the directory, prunes, starts the writer" (goals.md:27) | `src/logging.rs:112-116` → `platform::default_backend` → `NativeBackend::new` | Code matches, but no test calls `Logging::new`; every integration test uses `with_backend`. Only `linux.rs`/`windows.rs` unit tests construct a `NativeBackend` directly. Same for TS `new Logging()` with the default `rustBackend` | **Untested** |
| 12 | `logger.log("ordered", Level::Info)` — one call, one message, one chosen level (goals.md:28) | `src/logging.rs:144`; `logging.ts:54` | `each_level_names_itself_and_the_filter_drops_lower_levels` green | **Done** |
| 13 | `debug` / `info` / `warning` / `error` — "each level resolves once written" (goals.md:29-32) | `src/logging.rs:157-174`; `logging.ts:64-81` | Level tests green both sides | **Done** |
| 14 | "`log_exception` … the failure and its stack, as talker's `handle()` takes them" (goals.md:33) | `src/logging.rs:177-187`; `logging.ts:84` | `log_exception_records_the_failure_and_its_stack` green both sides. Rust takes `(error, Option<&str>)`; the goal's one-argument sample is shorthand, not the signature | **Done** |
| 15 | "`logs()` … `Vec<LogEntry>`: what was written, oldest first" (goals.md:34) | `src/logging.rs:190-192`; `logging.ts:89` | `logs_returns_the_recent_entries_in_order` green both sides (also pins the 1000-entry bound) | **Done** |
| 16 | "`clean()` … empties the stored logs and deletes the files" (goals.md:35) | `src/logging.rs:195-208`; `logging.ts:95` | `clean_empties_the_logs_and_the_files` green both sides | **Done** |
| 17 | "`flush()` … drains the non-blocking writer before exit" (goals.md:36) | `src/logging.rs:212-215`; `logging.ts:105` | `flush` asserted in `dropping_the_guard_flushes_every_buffered_write`, TS "flush on exit resolves every buffered write", `a_failure_is_recorded_with_its_stack_and_clean_empties_the_logs` — green | **Done** |
| 18 | "`with_backend` takes a `Backend` in Rust (`FakeBackend` for tests)" (goals.md:24) | `src/logging.rs:119-125`, `src/backend.rs:26-39`, `src/fake.rs` | Every Rust integration test builds through `with_backend` | **Done** |
| 19 | `FakeBackend` exported for tests | `src/fake.rs`, exported `src/lib.rs:21` | Used across 7 of 9 Rust test files | **Done** |
| 20 | "`LoggingOptions` is `#[non_exhaustive]` with a `Default`" (goals.md:39) | `src/options.rs:59-60, 83-102` | `the_defaults_are_the_documented_ones` green | **Done** |
| 21 | "`level` (the least severe level recorded, `debug`)" | `src/options.rs:63, 87` | Defaults test + `a_higher_construction_level_drops_lower_writes` green | **Done** |
| 22 | `directory` | `src/options.rs:65, 126-138` | `an_empty_directory_is_refused_on_native_targets`, invalid-options tests green | **Done** |
| 23 | "`file_base_name` (`"log"`)" | `src/options.rs:67, 90` | Defaults test green; blank name refused in `zero_caps_and_timeouts_are_refused` | **Done** |
| 24 | "`rotation` (`daily`)" with the four values | `src/options.rs:34-45, 91` | Defaults test green; `each_level_and_rotation_has_the_name_tsx_uses` refuses `weekly` | **Done** |
| 25 | `max_file_size_bytes` | `src/options.rs:71` | Zero refused (validate tests); roll behavior in `size_cap_rolls_a_file_mid_interval` | **Done** |
| 26 | `max_total_size_bytes` | `src/options.rs:72` | `max_total_size_bytes_deletes_the_oldest_files_first` green | **Done** |
| 27 | `max_age` | `src/options.rs:73` | `max_age_deletes_files_older_than_the_limit` green | **Done** |
| 28 | "`redact` (the field list above)" — six defaults | `src/options.rs:48-55, 94-98`; `logging.ts:14` | Redaction tests green; defaults asserted in `the_defaults_are_the_documented_ones` | **Done** |
| 29 | "`write_timeout` (off by default; when it passes, the write resolves `ERR_LOGGING_TIMEOUT`)" | `src/options.rs:79, 99`; `src/logging.rs:220-228` | `a_write_past_its_deadline_resolves_with_the_timeout_code` green both sides | **Done** |
| 30 | "Options are fixed at construction: there is no reconfigure call, no runtime severity switch and no on/off flag" (goals.md:39) | `src/options.rs:57-58`; `logging.ts:8-9` | `a_higher_construction_level_drops_lower_writes` green; no such names anywhere in the API (inspection) | **Done** |
| 31 | "TypeScript spells the same names in camelCase (`logException` for `log_exception`)" and mirrors the option fields (goals.md:24, parity.md:14) | `packages/.../src/options.ts` (`fileBaseName`, `maxFileSizeBytes`, `maxTotalSizeBytes`, `maxAgeMs`, `writeTimeoutMs`), `logging.ts` (`logException`) | `nsh check` green; all 21 TS tests exercise the camelCase surface | **Done** |
| 32 | "TypeScript picks `memoryBackend` or `rustBackend`" (goals.md:24) | `packages/.../src/memory.ts:38`, `src/rust.ts:10` | `memoryBackend` used by nearly every TS test, green | **Done** |
| 33 | `rustBackend` behaves as the Rust crate's writer | `packages/.../src/rust.ts` | Compiles (`nsh check`, `nsh build` green) but no test asserts its behavior; `App.tsx` uses it without assertions | **Untested** |

## 3. docs/goals.md — the error model (lines 43-56)

| # | Claim | Where | Evidence | Status |
|---|---|---|---|---|
| 34 | The eight kinds with exactly these codes and messages (goals.md:47-54) | `src/error.rs:88-115`; `packages/.../src/errors.ts:13-23` | `every_kind_carries_its_code_and_its_message` green on 3 OS + TS; all strings identical to the table, including the backticked `max_total_size_bytes` in the DiskFull message | **Done** |
| 35 | "`kind()` … a stable `code()` … and a one-sentence `message()`, modeled on `nsh-biometrics`' `error.rs`" (goals.md:43) | `src/error.rs:54-66`; `Display` prints the code (34, 118-122) | Asserted in the errors tests (`error.to_string() == code`) | **Done** |
| 36 | "No operation panics and none hangs" (goals.md:43) | Bounded waits: `DRAIN_TIMEOUT` (apple.rs:32, android.rs:40), timeout wrap (logging.rs:222), lossy writer + drop counter (apple.rs:64-78) | The deadline test proves a stuck write resolves; no test drives a wedged real worker or a panic path, and absence of panic/hang cannot be proven by test | **Partial** |
| 37 | "the crate-internal unit test `io_errors_map_to_their_kinds` in `src/error.rs`, which exercises `LoggingError::from_io`" (goals.md:86) | `src/error.rs:141-163` (`from_io` at 72-83) | Test exists with that exact name; green on all three OS (raw OS error 28 on unix, 112 on windows for DiskFull) | **Done** |
| 38 | "The `filesystem_failures_map_to_their_codes` tests script `FakeBackend` with already-built `LoggingError` values through the public write, clean and flush paths" (goals.md:86) | `tests/errors.rs:70-111`; `errors.test.ts:47-87` | Both tests do exactly that (append/clean/flush failures + status ride-along), green | **Done** |

## 4. docs/goals.md — the 21 edge cases (lines 62-84)

All 21 Rust tests exist with exactly the names below and ran `ok` on ubuntu, macos and windows in 43998696850; all 21 TS tests exist with exactly the names below and passed in the `typescript` job.

| # | Edge case (goals.md line) | Rust test (verified body) | TypeScript test (verified body) | Status |
|---|---|---|---|---|
| 39 | Time rotation rolls at the interval (64) | `rotation.rs:286` — minutely boundary crossed with an injected clock, asserts `log.2020-02-29-13-05` / `-13-06` and per-file lines | `rotation.test.ts:128` — same shape over `log.2026-11-09-12-30/31` | **Done** (both against scripted writers, not the platform backends) |
| 40 | Rotation alone bounds nothing (65) | `rotation.rs:322` — asserts `removed()` empty, `removes()==0`, `files_reads()==0` | `rotation.test.ts:144` — same three asserts | **Done** |
| 41 | Size-cap roll, one rule per family, "each named test runs both families" (66) | `rotation.rs:345` — predictive family yields `base, base.1, base.2`; rename-aside yields `base.1` holding first+second and `base` restarted | `rotation.test.ts:160` — identical two-family assertions | **Done** (against test-local model writers; see row 60) |
| 42 | `max_total_size_bytes` deletes oldest first (67) | `cleanup.rs:24` — 150 bytes over a 120 cap, shuffled mtimes; oldest removed; caps re-run after write (`files_reads()==2`) | `cleanup.test.ts:4` — same numbers | **Done** |
| 43 | `max_age` deletes older files (68) | `cleanup.rs:55` — 5000 ms old removed, 500 ms kept | `cleanup.test.ts:24` — same | **Done** |
| 44 | "The non-blocking writer flushes on exit" (69) | `writer.rs:156` — asserts the core's `Drop` polls `backend.flush()` exactly once (FakeBackend), remaining-owner and last-owner both checked | `writer.test.ts:9` — flush queues behind unsettled writes and resolves after all land | **Partial** — neither side exercises tracing-appender's `WorkerGuard` flush-on-exit; no test drops a real backend and checks the file without an explicit drain |
| 45 | Concurrent writes serialized, each resolves once (70) | `writer.rs:181` — 25 spawned writes, peak in-flight 1, lines in order, history in order | `writer.test.ts:25` — 25 concurrent calls, all written, in order | **Done** |
| 46 | A write timeout (71) | `writer.rs:217` — paused clock, `ERR_LOGGING_TIMEOUT`, nothing recorded, queue freed | `writer.test.ts:39` — wedged promise, same code, queue freed | **Done** |
| 47 | A cancel path (72) | `writer.rs:246` — scripted `Cancelled` error surfaces once, no retry, next write lands | `writer.test.ts:59` — same via `failNextAppend` | **Done** (scripted by design, goals.md:86; the crate has no other cancel trigger) |
| 48 | Redaction of tokens and personal fields (73) | `redaction.rs:13` — all six defaults plus a shape-masked JWT; `redact: []` passes through | `redaction.test.ts:4` — same | **Done** |
| 49 | Invalid options (74) | `options.rs:15` — zero file cap, zero total cap, zero timeout, blank base name, empty directory; all `ERR_LOGGING_INVALID_ARGUMENT` before the sink is asked | `options.test.ts:4` — same plus a negative cap (unrepresentable in Rust's `u64`) | **Done** |
| 50 | Higher construction level drops lower writes (75) | `options.rs:57` — debug/info/warning dropped, error kept, backend untouched | `options.test.ts:29` — same | **Done** |
| 51 | Levels and the filter; `critical` folds into `error`, `verbose` into `debug` (76) | `transport.rs:52` — four names, strict filter, ordering asserts; `critical`/`verbose` unrepresentable | `transport.test.ts:4` — same plus plain-JS `log("x","verbose"/"critical")` refused with the invalid-argument code | **Done** |
| 52 | Failures through `log_exception` with their stack (77) | `transport.rs:102` — with and without stack, error level, message `failure\nstack` | `transport.test.ts:39` — same | **Done** |
| 53 | Fixed line format, `formatFunc` not offered (78) | `transport.rs:124` — asserts exactly `"info {time_ms} ordered"` | `transport.test.ts:53` — same | **Done** |
| 54 | The transport seam is the backend (79) | `transport.rs:140` — `RecordingBackend` receives structured entries, redaction already applied | `transport.test.ts:63` — same | **Done** |
| 55 | Reading history back (80) | `history.rs:24` — order, levels, non-decreasing stamps, 1010-write flood sheds to the last 1000 | `history.test.ts:4` — same plus copy-on-read | **Done** |
| 56 | Clearing logs (81) | `history.rs:61` — files removed by name, history emptied, a write after clean lands | `history.test.ts:32` — same | **Done** |
| 57 | The web target without a file sink (82) | `web.rs:43` — NotAvailable on write/clean/flush, `logs()` stays an empty answer | `web.test.ts:4` — same | **Done** (via lookalike backends; see row 61) |
| 58 | Every kind carries its code and its message (83) | `errors.rs:57` | `errors.test.ts:32` | **Done** |
| 59 | Filesystem failures map to their codes (84) | `errors.rs:70` | `errors.test.ts:47` | **Done** |

Weaker-than-claimed or uncovered areas, called out as their own rows:

| # | Callout | Where | Evidence | Status |
|---|---|---|---|---|
| 60 | The real size-roll code paths — apple/android `reopen` with `filename_suffix(N)` (apple.rs:112-123, 271-295; android.rs:119-144) and linux/windows `roll_on_size` + `first_free_name` (linux.rs:128-141, 387-398; windows.rs same) — are never executed by any test; the family-rule tests run a re-implementation of the rules inside the tests | `src/platform/*.rs` | `rotation.rs`/`rotation.test.ts` use `IntervalWriter`/`PredictiveRollWriter`/`RenameAsideRollWriter`; `appended_lines_drain_to_the_directory_and_survive_deletion` (linux.rs:492, windows.rs:462) sets no size cap | **Untested** |
| 61 | The crate's own fallback backends at runtime: `web.rs` is compiled by the wasm32 cross-check only and never runs; `unsupported.rs` ("any other target") is compiled by no CI job and has no test | `src/platform/web.rs`, `src/platform/unsupported.rs` | cross-check wasm32 green (compile); the "fallback path runs on every runner" claim is met by `tests/web.rs`'s own `NoFileSink` | **Untested** |
| 62 | "A test counts only once it has been seen to fail" (goals.md:60) | CI history | Run 43147876579 (2026-11-12) shows the crate failing to compile on every rust/cross-check job and inside `nsh check` — the tests could not run, a stronger failure. No log anywhere shows a named test failing at an assertion and later passing; assertion-level red/green is not demonstrated | **Partial** |

## 5. docs/parity.md

| # | Claim (parity.md line) | Where | Evidence | Status |
|---|---|---|---|---|
| 63 | "A row whose code exists … says **implemented, pending CI** until a green `master` run covers it … every mapped row below reads **implemented, pending CI**" (12) | CI | Green run 43998696850 exists on `master`; every "pending CI" marker below is outdated | **Stale** |
| 64 | One API in two packages; TS camelCase (`logException`, `maxFileSizeBytes`) (14) | `src/lib.rs:19-25`, `packages/.../src/index.ts` | Exports verified; TS tests green | **Done** |
| 65 | `Talker({logger, observer, settings, filter, errorHandler, history})` → `Logging::new` over one options struct; pluggable piece is the backend (22) | `src/logging.rs:112, 119`; `logging.ts:39` | Construction + backend seam tested throughout | **Done** |
| 66 | `configure({ ... })` → nothing (23) | — | No such name in either API (inspection); fixed-at-construction tested (row 30) | **Done** |
| 67 | `stream` → nothing; "reading back goes through `logs()`" (24) | — | `logs()` green; no stream exists (inspection) | **Done** |
| 68 | `history` (getter) → `logs()`: "the entries live in files under `directory`, not in an in-memory list" (25) | `src/logging.rs:26, 190-192` | `logs()` works (tested), but it serves an in-memory `VecDeque` capped at 1000 entries — the storage claim is wrong. TS identical (`logging.ts:8, 91`) | **Stale** |
| 69 | `filter`/`TalkerFilter` → severity half survives as `LoggingOptions.level`; keys/search no counterpart (26) | `src/options.rs:63` | Level filter green; no key/search API (inspection) | **Done** |
| 70 | `handle(exception, [stackTrace, msg])` → `log_exception` (27) | `src/logging.rs:177` | Edge-case row 52 green | **Done** |
| 71 | `log(message, { logLevel })` → `log`, same — same name and behavior (28) | `src/logging.rs:144` | Row 12 green | **Done** |
| 72 | `log(message, { exception, stackTrace })` → failures go through `log_exception`; `log` takes only a message and a level (29) | `src/logging.rs:144, 177` | Signatures verified; both tested | **Done** |
| 73 | `log(message, { pen })` → nothing (30) | — | Plain text lines; no pen anywhere (inspection) | **Done** |
| 74 | `logCustom(TalkerLog)`, deprecated `logTyped` → nothing (31) | — | No subclassable model (inspection) | **Done** |
| 75 | `critical` → recorded through `error` (32) | `src/options.rs:17`, `native.rs:236` | `parse_level("critical") == None` (unit test green); TS refusal test green | **Done** |
| 76 | `debug` → `debug`, same (33) | `src/logging.rs:157` | Level tests green | **Done** |
| 77 | `error` → `error`, same (34) | `src/logging.rs:172` | Level tests green | **Done** |
| 78 | `info` → `info`, same (35) | `src/logging.rs:162` | Level tests green | **Done** |
| 79 | `verbose` → recorded through `debug` (36) | `src/options.rs:11`, `native.rs:237` | Refusal tests green both sides | **Done** |
| 80 | `warning` → `warning`, same (37) | `src/logging.rs:167` | Level tests green | **Done** |
| 81 | `cleanHistory()` → `clean` (38) | `src/logging.rs:195` | Row 56 green | **Done** |
| 82 | `enable()` / `disable()` → nothing (39) | — | No runtime switch (inspection); fixed level tested | **Done** |
| 83 | `TalkerSettings.enabled` → nothing (45) | — | Inspection | **Done** |
| 84 | `useHistory` → nothing (46) | — | Inspection | **Done** |
| 85 | `useConsoleLogs` → nothing; ours writes files (47) | — | No console write anywhere in `src/` (grep); file writes tested on linux/windows | **Done** |
| 86 | `maxHistoryItems` → "retention is bounded by bytes and age, not a count" (48) | `src/logging.rs:58-88` | Files: bytes/age caps green (cleanup tests). Read-back: bounded by a 1000-entry count (`HISTORY_LIMIT`), asserted by the history tests — the "not a count" half is wrong for `logs()` | **Partial** |
| 87 | `titles`, `colors`, `timeFormat`, `registeredKeys`/`registerKeys()`, `copyWith()`, `getTitleByKey()`/`getPenByKey()` → nothing (49-54) | — | None of these names exists in either API (inspection); the timestamp format is fixed by `format_line` (tested, row 53) | **Done** |
| 88 | `writeToFile` → "this crate is that row: `directory`, `file_base_name`, `rotation` and the size and age caps are the file output talker removed" (55) | `src/options.rs`, `src/platform/*.rs` | Rotation/caps/options tests green (real files proven on linux/windows runners; apple/android compile-checked) | **Done** |
| 89 | `LogLevel` set → four levels, folds (61) | `src/options.rs:9-19` | Row 51 green | **Done** |
| 90 | `TalkerData` → nothing; "no public log model; reading back goes through `logs()`" (62) | — | `LogEntry` is the read-back shape, not a subclassable model (inspection); `logs()` green | **Done** |
| 91 | `TalkerLog` → nothing (63) | — | Inspection | **Done** |
| 92 | `TalkerError` / `TalkerException` → failures through `log_exception` (64) | `src/logging.rs:177` | Row 52 green | **Done** |
| 93 | `List<TalkerData>.text()` → nothing (65) | — | Inspection | **Done** |
| 94 | `firstWhereLogTypeOrNull` → nothing (66) | — | Inspection | **Done** |
| 95 | `TalkerHistory` → "internal in ours; the surface is `logs()` and `clean`, and the bound is bytes and age, not an item count" (67) | `src/logging.rs:26, 190, 195` | Surface green; the bound claim is wrong for `logs()` (1000-entry in-memory cap, tested) | **Stale** |
| 96 | `TalkerObserver` → nothing; "every call resolves once with success or a coded `LoggingError`" (68) | — | No callbacks (inspection); resolve-once tests green | **Done** |
| 97 | `TalkerKey` → nothing (69) | — | Inspection | **Done** |
| 98 | `TalkerFilter.copyWith()` → nothing (70) | — | Inspection | **Done** |
| 99 | `TimeFormat` → nothing; format fixed by the writer (71) | `src/logging.rs:40-42` | Row 53 green | **Done** |
| 100 | `TalkerDateTimeFormatter` → nothing (72) | — | Inspection | **Done** |
| 101 | `TalkerJsonFormatter` → nothing (73) | — | Inspection | **Done** |
| 102 | `TalkerLogger.log` and its level shortcuts → nothing; ours writes files (79) | — | No console logging methods (inspection) | **Done** |
| 103 | `TalkerLoggerSettings.level` → `LoggingOptions.level` (80) | `src/options.rs:63` | Row 50 green | **Done** |
| 104 | `colors`/`enableColors`, `enable`, `defaultTitle`, `lineSymbol`/`maxLineWidth`, `TalkerLoggerSettings.copyWith()`, `TalkerLogger.copyWith()`, `ConsoleUtils.getUnderline()`/`getTopline()`, `LoggerFormatter` → nothing (81-88) | — | None of these names exists (inspection); the line format is fixed (row 53) | **Done** |
| 105 | `LogDetails` → nothing; "the `Backend` trait defines its own write shape" (89) | `src/backend.rs:26-39` | Row 54 green | **Done** |
| 106 | `LoggerFilter` → the severity gate in `LoggingOptions.level` (90) | `src/options.rs:63` | Row 51 green | **Done** |
| 107 | `LogLevelFilter` → the same severity gate (91) | same | Row 51 green | **Done** |
| 108 | `logLevelPriorityList` → nothing; "no priority list is exported" (92) | — | `LEVELS` is module-private in `logging.ts` and unexported; Rust `Level` ordering is derived (inspection) | **Done** |
| 109 | `LoggerOutput` → the `Backend` trait is the seam, "takes structured writes, not formatted strings" (93) | `src/backend.rs:29` (`append(&self, entry: &LogEntry)`) | Row 54 green | **Done** |
| 110 | `createLogger(config)` → `Logging::new` over `LoggingOptions` (TS `new Logging(options)`), "no factory wrapper" (101) | `src/logging.rs:112`; `logging.ts:39` | Tested via `with_backend`/`backend` config; `Logging::new` itself untested (row 11) | **Done** |
| 111 | `severity` → `LoggingOptions.level` at construction (102) | `src/options.rs:63` | Row 50 green | **Done** |
| 112 | `transport`/`transports` → the `Backend` trait; "one backend per logger, no per-call array" (103) | `src/logging.rs:119`, `src/backend.rs` | One backend per `Shared`; no array anywhere (inspection); backend tests green | **Done** |
| 113 | `transportOptions` → `LoggingOptions` is the single options bag (104) | `src/options.rs:61-81` | Options tests green; no second options object exists | **Done** |
| 114 | `levels` (custom level map) → nothing; custom levels not accepted (105) | — | TS refusal test (row 51); Rust enum makes them unrepresentable | **Done** |
| 115 | `async` → "every call resolves exactly once … bounded by `write_timeout`; no idle-time deferral" (106) | `src/logging.rs:217-248`; `logging.ts:125-155` | Rows 45-47 green | **Done** |
| 116 | `asyncFunc` → nothing (107) | — | Inspection | **Done** |
| 117 | `stringifyFunc` → nothing; "`redact` is the only transformation before writing" (108) | `src/logging.rs:148-153` | Redaction tests green; no stringify step exists | **Done** |
| 118 | `formatFunc` → nothing; format fixed by the writer (109) | `src/logging.rs:40` | Row 53 green | **Done** |
| 119 | `dateFormat` → nothing (110) | — | Row 53 pins the fixed stamp | **Done** |
| 120 | `printLevel` / `printDate` → nothing (111) | — | Row 53: the line always carries level and stamp | **Done** |
| 121 | `fixedExtLvlLength` → nothing (112) | — | Inspection | **Done** |
| 122 | `enabled` → nothing (113) | — | Inspection; fixed level tested | **Done** |
| 123 | `enabledExtensions` → nothing (114) | — | Inspection | **Done** |
| 124 | `printFileLine` / `fileLineOffset` → nothing (115) | — | Inspection | **Done** |
| 125 | `debug`, `info`, `error` → same names (121) | Both APIs | Level tests green | **Done** |
| 126 | `warn` → `warning` (122) | `src/logging.rs:167` | Tests use `warning` throughout | **Done** |
| 127 | Level methods from a custom `levels` map → nothing (123) | — | Row 114 | **Done** |
| 128 | `extend(extension)` → nothing (124) | — | Inspection | **Done** |
| 129 | `enable()` / `disable()` → nothing (125) | — | Inspection | **Done** |
| 130 | `getExtensions()` → nothing (126) | — | Inspection | **Done** |
| 131 | `setSeverity(level)` / `getSeverity()` → nothing (127) | — | Row 50 proves construction fixes it; no such names (inspection) | **Done** |
| 132 | `patchConsole()` → nothing (128) | — | Inspection | **Done** |
| 133 | `consoleTransport` → nothing (134) | — | Inspection | **Done** |
| 134 | `mapConsoleTransport` → nothing (135) | — | Inspection | **Done** |
| 135 | `fileAsyncTransport` → "this is the crate's core" (136) | `src/platform/*.rs` | Real-file write/drain/delete proven on linux (ubuntu) and windows runners by `appended_lines_drain_to_the_directory_and_survive_deletion`; apple has no real-write test, android is compile-checked only | **Partial** |
| 136 | `fileAsyncTransport` option `FS` → "nothing is injected" (137) | `src/platform/*.rs`; `packages/.../src/memory.ts`, `rust.ts` | No filesystem parameter anywhere; TS picks a backend (inspection + tests) | **Done** |
| 137 | `fileName` (`{date-today}`) → `file_base_name` + rotation schedule naming (138) | `src/platform/linux.rs:335`, apple via tracing-appender | Row 2 green | **Done** |
| 138 | `fileNameDateType` → rollover follows `rotation`; format not an option (139) | `src/options.rs:34` | Row 2/53 green | **Done** |
| 139 | `filePath` → `LoggingOptions.directory` (140) | `src/options.rs:65` | Row 22 green | **Done** |
| 140 | `sentryTransport` → nothing (141) | — | Inspection | **Done** |
| 141 | `crashlyticsTransport` → nothing (142) | — | Inspection | **Done** |
| 142 | `transportFunctionType` → the `Backend` trait, `FakeBackend`, TS `memoryBackend`/`rustBackend` (143) | `src/backend.rs`, `src/fake.rs`, `memory.ts`, `rust.ts` | Fake/memory backends green; `rustBackend` untested (row 33) | **Done** |
| 143 | "`flush`, `with_backend`, `redact`, `write_timeout`, the three caps, `FakeBackend` and the `ERR_LOGGING_` codes are this library's own … `flush` drains the writer before exit" (145) | Throughout | Each tested green (rows 17-29, 34, 38) | **Done** |

## 6. docs/decisions.md

| # | Claim (decisions.md line) | Where | Evidence | Status |
|---|---|---|---|---|
| 144 | "On 2026-11-09 all five reference repositories were cloned successfully into `.refs/`" (13) | — | `.refs/` is gitignored (`.gitignore:4`) and absent from the tree; nothing in the repo or CI can verify this | **Untested** |
| 145 | "built and tested with NSH 0.5.1 exactly (`NSH_VERSION: v0.5.1` … installed from SuperNSH/nsh-builds) … The Rust crate's `nsh-core` dependency is `0.5.1`" (17) | `.github/workflows/ci.yml:108-111`, `crates/nsh-logging/Cargo.toml:16` | ci.yml installs `v0.5.1` from `https://raw.githubusercontent.com/SuperNSH/nsh-builds/master/install.sh`; typescript job green | **Done** |
| 146 | "`non_blocking` … hands lines to a dedicated worker thread, and the `WorkerGuard` it returns flushes every buffered line when the process exits, which is the flush-on-exit edge case the brief demands" (21) | `WorkerGuard` held in all four native backends (apple.rs:48, android.rs:61, linux.rs:39, windows.rs:39) | Guards are held; no test exercises guard-drop flush on a real writer (row 44). The core's own drop-flush and explicit `flush()` are tested | **Partial** |
| 147 | "`RollingFileAppender` rotates `never`, `minutely`, `hourly`, `daily` and `weekly` … This library's `rotation` option takes four of the crate's five intervals … `weekly` stays tracing-appender's" (21) | `src/options.rs:34-45`, `src/platform/*.rs` (`file_rotation`), `native.rs:180-188` | Four values offered; `parse_rotation("weekly") == None` unit test green; each backend maps the four | **Done** |
| 148 | "its builder's `max_log_files` prunes old files. What it does not do is bound a file's size … so `max_file_size_bytes`, `max_total_size_bytes` and `max_age` are written here on top" (21) | — | `max_log_files` is used nowhere in `src/` (grep); the three caps are implemented and tested (rows 3, 25-27). tracing-appender's own behavior is an external fact, not verifiable here | **Done** |
| 149 | "the size cap rolls mid-interval under two family rules — apple and android roll before the write that would outgrow the cap, the live file carrying `prefix.<date>.N`; linux and windows rename the full file to the first free `prefix.<date>.N` … and restart the plain interval name" (21) | `apple.rs:112-123, 271-295`; `android.rs:119-144, 413-437`; `linux.rs:128-141, 387-398`; `windows.rs` same | Code read line by line and matches both rules; no runtime test executes these paths (row 60) | **Partial** |
| 150 | "The total and age caps delete the oldest files first after each rotation and each write" (21) | `src/logging.rs:58-88` (`enforce_caps` after every append, which contains the roll, and at construction) | Cleanup tests green both sides | **Done** |
| 151 | "Storage is `std::fs` on native targets" (21) | All four native backends use `std::fs` exclusively | Real-file tests green on linux/windows; apple/android compile-checked | **Done** |
| 152 | "Where a host provides a filesystem instead … the crate goes through `nsh_core::filesystem`'s `FileSystem` capability, whose requests carry normalized relative paths under the app's private root. That seam is also where the tests script disk-full, permission-denied and not-found" (21) | — | No `nsh_core::filesystem` usage exists anywhere (grep); `web.rs:3-8` itself says the seam does not exist yet and every call resolves NotAvailable; the failure scripting lives in `FakeBackend`/`memoryBackend`, not in any filesystem seam | **Stale** |
| 153 | "carries: `nsh-core` 0.5.1, `thiserror` 2.0.21, `tokio` 1.54.1 (with `sync` and `time`), `tracing-appender` 0.2.5 — held off `wasm32` … on Android only, `jni` 0.22.4 with `ndk-context` 0.1.1 … caret requirements, not exact pins … `Cargo.lock` is not committed" (25) | `crates/nsh-logging/Cargo.toml:15-31`, `.gitignore:3` | Manifest matches every version and target gate exactly; lock ignored; green run resolved them. ("match the org template's crate" is out-of-repo and unverifiable.) | **Done** |
| 154 | "The web has no app-private log directory … file logs report `ERR_LOGGING_NOT_AVAILABLE` there … Any other target gets the same unsupported fallback, so the NotAvailable path is one code path, tested on every runner rather than only where it is real" (29) | `src/platform/web.rs`, `unsupported.rs`, `tests/web.rs` | Web/unsupported code returns NotAvailable uniformly (read); what every runner tests is a lookalike backend; `unsupported.rs` compiled by no CI job (row 61) | **Partial** |
| 155 | "ours keeps four of the six — `debug`, `info`, `warning`, `error` — folding `critical` into `error` and `verbose` into `debug`"; `handle()` → `log_exception`; severity fixed at construction; file transport is the core (33) | `src/options.rs`, `src/logging.rs` | Rows 14, 30, 51, 135 | **Done** |
| 156 | "its history read-back (`logs()`, stored as files, not a Dart list)" (33) | `src/logging.rs:26, 190` | `logs()` reads an in-memory bounded history; files are written but never read back | **Stale** |
| 157 | "Console output, custom line formatting and runtime reconfiguration are deliberately not offered" (33) | — | Inspection: no console writes, fixed format (tested, row 53), no reconfigure. Minor wart: android's caught `ndk-context` probes each print one stderr line (android.rs:291-295, acknowledged in its comment) | **Done** |

## 7. docs/brief.md

| # | Claim (brief.md line) | Where | Evidence | Status |
|---|---|---|---|---|
| 158 | "tracing-appender: Rust non-blocking writer, time-based rotation" (7) | `Cargo.toml:23`, `src/platform/*.rs` | Green cross-checks and rust runs | **Done** |
| 159 | "Size-based cleanup must be added on top" (8) | `src/logging.rs:58-88`, backends' roll paths | Cleanup + rotation tests green | **Done** |
| 160 | "Redact tokens and personal fields before writing" (9) | `src/redact.rs` | Row 48 green | **Done** |
| 161 | Edge case: "Time-based rotation has no size cap" (17) → the cap exists | `src/options.rs:71` | Row 41 green (with row 60 caveat) | **Done** |
| 162 | Edge case: "Unbounded growth fills the disk" (18) → bounded | `src/options.rs:72-73` | Rows 42-43 green | **Done** |
| 163 | Edge case: "Non-blocking writers must flush on exit" (19) | `WorkerGuard` + `flush()` + core `Drop` | Explicit flush tested; guard-drop flush untested (row 44) | **Partial** |
| 164 | "Research done 2026-11-09 … Versions and licences are unverified until checked on the registry before pinning" (23) | — | Historical note; versions are now pinned and proven by a green run that resolved them fresh (lock not committed) | **Done** |

## 8. README.md

| # | Claim (README.md line) | Where | Evidence | Status |
|---|---|---|---|---|
| 165 | "Local log files with rotation, bounded disk use and redaction." (3) | The crate | Rows 2-4 green | **Done** |
| 166 | "a web status page on GitHub Pages at https://supernsh.github.io/logging/" (5) | `.github/workflows/pages.yml` | `gh api repos/SuperNSH/logging/pages`: `html_url` is that URL, `build_type: workflow`; pages run 43998696820 success on 80fd950; `curl` returns 200 | **Done** |
| 167 | Status: "Implemented, pending this run's first CI check … No green CI run exists, so nothing on this page is proven" (9) | CI history | Run 43998696850 (master, 80fd950, 2026-11-17): all nine CI jobs success and the pages run success — a green run exists | **Stale** |
| 168 | "Writes log lines to local files through a non-blocking writer, rotated by time (`never`, `minutely`, `hourly`, `daily`)." (13) | Backends | Row 2 green | **Done** |
| 169 | "Bounds disk use with `max_file_size_bytes`, `max_total_size_bytes` and `max_age`: an oversized file rolls mid-interval — on apple and android before the write that would outgrow the cap, on linux and windows by renaming the full file aside once it reaches the cap — and the oldest files are deleted first." (14) | `src/platform/*.rs`, `src/logging.rs` | Rows 3, 41-43 green; real-backend roll paths untested (row 60) | **Done** |
| 170 | "Redacts tokens and personal fields before anything is written." (15) | `src/redact.rs` | Row 48 green | **Done** |
| 171 | "Resolves every call exactly once: success, a coded failure such as `ERR_LOGGING_DISK_FULL`, or a cancel." (16) | `src/logging.rs` | Rows 45-47, 59 green | **Done** |
| 172 | "One API with the same names in Rust … and TypeScript … What is deliberately not offered … is recorded row by row in docs/parity.md" (17) | Both APIs | Row 1 green; parity section above | **Done** |
| 173 | Layout: `crates/nsh-logging/` (src + tests), `packages/nsh-logging/` ("`tests/` mirrors each Rust test"), the four docs files, `site/`, `.github/workflows/ci.yml` (21-28) | Tree | All paths exist; 21/21 tests mirrored by name; `nsh.json` points `rust` at the crate | **Done** |
| 174 | Checks: format job runs `cargo fmt --all -- --check`; rust job runs clippy `--all-targets --all-features -- -D warnings` and `cargo test --all-features --no-fail-fast` on the 3-OS matrix; cross-check covers the four targets with the stated OSes and the NDK toolchain; typescript "installs NSH 0.5.1" and runs `nsh check`, `nsh build` failing on any warning, `nsh test` (34-37) | `.github/workflows/ci.yml` | Matches command for command (ci.yml:36, 63-64, 74-100, 107-120); NSH install is `NSH_VERSION: v0.5.1` from `SuperNSH/nsh-builds` install.sh; all jobs green | **Done** |
| 175 | "There is no demo or publish job: this repo has no demo app. `site/` is published by the separate `.github/workflows/pages.yml`." (39) | Workflows, releases | ci.yml has no demo/publish job; `gh release list` is empty; pages.yml deploys `site/` | **Done** |
| 176 | "Until a run of these jobs is green, every status claim above is unverified." (39) | CI | The condition is now met by 43998696850 — but the Status section above it still says no green run exists (row 167) | **Done** (sentence is conditional and now satisfied) |
| 177 | "MIT. See LICENSE." and "Repository: https://github.com/SuperNSH/logging" (43-45) | `LICENSE`, `Cargo.toml:8-9`, `package.json:17` | LICENSE is the MIT License (c) 2026 hmziqrs; URLs match | **Done** |

## 9. CI workflow vs what goals.md/README claim proves each criterion

| # | Claim | Evidence | Status |
|---|---|---|---|
| 178 | ci.yml "runs on every push to `master` and on pull requests" (README:32) | ci.yml:7-11 (`push: branches: [master]`, `pull_request:`, `workflow_dispatch:`); run 43998696850 is a push-to-master run | **Done** |
| 179 | `format` job = `cargo fmt --all -- --check` | ci.yml:27-36; green | **Done** |
| 180 | `rust` job: 3-OS matrix, clippy + test commands as documented; the named tests run there | ci.yml:38-64; green 3/3; per-OS log shows all 21 named tests plus unit tests `ok` on each runner; `platform::linux::*` tests ran only on ubuntu, `platform::windows::*` only on windows, `platform::apple::*` only on macos | **Done** |
| 181 | `cross-check`: iOS + iOS-sim on macOS, android on Ubuntu with the NDK toolchain, wasm32 on Ubuntu, each `cargo clippy --target … --all-features -- -D warnings` | ci.yml:66-100; all four green | **Done** |
| 182 | `typescript` job: installs NSH 0.5.1, then `nsh check`, `nsh build` with no warnings, `nsh test` | ci.yml:102-120; green; build log contained no `warn` lines (the `! grep -i warn build.log` gate passed); `nsh test` printed "21 tests: 21 passed, 0 failed" | **Done** |
| 183 | pages.yml publishes `site/` on every push to master | pages.yml:1-34; run 43998696820 success on 80fd950; site serves 200 | **Done** |

## 10. NSH 0.4.0 / 0.5.0 mentions

| # | Mention | Verdict | Status |
|---|---|---|---|
| 184 | decisions.md:17 — "It started on 0.4.0, the version the org template's CI pinned, moved to 0.5.0 with the SuperNSH set on 2026-11-11, and to 0.5.1 … on 2026-11-17." | The only 0.4.0/0.5.0 mention in the tree (grep over docs/, README, src, .zcode/, site/, workflows). History narrative; the current pin (0.5.1) is verified in ci.yml and Cargo.toml. Keep as history — accurate as of 0.5.1 | **Done** |

## 11. .zcode and site

| # | Claim | Where | Evidence | Status |
|---|---|---|---|---|
| 185 | `.zcode/workflow.ts`: every phase status "not-started" (lines 25-30) | The repo | Implementation, unit tests and green CI exist; all six statuses are wrong. Also its gate list (16-18: clippy without `--all-features`, `cargo test --all-targets`) differs from what ci.yml actually runs. (`.zcode/workflow-drafts/` is gitignored and absent — nothing to audit.) | **Stale** |
| 186 | site/index.html:23 — "Status: scaffold. No implementation yet." | `site/`, the live page | Implementation exists and CI is green; the deployed page at https://supernsh.github.io/logging/ still says "scaffold" (verified by fetch of the live page, deployed from 80fd950) | **Stale** |

## Summary

186 rows. Done 160. Partial 12: rows 5, 7, 10, 36, 44, 62, 86, 135, 146, 149, 154, 163. Untested 5: rows 11, 33, 60, 61, 144. Stale 9: rows 9, 63, 68, 95, 152, 156, 167, 185, 186. Missing 0, Blocked 0.

The recurring gaps, in one place:

- The two platform-family size-roll rules and the real apple write path have no runtime test; the rotation edge-case tests prove the rules against a model writer written inside the tests (row 60).
- `Logging::new`/default `rustBackend` — the real platform path both APIs advertise — is never called by a test (rows 11, 33).
- No test exercises tracing-appender's `WorkerGuard` flush-on-exit on a real writer (rows 44, 146, 163).
- `web.rs` runs nowhere and `unsupported.rs` is compiled by no CI job (rows 61, 154).
- The "seen to fail once" rule is evidenced only by a compile-failure run, not assertion-level red/green (row 62).
- Doc/code contradictions: `logs()` is an in-memory bounded history, not a file read-back (rows 68, 86, 95, 156); the `nsh_core::filesystem` capability seam described in decisions.md does not exist in the code (row 152).
- Stale status text: README Status section, parity.md's "pending CI" preamble, `.zcode/workflow.ts`, and the live Pages site all predate the green run (rows 63, 167, 185, 186).
