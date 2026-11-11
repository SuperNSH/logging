import { expect, test } from "nsh/test";
import {
  Logging,
  LoggingError,
  asLoggingError,
  loggingError,
  memoryBackend,
  type LoggingBackend,
  type LoggingErrorCode,
} from "../src";

/** Lets a rejected write free the queue before the next scripted failure arms it. */
async function delivered(): Promise<void> {
  for (let turn = 0; turn < 4; turn += 1) await Promise.resolve();
}

/** The eight codes and their messages, as docs/goals.md's table spells them. */
const ANSWERS: [LoggingErrorCode, string][] = [
  ["ERR_LOGGING_NOT_AVAILABLE", "File logs are not available on this target."],
  [
    "ERR_LOGGING_INVALID_ARGUMENT",
    "An option is empty or out of range, such as a zero timeout or an empty directory.",
  ],
  ["ERR_LOGGING_CANCELLED", "The write was cancelled before it reached the disk."],
  ["ERR_LOGGING_TIMEOUT", "The write timed out, so the line may not be on disk."],
  ["ERR_LOGGING_DISK_FULL", "The disk is full. Free space or lower `max_total_size_bytes`."],
  ["ERR_LOGGING_PERMISSION_DENIED", "The log directory is not writable by this app."],
  ["ERR_LOGGING_NOT_FOUND", "The log file is gone: it was rotated away or cleared."],
  ["ERR_LOGGING_BACKEND", "The platform's file service failed."],
];

test("every kind carries its code and its message", () => {
  expect(ANSWERS).toHaveLength(8);
  expect(new Set(ANSWERS.map(([code]) => code)).size).toBe(8);
  for (const [code, message] of ANSWERS) {
    expect(code.startsWith("ERR_LOGGING_")).toBe(true);
    const error = loggingError(code);
    expect(error.code).toBe(code);
    expect(error.message).toBe(message);
    expect(error.name).toBe("LoggingError");
    expect(error instanceof LoggingError).toBe(true);
    expect(error instanceof Error).toBe(true);
    expect(new LoggingError(code).message).toBe(message);
  }
});

test("filesystem failures map to their codes", async () => {
  const backend = memoryBackend();
  const logger = new Logging({ backend });

  // A scripted disk answers full, denied and missing; each write keeps its code and records nothing.
  for (const code of ["ERR_LOGGING_DISK_FULL", "ERR_LOGGING_PERMISSION_DENIED", "ERR_LOGGING_NOT_FOUND"] as const) {
    backend.failNextAppend(code);
    await expect(logger.error("burst")).rejects.toMatchObject({ code });
    await delivered();
    expect(await logger.logs()).toEqual([]);
  }

  expect(await logger.error("written")).toBeUndefined();
  expect(backend.lines()).toHaveLength(1);

  // Anything the disk said that fits no kind is a backend failure.
  const memory = memoryBackend();
  let rawFailure = true;
  const flaky: LoggingBackend = {
    ...memory,
    append: (entry) => {
      if (rawFailure) {
        rawFailure = false;
        return Promise.reject(new Error("device gone"));
      }
      return memory.append(entry);
    },
  };
  const flakyLogger = new Logging({ backend: flaky });
  await expect(flakyLogger.error("burst")).rejects.toMatchObject({ code: "ERR_LOGGING_BACKEND" });
  expect(await flakyLogger.error("written")).toBeUndefined();

  // A host's rejection names the code in its message; an unknown one is a backend failure.
  expect(
    asLoggingError({ code: "ERR_NSH_HOST_ERROR", message: "ERR_LOGGING_DISK_FULL: text" }).code,
  ).toBe("ERR_LOGGING_DISK_FULL");
  expect(asLoggingError({ code: "ERR_LOGGING_PERMISSION_DENIED", message: "x" }).code).toBe(
    "ERR_LOGGING_PERMISSION_DENIED",
  );
  expect(asLoggingError(new Error("boom")).code).toBe("ERR_LOGGING_BACKEND");
});
