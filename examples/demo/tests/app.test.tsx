// The demo's flows, with the steps a user takes: the four levels and the
// exception in the history, redaction, the warning-level filter, clean, the
// file logger, and the code an invalid option makes every write answer with.
import { expect, open, test } from "nsh/test";

test("every level lands in the history, the newest on top", async () => {
  const app = await open("../App.tsx", { width: 900, height: 1200 });
  await app.waitFor("id:empty");
  await app.press("id:write-debug");
  await app.press("id:write-info");
  await app.press("id:write-warning");
  await app.press("id:write-error");
  await app.waitUntil(() => app.texts().includes("error an error line"));
  const texts = app.texts();
  expect(texts).toContain("debug a debug line");
  expect(texts).toContain("info an info line");
  expect(texts).toContain("warning a warning line");
  expect(texts).toContain("error an error line");
  expect(texts.indexOf("error an error line")).toBeLessThan(texts.indexOf("warning a warning line"));
  expect(texts.indexOf("warning a warning line")).toBeLessThan(texts.indexOf("debug a debug line"));
});

test("redaction masks secret values before they reach the history", async () => {
  const app = await open("../App.tsx", { width: 900, height: 1200 });
  await app.waitFor("id:empty");
  await app.press("id:write-secret");
  await app.waitUntil(() => app.texts().some((text) => text.startsWith("info login failed")));
  const texts = app.texts();
  expect(texts).toContain("info login failed password=[REDACTED] token=[REDACTED]");
  expect(texts.some((text) => text.includes("hunter2"))).toBe(false);
  expect(texts.some((text) => text.includes("cafe1234"))).toBe(false);
});

test("logException records the thrown error at the error level", async () => {
  const app = await open("../App.tsx", { width: 900, height: 1200 });
  await app.waitFor("id:empty");
  await app.press("id:write-exception");
  await app.waitUntil(() => app.texts().some((text) => text.startsWith("error the demo threw on purpose")));
});

test("clean empties the history", async () => {
  const app = await open("../App.tsx", { width: 900, height: 1200 });
  await app.waitFor("id:empty");
  await app.press("id:write-info");
  await app.waitUntil(() => app.texts().includes("info an info line"));
  await app.waitFor("id:empty", { state: "gone" });
  await app.press("id:clean");
  await app.waitFor("id:empty");
  expect(app.texts().includes("info an info line")).toBe(false);
});

test("a logger built at warning drops a debug write and keeps a warning", async () => {
  const app = await open("../App.tsx", { width: 900, height: 1200 });
  await app.waitFor("id:filter-status");
  await app.press("id:write-filter-debug");
  await app.waitUntil(() => app.texts().includes("debug dropped by the warning filter"));
  await app.press("id:write-filter-warning");
  await app.waitUntil(() => app.texts().includes("kept: 1 entries"));
});

test("the file logger writes through the Rust crate, then flushes", async () => {
  const app = await open("../App.tsx", { width: 900, height: 1200 });
  await app.press("id:write-file");
  await app.waitUntil(() => app.texts().includes("the files hold 1 entry"));
  await app.press("id:flush-file");
  await app.waitUntil(() => app.texts().includes("the files are flushed"));
});

test("a logger built with a zero timeout answers every write with its code", async () => {
  const app = await open("../App.tsx", { width: 900, height: 1200 });
  await app.press("id:write-invalid");
  await app.waitUntil(() => app.texts().includes("ERR_LOGGING_INVALID_ARGUMENT"));
});
