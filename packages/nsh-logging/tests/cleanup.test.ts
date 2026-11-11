import { expect, test } from "nsh/test";
import { Logging, memoryBackend } from "../src";

test("max total size bytes deletes the oldest files first", async () => {
  const now = Date.now();
  const backend = memoryBackend({
    // Deliberately shuffled: the core sorts oldest first before it chooses.
    files: [
      { name: "log.2026-11-09-21", size: 40, modifiedMs: now - 2_000 },
      { name: "log.2026-11-08", size: 100, modifiedMs: now - 3_000 },
      { name: "log.2026-11-09-22", size: 10, modifiedMs: now - 1_000 },
    ],
  });
  const logger = new Logging({ backend, maxTotalSizeBytes: 120 });

  // 150 bytes stored, the cap is 120: the oldest 100 go, 50 stay under it.
  await logger.info("one more line");

  expect(backend.removed()).toEqual(["log.2026-11-08"]);
  expect(backend.removes()).toBe(1);
  expect(await logger.logs()).toHaveLength(1);
});

test("max age deletes files older than the limit", async () => {
  const now = Date.now();
  const backend = memoryBackend({
    files: [
      { name: "log.2026-11-09", size: 20, modifiedMs: now - 500 },
      { name: "log.2026-11-07", size: 20, modifiedMs: now - 5_000 },
    ],
  });
  const logger = new Logging({ backend, maxAgeMs: 1_500 });

  await logger.info("the fresh file stays");

  expect(backend.removed()).toEqual(["log.2026-11-07"]);
  expect(backend.removes()).toBe(1);
  expect(await logger.logs()).toHaveLength(1);
});
