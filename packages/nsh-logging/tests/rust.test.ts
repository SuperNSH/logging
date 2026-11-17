import { expect, test } from "nsh/test";
import { Logging } from "../src";

/** Marks this run's lines, so leftovers from earlier runs cannot satisfy an assertion. */
const run = `${Date.now()}`;

test("the rust backend writes through the crate and reads back", async () => {
  // No backend: the logger runs on the Rust crate's writer, not a test sink.
  const logger = new Logging();

  await logger.info(`${run} through the crate`);
  await logger.error(`${run} burst`);
  await logger.flush();

  const entries = await logger.logs();
  expect(entries.map((entry) => entry.message)).toEqual([
    `${run} through the crate`,
    `${run} burst`,
  ]);
  expect(entries.map((entry) => entry.level)).toEqual(["info", "error"]);
  expect(entries[1].timeMs).toBeGreaterThanOrEqual(entries[0].timeMs);

  // A keyed secret is masked before the write and stays masked on the way back.
  await logger.info("password=hunter2");
  const entriesAfterLeak = await logger.logs();
  expect(entriesAfterLeak[entriesAfterLeak.length - 1].message).toBe("password=[REDACTED]");
  expect(entriesAfterLeak.map((entry) => entry.message).join("\n")).not.toContain("hunter2");
});

test("clean empties a rust-backed logger", async () => {
  const logger = new Logging();

  await logger.info(`${run} before the clean`);
  expect(await logger.logs()).toHaveLength(1);

  await logger.clean();

  expect(await logger.logs()).toEqual([]);

  // Clearing is not closing: a write after clean lands and is read back.
  await logger.info(`${run} after the clean`);
  expect((await logger.logs()).map((entry) => entry.message)).toEqual([`${run} after the clean`]);
});
