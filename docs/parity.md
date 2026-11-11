# Parity with `talker` and `react-native-logs`

One row per public API of each reference, what this library does for it, and the row's status. The references were read on 2026-11-09: `talker` 5.2.0 and `talker_logger` 5.2.0 (`.refs/talker/packages/talker`, `.refs/talker/packages/talker_logger`), `react-native-logs` 5.6.0 (`.refs/react-native-logs/src`).

Status words, exactly one per row:

- **same**: same name and behavior.
- **adapted**: ours covers it under a different name or shape; the row says how and why.
- **accepted, no effect**: the option is taken but does nothing, the way the biometrics parity rows handle platform-specific prompt options. No row here holds it: every reference option either maps to something real or is refused.
- **not planned**: ours does nothing for it; the row says why in one line.

No row is proven until CI runs a test for it. A row whose code exists in the working tree says **implemented, pending CI** until a green `master` run covers it. The crate and the TypeScript package are written in the working tree, so every mapped row below reads **implemented, pending CI**.

Ours is one API in two packages: the Rust crate `nsh-logging` (`Logging`, `LoggingOptions`, `LoggingError`, `Backend`, `FakeBackend`) and the TypeScript package `nsh-logging` (`Logging`, `LoggingOptions`, `LoggingError`, `memoryBackend`, `rustBackend`). TypeScript spells the same names in camelCase: `logException` for `log_exception`, `maxFileSizeBytes` for `max_file_size_bytes`.

## Flutter `talker`

### The `Talker` class

| `talker` | Ours | Status |
|---|---|---|
| `Talker({ logger, observer, settings, filter, errorHandler, history })` | `Logging::new` over one `LoggingOptions` struct (TS: `new Logging(options)`); the pluggable piece is the backend (`with_backend` in Rust, `memoryBackend` or `rustBackend` in TS), not talker's six injected collaborators | adapted — implemented, pending CI |
| `configure({ ... })` | nothing: options are fixed at construction; the API has no reconfigure call | not planned |
| `stream` | nothing: no event stream; reading back goes through `logs()` | not planned |
| `history` (getter) | `logs()` (TS: `logs()`): reads back what was written; the entries live in files under `directory`, not in an in-memory list | adapted — implemented, pending CI |
| `filter` / `TalkerFilter` (`enabledKeys`, `disabledKeys`, `searchQuery`) | the severity half survives as `LoggingOptions.level`; key lists and search queries have no counterpart | adapted — implemented, pending CI |
| `handle(exception, [stackTrace, msg])` | `log_exception` (TS: `logException`) records the failure; talker's Error/Exception split and observer dispatch are not kept | adapted — implemented, pending CI |
| `log(message, { logLevel })` | `log` (TS: `log`): one call, one message, one chosen level | same — implemented, pending CI |
| `log(message, { exception, stackTrace })` | failures go through `log_exception` (`logException`), which owns the failure shape; `log` takes only a message and a level | adapted — implemented, pending CI |
| `log(message, { pen })` | nothing: the files are plain text, so a console pen has no meaning there | not planned |
| `logCustom(TalkerLog)`, deprecated `logTyped` | nothing: there is no log model to subclass; the facade creates every entry | not planned |
| `critical(msg, [exception, stackTrace])` | recorded through `error`, the highest of the four levels; the fixed level set has no critical | adapted — implemented, pending CI |
| `debug(msg, [exception, stackTrace])` | `debug` (TS: `debug`) | same — implemented, pending CI |
| `error(msg, [exception, stackTrace])` | `error` (TS: `error`) | same — implemented, pending CI |
| `info(msg, [exception, stackTrace])` | `info` (TS: `info`) | same — implemented, pending CI |
| `verbose(msg, [exception, stackTrace])` | recorded through `debug`, the lowest level; the fixed level set has no verbose | adapted — implemented, pending CI |
| `warning(msg, [exception, stackTrace])` | `warning` (TS: `warning`) | same — implemented, pending CI |
| `cleanHistory()` | `clean` (TS: `clean`): same act, renamed because it empties the stored logs and files, not a Dart list | adapted — implemented, pending CI |
| `enable()` / `disable()` | nothing: no runtime switch; what is recorded was fixed by `LoggingOptions.level` at construction | not planned |

### `TalkerSettings`

| `talker` | Ours | Status |
|---|---|---|
| `enabled` | nothing: no on/off flag; the construction-time `level` decides what is recorded | not planned |
| `useHistory` | nothing: writing is the library's whole job; there is no history-off switch | not planned |
| `useConsoleLogs` | nothing: ours writes files; console output is the shell's job | not planned |
| `maxHistoryItems` | retention is bounded by bytes and age, not a count: `max_file_size_bytes`, `max_total_size_bytes`, `max_age` | adapted — implemented, pending CI |
| `titles` | nothing: entries carry a level and a message; there are no per-key titles | not planned |
| `colors` | nothing: plain files, no pens | not planned |
| `timeFormat` | nothing: the writer fixes the timestamp format; it is not an option | not planned |
| `registeredKeys` / `registerKeys()` | nothing: keys exist for talker's screen filters; ours has no UI | not planned |
| `copyWith()` | nothing: options are set once at `new`; there is no settings object to copy | not planned |
| `getTitleByKey()` / `getPenByKey()` | nothing: no titles or pens to look up (see `titles` and `colors`) | not planned |
| `writeToFile` (commented out in talker 5.2.0; core never shipped file output) | this crate is that row: `directory`, `file_base_name`, `rotation` and the size and age caps are the file output talker removed | adapted — implemented, pending CI |

### Models, levels, history, observer, utils

| `talker` | Ours | Status |
|---|---|---|
| `LogLevel` (`error`, `critical`, `info`, `debug`, `verbose`, `warning`) | four levels behind `LoggingOptions.level`: `debug`, `info`, `warning`, `error`; critical folds into `error`, verbose into `debug` | adapted — implemented, pending CI |
| `TalkerData` (`message`, `key`, `logLevel`, `exception`, `error`, `stackTrace`, `title`, `time`, `pen`) | nothing: no public log model; reading back goes through `logs()` | not planned |
| `TalkerLog` | nothing: no subclassable log type (see `logCustom`) | not planned |
| `TalkerError` / `TalkerException` | failures are recorded through `log_exception` (`logException`); there is no separate Error or Exception model | adapted — implemented, pending CI |
| `List<TalkerData>.text()` | nothing: no text aggregation; callers read `logs()` and format what they get | not planned |
| `firstWhereLogTypeOrNull` (`TalkerIterableLogTypeModifier` on `Iterable<TalkerLogType>`) | nothing: no log types to search; callers filter what `logs()` returns | not planned |
| `TalkerHistory` (`write`, `clean`, `history`) | internal in ours; the surface is `logs()` and `clean`, and the bound is bytes and age, not an item count | adapted — implemented, pending CI |
| `TalkerObserver` (`onError`, `onException`, `onLog`) | nothing: no callbacks; every call resolves once with success or a coded `LoggingError` | not planned |
| `TalkerKey` (string keys: base, http, bloc, riverpod, grpc and route sections; the riverpod section includes the four mutation keys) | nothing: entries carry a level, not a key | not planned |
| `TalkerFilter.copyWith()` | nothing: no filter object exists to copy (see `filter`) | not planned |
| `TimeFormat` (`timeAndSeconds`, `yearMonthDayAndTime`) | nothing: the timestamp format is fixed by the writer; it is not an option | not planned |
| `TalkerDateTimeFormatter` | nothing: the writer stamps the time itself; there is no caller-side date formatter | not planned |
| `TalkerJsonFormatter` (`format`, `stripQuotes`, `.custom`) | nothing: messages are strings; there is no object-to-JSON step to format | not planned |

### Console output (`talker_logger`, the logger talker wires in)

| `talker_logger` | Ours | Status |
|---|---|---|
| `TalkerLogger.log(msg, { level, pen })` and the level shortcuts `critical`/`error`/`warning`/`debug`/`verbose`/`info` | nothing: console output is not this library; ours writes files | not planned |
| `TalkerLoggerSettings.level` | `LoggingOptions.level`: the least severity recorded, set at construction rather than on a logger | adapted — implemented, pending CI |
| `TalkerLoggerSettings.colors` / `enableColors` | nothing: plain files | not planned |
| `TalkerLoggerSettings.enable` | nothing: no runtime switch (see `enable()` / `disable()`) | not planned |
| `TalkerLoggerSettings.defaultTitle` | nothing: no titles | not planned |
| `TalkerLoggerSettings.lineSymbol` / `maxLineWidth` | nothing: no console borders | not planned |
| `TalkerLoggerSettings.copyWith()` | nothing: options are set once at construction; there is no settings object to copy | not planned |
| `TalkerLogger.copyWith()` | nothing: the logger is built once at `new`; there is no copy seam | not planned |
| `ConsoleUtils.getUnderline()` / `getTopline()` | nothing: no console borders (see `lineSymbol` / `maxLineWidth`) | not planned |
| `LoggerFormatter` (`ExtendedLoggerFormatter`, `ColoredLoggerFormatter`) | nothing: the line format is fixed by the writer | not planned |
| `LogDetails` (`message`, `level`, `pen`; the formatter payload) | nothing: no formatter to receive it; the `Backend` trait defines its own write shape | not planned |
| `LoggerFilter` | the severity gate in `LoggingOptions.level`, as with `TalkerFilter` | adapted — implemented, pending CI |
| `LogLevelFilter` (the default `LoggerFilter` talker wires in) | the same severity gate in `LoggingOptions.level`, as with `LoggerFilter` | adapted — implemented, pending CI |
| `logLevelPriorityList` (exported top-level) | nothing: the four levels order themselves; no priority list is exported | not planned |
| `LoggerOutput` | the `Backend` trait is the seam (`with_backend`, `FakeBackend`; TS: `memoryBackend`, `rustBackend`), but it takes structured writes, not formatted strings | adapted — implemented, pending CI |

## `react-native-logs`

### `createLogger(config)` and its config

| `react-native-logs` | Ours | Status |
|---|---|---|
| `createLogger(config)` | `Logging::new` over `LoggingOptions` (TS: `new Logging(options)`); a typed options struct replaces the config object, with no factory wrapper | adapted — implemented, pending CI |
| `severity` | `LoggingOptions.level`, chosen at construction; there is no later severity call (see `setSeverity`) | adapted — implemented, pending CI |
| `transport` / `transports` (one or an array) | the `Backend` trait is the sink (`with_backend` in Rust; `memoryBackend` or `rustBackend` in TS); one backend per logger, no per-call array | adapted — implemented, pending CI |
| `transportOptions` | `LoggingOptions` is the single options bag (`directory`, `file_base_name`, `rotation`, the caps, `redact`, `write_timeout`); there is no second per-transport object | adapted — implemented, pending CI |
| `levels` (custom level map) | nothing: the level set is fixed — `debug`, `info`, `warning`, `error` — and custom levels are not accepted | not planned |
| `async` | every call resolves exactly once — success, a coded failure, or a cancel — bounded by `write_timeout`; there is no idle-time deferral of writes | adapted — implemented, pending CI |
| `asyncFunc` | nothing: no deferral, so no hook for a custom one | not planned |
| `stringifyFunc` | nothing: messages are strings, and `redact` is the only transformation before writing | not planned |
| `formatFunc` | nothing: the line format is fixed by the writer | not planned |
| `dateFormat` (`time` / `local` / `utc` / `iso` / function) | nothing: the timestamp format is fixed by the writer | not planned |
| `printLevel` / `printDate` | nothing: the stored line format is not toggleable | not planned |
| `fixedExtLvlLength` | nothing: console column alignment; ours has no console output | not planned |
| `enabled` | nothing: no runtime switch; `LoggingOptions.level` at construction decides what is recorded | not planned |
| `enabledExtensions` | nothing: no extension or tag concept in the API | not planned |
| `printFileLine` / `fileLineOffset` (shipped in the defaults object, absent from the config type) | nothing: no caller file or line capture | not planned |

### The returned logger

| `react-native-logs` | Ours | Status |
|---|---|---|
| `debug`, `info`, `error` (default level methods) | `debug`, `info`, `error` (TS: same names) | same — implemented, pending CI |
| `warn` (default level method) | `warning`: the fixed API names the level `warning`, as talker does | adapted — implemented, pending CI |
| level methods from a custom `levels` map | nothing: the level set is fixed (see the `levels` row) | not planned |
| `extend(extension)` | nothing: one facade; no namespaced sub-loggers | not planned |
| `enable()` / `disable()` (logger or extension) | nothing: no runtime switch | not planned |
| `getExtensions()` | nothing: no extensions | not planned |
| `setSeverity(level)` / `getSeverity()` | nothing: the level lives in `LoggingOptions` and is fixed at construction | not planned |
| `patchConsole()` (the reserved key `getOriginalConsole` has no method in the reference either) | nothing: no global console patching; callers log through the facade | not planned |

### Transports

| `react-native-logs` | Ours | Status |
|---|---|---|
| `consoleTransport` (options `colors`, `extensionColors`, `consoleFunc`) | nothing: console output is the shell's job; ours writes files | not planned |
| `mapConsoleTransport` (option `mapLevels`) | nothing: no console to map levels onto | not planned |
| `fileAsyncTransport` | this is the crate's core: `directory`, `file_base_name` and `rotation` own the writing the reference delegates to an injected filesystem, and the size and age caps bound what it appends forever | adapted — implemented, pending CI |
| `fileAsyncTransport` option `FS` | nothing is injected: the crate writes its own files, and the TS side picks `memoryBackend` or `rustBackend` rather than passing an FS library | adapted — implemented, pending CI |
| `fileAsyncTransport` option `fileName` (with its `{date-today}` token) | `file_base_name` names the files; the date part comes from the `rotation` schedule (`never`, `minutely`, `hourly`, `daily`), not from a name template | adapted — implemented, pending CI |
| `fileAsyncTransport` option `fileNameDateType` (`eu` / `us` / `iso`) | rollover follows `rotation`; the file-name date format is the writer's, not an option | adapted — implemented, pending CI |
| `fileAsyncTransport` option `filePath` | `LoggingOptions.directory` | adapted — implemented, pending CI |
| `sentryTransport` | nothing: remote crash reporting is out of scope for a local-file library | not planned |
| `crashlyticsTransport` | nothing: same scope line | not planned |
| `transportFunctionType` (the transport contract) | the `Backend` trait in Rust, with `FakeBackend` for tests; `memoryBackend` and `rustBackend` in TS | adapted — implemented, pending CI |

Names with no reference row: `flush`, `with_backend`, `redact`, `write_timeout`, the three caps, `FakeBackend` and the `ERR_LOGGING_` codes are this library's own. `flush` drains the writer before exit; the references append fire-and-forget and offer nothing to wait on.
