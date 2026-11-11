import { expect, test } from "nsh/test";
import {
  Logging,
  memoryBackend,
  type FileStat,
  type LogEntry,
  type LoggingBackend,
} from "../src";

/** The interval's file name, as tracing-appender's UTC clock writes it: `prefix.YYYY-MM-DD-HH-mm`. */
function minuteStamp(prefix: string, timeMs: number): string {
  const part = (value: number): string => String(value).padStart(2, "0");
  const utc = new Date(timeMs);
  return `${prefix}.${utc.getUTCFullYear()}-${part(utc.getUTCMonth() + 1)}-${part(
    utc.getUTCDate(),
  )}-${part(utc.getUTCHours())}-${part(utc.getUTCMinutes())}`;
}

/**
 * The file naming both platform families share: a line lands in the UTC interval's
 * `prefix.YYYY-MM-DD-HH-mm` file. The clock is injected, so tests cross boundaries.
 */
abstract class IntervalWriter implements LoggingBackend {
  protected readonly files = new Map<string, { size: number; timeMs: number; lines: string[] }>();
  protected base = "";
  protected written = 0;

  constructor(
    protected readonly prefix: string,
    protected readonly clock: () => number,
    protected readonly maxBytes?: number,
  ) {}

  append = async (entry: LogEntry): Promise<void> => {
    const base = minuteStamp(this.prefix, this.clock());
    if (base !== this.base) {
      this.base = base;
      this.on_interval();
    }
    const line = `${entry.level} ${entry.timeMs} ${entry.message}\n`;
    this.place(line);
    this.written += line.length;
  };

  /** What a new interval resets; only the predictive family resets anything. */
  protected on_interval(): void {}

  /** Adds one line to the file it belongs in, rolling first when the family's rule says so. */
  protected abstract place(line: string): void;

  /** The named file, or a fresh one under this clock, for the caller to fill and set back. */
  protected take(name: string): { size: number; timeMs: number; lines: string[] } {
    return this.files.get(name) ?? { size: 0, timeMs: this.clock(), lines: [] };
  }

  flush = async (): Promise<void> => {};

  removeAll = async (): Promise<void> => {};

  files = async (): Promise<FileStat[]> =>
    Array.from(this.files, ([name, file]) => ({
      name,
      size: file.size,
      modifiedMs: file.timeMs,
    }));

  /** The rolled files, in the order this writer created them. */
  names(): string[] {
    return Array.from(this.files.keys());
  }

  linesOf(name: string): string[] {
    return this.files.get(name)?.lines ?? [];
  }
}

/**
 * The apple/android rule: the write that would outgrow the cap rolls first, the live
 * file carries its `name.N` suffix, and a new interval resets the counters.
 */
class PredictiveRollWriter extends IntervalWriter {
  private roll = 0;

  protected on_interval(): void {
    this.roll = 0;
    this.written = 0;
  }

  protected place(line: string): void {
    // A line longer than the cap still lands, so rolling cannot loop on one line.
    if (this.maxBytes !== undefined && this.written > 0 && this.written + line.length > this.maxBytes) {
      this.roll += 1;
      this.written = 0;
    }
    const name = this.roll > 0 ? `${this.base}.${this.roll}` : this.base;
    const file = this.take(name);
    file.lines.push(line);
    file.size += line.length;
    this.files.set(name, file);
  }
}

/**
 * The linux/windows rule: once the interval file is at the cap, the full file is
 * renamed aside to the first free `name.N`; a time roll alone resets no counter.
 */
class RenameAsideRollWriter extends IntervalWriter {
  protected place(line: string): void {
    if (this.maxBytes !== undefined && this.written >= this.maxBytes && this.files.has(this.base)) {
      const aside = `${this.base}.${this.firstFreeIndex()}`;
      this.files.set(aside, this.take(this.base));
      this.files.delete(this.base);
      this.written = 0;
    }
    const file = this.take(this.base);
    file.lines.push(line);
    file.size += line.length;
    this.files.set(this.base, file);
  }

  private firstFreeIndex(): number {
    for (let index = 1; ; index += 1) {
      if (!this.files.has(`${this.base}.${index}`)) return index;
    }
  }
}

test("time rotation rolls to a new file at the interval", async () => {
  let now = Date.UTC(2026, 10, 9, 12, 30, 5);
  const writer = new PredictiveRollWriter("log", () => now);
  const logger = new Logging({ backend: writer, rotation: "minutely" });

  await logger.info("before the boundary");
  now += 60_000;
  await logger.info("after the boundary");

  expect(writer.names()).toEqual(["log.2026-11-09-12-30", "log.2026-11-09-12-31"]);
  expect(writer.linesOf("log.2026-11-09-12-30")).toHaveLength(1);
  expect(writer.linesOf("log.2026-11-09-12-30")[0]).toContain("before the boundary");
  expect(writer.linesOf("log.2026-11-09-12-31")).toHaveLength(1);
  expect(writer.linesOf("log.2026-11-09-12-31")[0]).toContain("after the boundary");
});

test("rotation alone bounds nothing until caps are set", async () => {
  const rolled = ["log.2026-11-09-12-2", "log.2026-11-09-12-3", "log.2026-11-09-12-4"].map(
    (name, index): FileStat => ({ name, size: 50, modifiedMs: index + 1 }),
  );
  const backend = memoryBackend({ files: rolled });
  const logger = new Logging({ backend, rotation: "minutely" });

  await logger.info("a fifth file changes nothing");

  expect(backend.removed()).toEqual([]);
  expect(backend.removes()).toBe(0);
  // With no caps the directory is not even read: nothing decides what goes.
  expect(backend.filesReads()).toBe(0);
  expect(await logger.logs()).toHaveLength(1);
});

test("size cap rolls a file mid interval", async () => {
  const fixed = Date.UTC(2026, 10, 9, 12, 30, 5);
  const base = minuteStamp("log", fixed);
  // Each line is 25-26 bytes ("info " + the 13-digit stamp + " " + message + "\n").
  // The stored lines keep their trailing newline, as the writer writes them; words() strips it.
  const words = (writer: IntervalWriter, name: string): string =>
    writer.linesOf(name).map((line) => line.replace(/\n$/, "").split(" ").slice(2).join(" ")).join(" | ");

  // apple/android: each write that would outgrow the cap starts a fresh `name.N`.
  const predictive = new PredictiveRollWriter("log", () => fixed, 45);
  const predictiveLogger = new Logging({ backend: predictive, rotation: "minutely", maxFileSizeBytes: 45 });
  await predictiveLogger.info("first");
  await predictiveLogger.info("second");
  await predictiveLogger.info("third");
  expect(predictive.names()).toEqual([base, `${base}.1`, `${base}.2`]);
  expect(words(predictive, base)).toBe("first");
  expect(words(predictive, `${base}.1`)).toBe("second");
  expect(words(predictive, `${base}.2`)).toBe("third");

  // linux/windows: the file reaches the cap whole, then is renamed aside; the new
  // line restarts the plain interval name.
  const aside = new RenameAsideRollWriter("log", () => fixed, 45);
  const asideLogger = new Logging({ backend: aside, rotation: "minutely", maxFileSizeBytes: 45 });
  await asideLogger.info("first");
  await asideLogger.info("second");
  await asideLogger.info("third");
  expect(aside.names().sort()).toEqual([base, `${base}.1`]);
  expect(aside.linesOf(`${base}.1`)).toHaveLength(2);
  expect(words(aside, `${base}.1`)).toBe("first | second");
  expect(aside.linesOf(base)).toHaveLength(1);
  expect(words(aside, base)).toBe("third");
});
