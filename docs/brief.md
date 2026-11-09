# Logging: brief

Local log files with rotation and bounded disk use.

## Options

- tracing-appender: Rust non-blocking writer, time-based rotation
- Size-based cleanup must be added on top
- Redact tokens and personal fields before writing

## Recommended path

tracing-appender for writing. Add size and age cleanup, and redaction.

## Edge cases to design for

- Time-based rotation has no size cap
- Unbounded growth fills the disk
- Non-blocking writers must flush on exit

## Research status

Research done 2026-11-09 by web search. Versions and licences are unverified until checked on the registry before pinning. Full catalogue: the research notes for the SuperNSH library set.
