import { expect, test } from "nsh/test";
import { Logging, loggingError, type LoggingBackend } from "../src";

test("a target without a file sink rejects with the not available code", async () => {
  // The web backend's one answer, as the crate's unsupported sink gives it: every
  // storage call reports that file logs are not available here.
  const noSink: LoggingBackend = {
    append: () => Promise.reject(loggingError("ERR_LOGGING_NOT_AVAILABLE")),
    flush: () => Promise.reject(loggingError("ERR_LOGGING_NOT_AVAILABLE")),
    removeAll: () => Promise.reject(loggingError("ERR_LOGGING_NOT_AVAILABLE")),
    files: () => Promise.reject(loggingError("ERR_LOGGING_NOT_AVAILABLE")),
  };
  const logger = new Logging({ backend: noSink });

  await expect(logger.info("nowhere to write")).rejects.toMatchObject({
    code: "ERR_LOGGING_NOT_AVAILABLE",
  });
  await expect(logger.clean()).rejects.toMatchObject({ code: "ERR_LOGGING_NOT_AVAILABLE" });
  await expect(logger.flush()).rejects.toMatchObject({ code: "ERR_LOGGING_NOT_AVAILABLE" });

  // History needs no storage, so reading it back stays an answer, an empty one.
  expect(await logger.logs()).toEqual([]);
});
