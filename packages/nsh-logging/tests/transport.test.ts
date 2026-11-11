import { expect, test } from "nsh/test";
import { Logging, memoryBackend, type LogEntry, type LoggingBackend } from "../src";

test("each level names itself and the filter drops lower levels", async () => {
  const backend = memoryBackend();
  const logger = new Logging({ backend });

  await logger.debug("d");
  await logger.info("i");
  await logger.warning("w");
  await logger.error("e");
  expect(backend.lines().map((line) => line.split(" ")[0])).toEqual([
    "debug",
    "info",
    "warning",
    "error",
  ]);

  const strictBackend = memoryBackend();
  const strict = new Logging({ backend: strictBackend, level: "warning" });
  expect(await strict.debug("dropped")).toBeUndefined();
  expect(await strict.info("dropped")).toBeUndefined();
  expect(strictBackend.appends()).toBe(0);
  await strict.warning("kept");
  await strict.error("kept");
  expect(strictBackend.lines().map((line) => line.split(" ")[0])).toEqual(["warning", "error"]);

  // talker's `verbose` and `critical` fold into `debug` and `error`: they are not
  // level names here, and a plain-JS caller sending one is refused.
  const plain = strict as unknown as { log(message: string, level: string): Promise<void> };
  await expect(plain.log("x", "verbose")).rejects.toMatchObject({
    code: "ERR_LOGGING_INVALID_ARGUMENT",
  });
  await expect(plain.log("x", "critical")).rejects.toMatchObject({
    code: "ERR_LOGGING_INVALID_ARGUMENT",
  });
});

test("logException records the failure and its stack", async () => {
  const backend = memoryBackend();
  const logger = new Logging({ backend });

  await logger.logException("payment failed", "at checkout");
  await logger.logException("bare failure");

  const entries = await logger.logs();
  expect(entries.map((entry) => entry.level)).toEqual(["error", "error"]);
  expect(entries[0].message).toBe("payment failed\nat checkout");
  expect(entries[1].message).toBe("bare failure");
  expect(backend.lines()[0]).toContain("payment failed\nat checkout");
});

test("the written line carries the level, the timestamp and the message", async () => {
  const backend = memoryBackend();
  const logger = new Logging({ backend });

  await logger.info("ordered");

  const [entry] = await logger.logs();
  expect(backend.lines()).toEqual([`info ${entry.timeMs} ordered`]);
});

test("the backend receives structured writes", async () => {
  const received: LogEntry[] = [];
  const backend: LoggingBackend = {
    append: async (entry) => {
      received.push({ ...entry });
    },
    flush: async () => {},
    removeAll: async () => {},
    files: async () => [],
  };
  const logger = new Logging({ backend, redact: ["card"] });

  await logger.info("card: 4242424242424242");
  await logger.error("burst");

  expect(received).toHaveLength(2);
  expect(received[0].level).toBe("info");
  expect(received[0].message).toBe("card: [REDACTED]");
  expect(received[0].timeMs).toBeGreaterThan(0);
  expect(received[1].level).toBe("error");
  expect(received[1].message).toBe("burst");
});
