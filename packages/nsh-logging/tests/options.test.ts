import { expect, test } from "nsh/test";
import { Logging, memoryBackend, type LoggingOptions } from "../src";

test("invalid options are refused with the invalid argument code", async () => {
  const refused: LoggingOptions[] = [
    { maxFileSizeBytes: 0 },
    { maxTotalSizeBytes: 0 },
    { maxTotalSizeBytes: -100 },
    { writeTimeoutMs: 0 },
    { fileBaseName: "   " },
  ];

  for (const options of refused) {
    const backend = memoryBackend();
    const logger = new Logging({ backend, ...options });
    // The constructor cannot answer async; every call rejects instead, and nothing
    // reaches the backend.
    await expect(logger.log("no", "info")).rejects.toMatchObject({
      code: "ERR_LOGGING_INVALID_ARGUMENT",
    });
    await expect(logger.logs()).rejects.toMatchObject({ code: "ERR_LOGGING_INVALID_ARGUMENT" });
    await expect(logger.clean()).rejects.toMatchObject({ code: "ERR_LOGGING_INVALID_ARGUMENT" });
    await expect(logger.flush()).rejects.toMatchObject({ code: "ERR_LOGGING_INVALID_ARGUMENT" });
    expect(backend.appends()).toBe(0);
    expect(backend.flushes()).toBe(0);
  }
});

test("a higher construction level drops lower writes", async () => {
  const backend = memoryBackend();
  const logger = new Logging({ backend, level: "error" });

  // Dropped writes resolve: the filter is not an error, and the backend sees nothing.
  expect(await logger.debug("quiet")).toBeUndefined();
  expect(await logger.info("quiet")).toBeUndefined();
  expect(await logger.warning("quiet")).toBeUndefined();
  expect(backend.appends()).toBe(0);

  expect(await logger.error("loud")).toBeUndefined();
  expect(backend.lines()).toHaveLength(1);
  expect((await logger.logs()).map((entry) => entry.level)).toEqual(["error"]);
});
