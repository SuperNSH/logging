/** A severity, as the levels name themselves. `debug` is the least severe; talker's `verbose` folds into it. */
export type Level = "debug" | "info" | "warning" | "error";

/** How often the writer rolls to a new file. `weekly` stays tracing-appender's, not ours. */
export type Rotation = "never" | "minutely" | "hourly" | "daily";

/**
 * The options of one logger, fixed at construction: there is no reconfigure call, no runtime
 * severity switch and no on/off flag. Unset fields keep the crate's defaults.
 */
export interface LoggingOptions {
  /** The least severe level recorded. Writes below it are dropped, not errors. */
  level?: Level;
  /** The directory the log files live in. The Rust crate fills in a writable default. */
  directory?: string;
  /** The file names' prefix; the rotation schedule appends the date part. */
  fileBaseName?: string;
  /** How often the writer rolls to a new file. */
  rotation?: Rotation;
  /** The size in bytes at which a file rolls mid-interval. */
  maxFileSizeBytes?: number;
  /** The total bytes the directory may hold; the oldest files go first. */
  maxTotalSizeBytes?: number;
  /** The age in milliseconds at which a file is deleted. */
  maxAgeMs?: number;
  /** The fields whose values are masked before any write. */
  redact?: string[];
  /** The longest a write waits, in milliseconds, before it rejects with `ERR_LOGGING_TIMEOUT`. */
  writeTimeoutMs?: number;
}
