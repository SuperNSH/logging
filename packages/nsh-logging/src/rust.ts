import { NativeLogging, type NativeLoggingOptions } from "nsh-logging/native";
import { type FileStat, type LogEntry, type LoggingBackend } from "./backend";
import { asLoggingError } from "./errors";
import type { LoggingOptions } from "./options";

/**
 * The Rust crate's writer (`nsh-logging/native`). The app or package must list the crate
 * under `"rust"` in `nsh.json`; importing this module without it fails the bundle.
 */
export function rustBackend(options: LoggingOptions = {}): LoggingBackend {
  return new RustBackend(options);
}

/** Only the fields the crate reads, camelCased as the native bridge spells them. */
function nativeOptions(options: LoggingOptions): NativeLoggingOptions {
  const result: NativeLoggingOptions = {};
  if (options.level !== undefined) result.level = options.level;
  if (options.directory !== undefined) result.directory = options.directory;
  if (options.fileBaseName !== undefined) result.fileBaseName = options.fileBaseName;
  if (options.rotation !== undefined) result.rotation = options.rotation;
  if (options.maxFileSizeBytes !== undefined) result.maxFileSizeBytes = options.maxFileSizeBytes;
  if (options.maxTotalSizeBytes !== undefined) result.maxTotalSizeBytes = options.maxTotalSizeBytes;
  if (options.maxAgeMs !== undefined) result.maxAgeMs = options.maxAgeMs;
  if (options.redact !== undefined) result.redact = options.redact;
  if (options.writeTimeoutMs !== undefined) result.writeTimeoutMs = options.writeTimeoutMs;
  return result;
}

class RustBackend implements LoggingBackend {
  private readonly native: NativeLogging;

  constructor(options: LoggingOptions) {
    this.native = new NativeLogging(nativeOptions(options));
  }

  /** The native side filters and redacts with the same options, so the core's pass agrees. */
  append(entry: LogEntry): Promise<void> {
    return this.settle(() => this.native.log(entry.message, entry.level));
  }

  flush(): Promise<void> {
    return this.settle(() => this.native.flush());
  }

  /** The files stay on the Rust side, so the core's list is everything; native cleans as a whole. */
  removeAll(): Promise<void> {
    return this.settle(() => this.native.clean());
  }

  /** Nothing to list: rotation and the caps run on the Rust side, where the files are. */
  files(): Promise<FileStat[]> {
    return Promise.resolve([]);
  }

  private async settle<T>(operation: () => Promise<T>): Promise<T> {
    try {
      return await operation();
    } catch (error: unknown) {
      throw asLoggingError(error);
    }
  }
}
