/** The codes a logging rejection carries. Apps match on `code`. */
export type LoggingErrorCode =
  | "ERR_LOGGING_NOT_AVAILABLE"
  | "ERR_LOGGING_INVALID_ARGUMENT"
  | "ERR_LOGGING_CANCELLED"
  | "ERR_LOGGING_TIMEOUT"
  | "ERR_LOGGING_DISK_FULL"
  | "ERR_LOGGING_PERMISSION_DENIED"
  | "ERR_LOGGING_NOT_FOUND"
  | "ERR_LOGGING_BACKEND";

/** Same wording as the Rust crate's messages. None holds the reason text. */
const MESSAGES: Record<LoggingErrorCode, string> = {
  ERR_LOGGING_NOT_AVAILABLE: "File logs are not available on this target.",
  ERR_LOGGING_INVALID_ARGUMENT:
    "An option is empty or out of range, such as a zero timeout or an empty directory.",
  ERR_LOGGING_CANCELLED: "The write was cancelled before it reached the disk.",
  ERR_LOGGING_TIMEOUT: "The write timed out, so the line may not be on disk.",
  ERR_LOGGING_DISK_FULL: "The disk is full. Free space or lower `max_total_size_bytes`.",
  ERR_LOGGING_PERMISSION_DENIED: "The log directory is not writable by this app.",
  ERR_LOGGING_NOT_FOUND: "The log file is gone: it was rotated away or cleared.",
  ERR_LOGGING_BACKEND: "The platform's file service failed.",
};

const KNOWN = new Set<string>(Object.keys(MESSAGES));

/** A rejection of the logging API. `code` is the stable name; the message says what to do. */
export class LoggingError extends Error {
  readonly code: LoggingErrorCode;

  constructor(code: LoggingErrorCode) {
    super(MESSAGES[code]);
    this.name = "LoggingError";
    this.code = code;
  }
}

/** The error for a known code. */
export function loggingError(code: LoggingErrorCode): LoggingError {
  return new LoggingError(code);
}

/**
 * Turns whatever the native side or a host threw into a coded `LoggingError`. A host's
 * rejection carries `ERR_NSH_HOST_ERROR`, and its message starts with the code, as
 * `ERR_LOGGING_CANCELLED: text`. Anything unknown is a backend failure.
 */
export function asLoggingError(error: unknown): LoggingError {
  if (error instanceof LoggingError) return error;
  const raw = (error ?? {}) as { code?: unknown; message?: unknown };
  const code = typeof raw.code === "string" ? raw.code : "";
  const message = typeof raw.message === "string" ? raw.message : "";
  if (code === "ERR_NSH_HOST_ERROR") {
    const named = /^(ERR_LOGGING_[A-Z_]+): /.exec(message)?.[1];
    return named !== undefined && KNOWN.has(named)
      ? loggingError(named as LoggingErrorCode)
      : loggingError("ERR_LOGGING_BACKEND");
  }
  return KNOWN.has(code) ? loggingError(code as LoggingErrorCode) : loggingError("ERR_LOGGING_BACKEND");
}
