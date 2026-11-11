import { expect, test } from "nsh/test";
import { Logging, memoryBackend } from "../src";

test("tokens and personal fields are masked before anything is written", async () => {
  const backend = memoryBackend();
  const logger = new Logging({ backend });

  await logger.info("password: hunter2");
  await logger.error("the session token was eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9x9");
  await logger.warning("secret: shhh");
  await logger.info("authorization: sesame");
  await logger.debug("email: ada@example.com phone: 555-0100");

  const entries = await logger.logs();
  expect(entries.map((entry) => entry.message)).toEqual([
    "password: [REDACTED]",
    "the session token was [REDACTED]",
    "secret: [REDACTED]",
    "authorization: [REDACTED]",
    "email: [REDACTED] phone: [REDACTED]",
  ]);
  // The lines are the same redacted messages: masking happened before the write, not on read-back.
  expect(backend.lines()).toEqual(entries.map((entry) => `${entry.level} ${entry.timeMs} ${entry.message}`));
  expect(backend.lines().join("\n")).not.toContain("hunter2");
  expect(backend.lines().join("\n")).not.toContain("ada@example.com");

  // The field list is the option: with it emptied, the same message passes through.
  const open = new Logging({ backend: memoryBackend(), redact: [] });
  await open.info("password: hunter2");
  expect((await open.logs())[0].message).toBe("password: hunter2");
});
