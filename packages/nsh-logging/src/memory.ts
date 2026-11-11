import { type FileStat, type LogEntry, type LoggingBackend } from "./backend";
import { loggingError, type LoggingErrorCode } from "./errors";

/** The first directory snapshot a memory backend answers. */
export interface MemoryLoggingConfig {
  /** What `files` answers until `setFiles` replaces it. */
  files?: FileStat[];
}

/** A backend the test drives: it records lines, scripts failures and counts what was asked. */
export interface MemoryLogging extends LoggingBackend {
  /** Makes the next `append` fail with `code`, once. */
  failNextAppend(code: LoggingErrorCode): void;
  /** Makes the next `flush` fail with `code`, once. */
  failNextFlush(code: LoggingErrorCode): void;
  /** Makes the next `removeAll` fail with `code`, once. */
  failNextRemove(code: LoggingErrorCode): void;
  /** Makes the next `files` fail with `code`, once. */
  failNextFiles(code: LoggingErrorCode): void;
  /** Sets what `files` answers: the directory snapshot the caps read. */
  setFiles(files: FileStat[]): void;
  /** The lines the backend received, in the crate's fixed format, oldest first. */
  lines(): string[];
  /** The names the core asked to remove, in the order it asked for them. */
  removed(): string[];
  appends(): number;
  flushes(): number;
  removes(): number;
  filesReads(): number;
}

/** The writer's fixed line format — `level timestamp message` — is not an option. */
function formatLine(entry: LogEntry): string {
  return `${entry.level} ${entry.timeMs} ${entry.message}`;
}

/** A memory backend, for tests and for apps that log without a file sink. */
export function memoryBackend(config: MemoryLoggingConfig = {}): MemoryLogging {
  let files = (config.files ?? []).map((file) => ({ ...file }));
  let lines: string[] = [];
  let removed: string[] = [];
  let nextAppendError: LoggingErrorCode | null = null;
  let nextFlushError: LoggingErrorCode | null = null;
  let nextRemoveError: LoggingErrorCode | null = null;
  let nextFilesError: LoggingErrorCode | null = null;
  let appendCount = 0;
  let flushCount = 0;
  let removeCount = 0;
  let filesCount = 0;

  return {
    append: async (entry: LogEntry) => {
      if (nextAppendError !== null) {
        const code = nextAppendError;
        nextAppendError = null;
        throw loggingError(code);
      }
      lines.push(formatLine(entry));
      appendCount += 1;
    },
    flush: async () => {
      // The crate's fake counts every flush, failed or not; the other calls count only successes.
      const code = nextFlushError;
      nextFlushError = null;
      flushCount += 1;
      if (code !== null) throw loggingError(code);
    },
    removeAll: async (names: string[]) => {
      if (nextRemoveError !== null) {
        const code = nextRemoveError;
        nextRemoveError = null;
        throw loggingError(code);
      }
      removed.push(...names);
      removeCount += 1;
    },
    files: async () => {
      if (nextFilesError !== null) {
        const code = nextFilesError;
        nextFilesError = null;
        throw loggingError(code);
      }
      filesCount += 1;
      return files.map((file) => ({ ...file }));
    },
    failNextAppend: (code) => {
      nextAppendError = code;
    },
    failNextFlush: (code) => {
      nextFlushError = code;
    },
    failNextRemove: (code) => {
      nextRemoveError = code;
    },
    failNextFiles: (code) => {
      nextFilesError = code;
    },
    setFiles: (next) => {
      files = next.map((file) => ({ ...file }));
    },
    lines: () => [...lines],
    removed: () => [...removed],
    appends: () => appendCount,
    flushes: () => flushCount,
    removes: () => removeCount,
    filesReads: () => filesCount,
  };
}
