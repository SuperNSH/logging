import { expect, test } from "nsh/test";
import { Logging, memoryBackend, type LoggingBackend } from "../src";

/** Lets a settled write free the queue, which hands the next call over a microtask later. */
async function delivered(): Promise<void> {
  for (let turn = 0; turn < 4; turn += 1) await Promise.resolve();
}

test("flush on exit resolves every buffered write", async () => {
  const backend = memoryBackend();
  const logger = new Logging({ backend });
  const writes = [logger.debug("first"), logger.info("second"), logger.warning("third")];
  // Flushing before the writes settle is the exit path: it queues behind them and
  // resolves only once every line has landed.
  const flushed = logger.flush();

  await Promise.all(writes);
  await flushed;

  expect(backend.lines()).toHaveLength(3);
  expect(backend.flushes()).toBe(1);
  expect(await logger.logs()).toHaveLength(3);
});

test("concurrent writes are serialized and each resolves once", async () => {
  const backend = memoryBackend();
  const logger = new Logging({ backend });
  const calls = Array.from({ length: 25 }, (_, index) => logger.info(`line ${index}`));

  const answers = await Promise.all(calls.map((call) => call.then(() => "written", () => "failed")));

  expect(answers.every((answer) => answer === "written")).toBe(true);
  const messages = backend.lines().map((line) => line.split(" ").slice(2).join(" "));
  expect(messages).toEqual(Array.from({ length: 25 }, (_, index) => `line ${index}`));
  expect(backend.appends()).toBe(25);
  expect(await logger.logs()).toHaveLength(25);
});

test("a write past its deadline rejects with the timeout code", async () => {
  const memory = memoryBackend();
  let wedged = true;
  const backend: LoggingBackend = {
    ...memory,
    // A write that never answers, as a stuck file service would be.
    append: (entry) => (wedged ? new Promise<void>(() => {}) : memory.append(entry)),
  };
  const logger = new Logging({ backend, writeTimeoutMs: 5 });

  await expect(logger.info("never lands")).rejects.toMatchObject({ code: "ERR_LOGGING_TIMEOUT" });
  await delivered();

  // The deadline frees the queue, so the write behind it is not hostage to the stuck one.
  wedged = false;
  expect(await logger.info("after the deadline")).toBeUndefined();
  expect(memory.lines()).toHaveLength(1);
  expect(memory.lines()[0]).toContain("after the deadline");
});

test("cancel resolves the pending write exactly once as cancelled", async () => {
  const backend = memoryBackend();
  const logger = new Logging({ backend });

  backend.failNextAppend("ERR_LOGGING_CANCELLED");
  await expect(logger.warning("dropped before the disk")).rejects.toMatchObject({
    code: "ERR_LOGGING_CANCELLED",
  });
  await delivered();

  // One answer, no retry, and nothing recorded for a write that never landed.
  expect(await logger.logs()).toEqual([]);
  expect(await logger.info("after the cancel")).toBeUndefined();
  expect(backend.appends()).toBe(1);
  expect(backend.lines()).toHaveLength(1);
});
