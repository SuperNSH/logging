import { type LogEntry, type LoggingBackend } from "./backend";
import { asLoggingError, loggingError, type LoggingError } from "./errors";
import type { Level, LoggingOptions } from "./options";
import { redact } from "./redact";
import { rustBackend } from "./rust";

/** How many written entries `logs` keeps in memory. The files are bounded by the caps, not this. */
const HISTORY_LIMIT = 1000;

/** The severity order: `debug` is the least severe, as the crate's `Level` derives it. */
const LEVELS: readonly Level[] = ["debug", "info", "warning", "error"];

/** The fields masked before any write, as `docs/goals.md` lists them. */
const DEFAULT_REDACT: readonly string[] = ["token", "password", "secret", "authorization", "email", "phone"];

/** The options of one logger, and the sink it writes through. */
export interface LoggingConfig extends LoggingOptions {
  /** Where the lines go: the Rust crate's writer, or a memory backend in tests. */
  backend?: LoggingBackend;
}

/**
 * Local log files behind one API, with the names `talker` and `react-native-logs` made
 * familiar: levels, exception handling, history read-back and clearing. It runs on the Rust
 * crate unless `backend` gives another sink. Options are fixed at construction, one write runs
 * at a time, and each call resolves exactly once.
 */
export class Logging {
  private readonly backend: LoggingBackend;
  private readonly level: Level;
  private readonly redactFields: string[];
  private readonly maxTotalSizeBytes?: number;
  private readonly maxAgeMs?: number;
  private readonly writeTimeoutMs?: number;
  private readonly history: LogEntry[] = [];
  private queue: Promise<unknown> = Promise.resolve();
  private readonly invalid: LoggingError | null;

  constructor(config: LoggingConfig = {}) {
    this.level = config.level ?? "debug";
    this.redactFields = config.redact !== undefined ? [...config.redact] : [...DEFAULT_REDACT];
    this.maxTotalSizeBytes = config.maxTotalSizeBytes;
    this.maxAgeMs = config.maxAgeMs;
    this.writeTimeoutMs = config.writeTimeoutMs;
    // The constructor cannot answer async; invalid options reject every call instead.
    this.invalid = invalidOptions(config);
    this.backend = config.backend ?? rustBackend(config);
  }

  /**
   * Writes one message at `level`. A level below the construction level is dropped and
   * resolves: the filter is not an error.
   */
  log(message: string, level: Level): Promise<void> {
    if (this.invalid !== null) return Promise.reject(this.invalid);
    // The type cannot express an unknown level; a plain-JS caller can, and the crate refuses it.
    if (!LEVELS.includes(level)) return Promise.reject(loggingError("ERR_LOGGING_INVALID_ARGUMENT"));
    if (LEVELS.indexOf(level) < LEVELS.indexOf(this.level)) return Promise.resolve();
    const entry: LogEntry = { timeMs: Date.now(), level, message: redact(message, this.redactFields) };
    return this.write(entry);
  }

  /** Writes at the debug level. */
  debug(message: string): Promise<void> {
    return this.log(message, "debug");
  }

  /** Writes at the info level. */
  info(message: string): Promise<void> {
    return this.log(message, "info");
  }

  /** Writes at the warning level. */
  warning(message: string): Promise<void> {
    return this.log(message, "warning");
  }

  /** Writes at the error level. */
  error(message: string): Promise<void> {
    return this.log(message, "error");
  }

  /** Records a failure and its stack, as talker's `handle` takes them, at the error level. */
  logException(error: string, stack?: string): Promise<void> {
    return this.log(stack === undefined ? error : `${error}\n${stack}`, "error");
  }

  /** The written entries, oldest first, up to the history bound. */
  logs(): Promise<LogEntry[]> {
    if (this.invalid !== null) return Promise.reject(this.invalid);
    return Promise.resolve(this.history.map((entry) => ({ ...entry })));
  }

  /** Empties the stored logs and deletes the files. */
  clean(): Promise<void> {
    if (this.invalid !== null) return Promise.reject(this.invalid);
    return this.enqueue(async () => {
      const names = (await this.backend.files()).map((file) => file.name);
      await this.backend.removeAll(names);
      this.history.length = 0;
    });
  }

  /** Drains the writer, so every appended line is on disk. Call it before exit. */
  flush(): Promise<void> {
    if (this.invalid !== null) return Promise.reject(this.invalid);
    return this.enqueue(() => this.backend.flush());
  }

  private write(entry: LogEntry): Promise<void> {
    return this.enqueue(async () => {
      await this.backend.append(entry);
      this.history.push(entry);
      while (this.history.length > HISTORY_LIMIT) this.history.shift();
      await this.enforceCaps();
    }, this.writeTimeoutMs);
  }

  /**
   * One operation at a time: the queue orders the append, the history and the cleanup, so
   * concurrent writers cannot interleave them. Under a `writeTimeoutMs` the deadline both
   * rejects the call and frees the queue, so a write that never answers cannot hang the ones
   * behind it.
   */
  private enqueue<T>(operation: () => Promise<T>, limitMs?: number): Promise<T> {
    const run = async (): Promise<T> => {
      if (limitMs === undefined) {
        try {
          return await operation();
        } catch (error: unknown) {
          throw asLoggingError(error);
        }
      }
      return new Promise<T>((resolve, reject) => {
        const timer = setTimeout(() => reject(loggingError("ERR_LOGGING_TIMEOUT")), limitMs);
        // The loser is left behind: a running promise cannot be cancelled, only overtaken.
        Promise.resolve(operation()).then(
          (value) => {
            clearTimeout(timer);
            resolve(value);
          },
          (error: unknown) => {
            clearTimeout(timer);
            reject(asLoggingError(error));
          },
        );
      });
    };
    const settled = this.queue.then(run, run);
    this.queue = settled.then(
      () => undefined,
      () => undefined,
    );
    return settled;
  }

  /** The core's policy: delete the oldest files first until the directory is under both caps. */
  private async enforceCaps(): Promise<void> {
    if (this.maxTotalSizeBytes === undefined && this.maxAgeMs === undefined) return;
    const files = await this.backend.files();
    // Oldest first, ties by name, so what goes and what stays is deterministic.
    files.sort((a, b) => a.modifiedMs - b.modifiedMs || compareNames(a.name, b.name));
    let total = files.reduce((sum, file) => sum + file.size, 0);
    const doomed: string[] = [];
    for (const file of files) {
      const over = this.maxTotalSizeBytes !== undefined && total > this.maxTotalSizeBytes;
      const expired = this.maxAgeMs !== undefined && Date.now() - file.modifiedMs > this.maxAgeMs;
      if (!over && !expired) break; // Sorted oldest first: no later file is over either cap.
      total -= file.size;
      doomed.push(file.name);
    }
    if (doomed.length > 0) await this.backend.removeAll(doomed);
  }
}

function compareNames(left: string, right: string): number {
  return left < right ? -1 : left > right ? 1 : 0;
}

/**
 * Refuses what `docs/goals.md` calls out: a zero cap, a zero timeout and an empty file base
 * name. The empty-directory refusal stays on the Rust side, which owns the filesystem and
 * fills in the default directory.
 */
function invalidOptions(options: LoggingOptions): LoggingError | null {
  const numbers = [options.maxFileSizeBytes, options.maxTotalSizeBytes, options.writeTimeoutMs];
  if (numbers.some((value) => value !== undefined && !(value > 0))) {
    return loggingError("ERR_LOGGING_INVALID_ARGUMENT");
  }
  if ((options.fileBaseName ?? "log").trim() === "") {
    return loggingError("ERR_LOGGING_INVALID_ARGUMENT");
  }
  return null;
}
