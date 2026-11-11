import type { Level } from "./options";

/** One written entry: what a backend receives and what `logs` reads back. */
export interface LogEntry {
  /** When the entry was written, in milliseconds since the epoch. */
  timeMs: number;
  /** The entry's level. */
  level: Level;
  /** The message, already redacted. */
  message: string;
}

/** One file of the directory snapshot, as the cleanup policy sees it. */
export interface FileStat {
  /** The file's base name. */
  name: string;
  /** Its size in bytes. */
  size: number;
  /** When it was last modified, in milliseconds since the epoch. */
  modifiedMs: number;
}

/**
 * What a platform provides. `Logging` adds the rules: the severity filter, the redaction, the
 * write timeout and the size and age caps.
 */
export interface LoggingBackend {
  /** Appends one line for `entry`, in the crate's fixed line format. */
  append(entry: LogEntry): Promise<void>;
  /** Drains the writer, so every appended line is on disk. */
  flush(): Promise<void>;
  /** Removes every named file. An empty list removes nothing. */
  removeAll(names: string[]): Promise<void>;
  /** The stored files, in any order; the core sorts them oldest first. */
  files(): Promise<FileStat[]>;
}
