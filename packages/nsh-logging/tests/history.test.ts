import { expect, test } from "nsh/test";
import { Logging, memoryBackend } from "../src";

test("logs returns the recent entries in order", async () => {
  const backend = memoryBackend();
  const logger = new Logging({ backend });

  await logger.debug("first");
  await logger.info("second");
  await logger.error("third");

  const entries = await logger.logs();
  expect(entries.map((entry) => entry.message)).toEqual(["first", "second", "third"]);
  expect(entries.map((entry) => entry.level)).toEqual(["debug", "info", "error"]);
  expect(entries[1].timeMs).toBeGreaterThanOrEqual(entries[0].timeMs);
  expect(entries[2].timeMs).toBeGreaterThanOrEqual(entries[1].timeMs);

  // The answer is a copy: tampering with it changes no stored entry.
  entries.pop();
  entries[0].message = "tampered";
  expect((await logger.logs()).map((entry) => entry.message)).toEqual(["first", "second", "third"]);

  // The history holds the last 1000 entries, so a flood sheds its oldest tail.
  const flood = 1010;
  await Promise.all(Array.from({ length: flood }, (_, index) => logger.debug(`m${index}`)));
  const recent = await logger.logs();
  expect(recent).toHaveLength(1000);
  expect(recent[0].message).toBe("m10");
  expect(recent[999].message).toBe("m1009");
});

test("clean empties the logs and the files", async () => {
  const backend = memoryBackend({
    files: [
      { name: "log.2026-11-08", size: 30, modifiedMs: 1 },
      { name: "log.2026-11-09", size: 30, modifiedMs: 2 },
    ],
  });
  const logger = new Logging({ backend });

  await logger.info("one");
  await logger.info("two");
  expect(await logger.logs()).toHaveLength(2);

  await logger.clean();

  expect(backend.removed()).toEqual(["log.2026-11-08", "log.2026-11-09"]);
  expect(backend.removes()).toBe(1);
  expect(await logger.logs()).toEqual([]);

  // Clearing is not closing: a write after clean lands and is read back.
  await logger.info("after the clean");
  expect((await logger.logs()).map((entry) => entry.message)).toEqual(["after the clean"]);
});
