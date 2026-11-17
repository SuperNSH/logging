import { mount, signal, Text, View } from "nsh";
import { type Level, type LogEntry, Logging, LoggingError, memoryBackend } from "../../packages/nsh-logging/src";

/** A logger into memory, so the surface also runs where no file sink exists. */
const memory = new Logging({ backend: memoryBackend() });
/** Built with level warning: a write below it is dropped, not an error. */
const filtered = new Logging({ backend: memoryBackend(), level: "warning" });
/** The platform's log files, through the Rust crate's writer. */
const files = new Logging();
/** Built with a zero timeout: the constructor cannot answer async, so every call rejects. */
const invalid = new Logging({ backend: memoryBackend(), writeTimeoutMs: 0 });

/** The memory logger's history, newest on top. */
const entries = signal<LogEntry[]>([]);
/** What the last write through the warning-level logger did. */
const filterStatus = signal("nothing written yet");
/** The file logger's last answer: a count, or the error's code. */
const fileStatus = signal("nothing written yet");
/** What the zero-timeout logger answered: always a code, never a write. */
const invalidStatus = signal("nothing written yet");

function codeOf(error: unknown): string {
  return error instanceof LoggingError ? error.code : "unknown error";
}

/** Reads the history back and puts the newest entry on top. */
async function refresh(): Promise<void> {
  entries.set((await memory.logs()).reverse());
}

/** Writes one line at `level`, then reads the history back. */
function write(level: Level, message: string): void {
  memory.log(message, level).then(refresh);
}

/** Throws on purpose, and records the failure and its stack as logException takes them. */
function writeException(): void {
  try {
    throw new Error("the demo threw on purpose");
  } catch (error) {
    const thrown = error instanceof Error ? error : new Error(String(error));
    memory.logException(thrown.message, thrown.stack).then(refresh);
  }
}

/** Writes through the warning-level logger, and says whether the filter kept the line. */
async function writeFiltered(level: "debug" | "warning"): Promise<void> {
  const before = (await filtered.logs()).length;
  await filtered.log(`the ${level} line`, level);
  const after = (await filtered.logs()).length;
  filterStatus.set(after > before ? `kept: ${after} entries` : "debug dropped by the warning filter");
}

/** Writes through the Rust crate's files, and reports the count or the error's code. */
async function writeFile(): Promise<void> {
  try {
    await files.info("written through the Rust crate");
    const onDisk = await files.logs();
    fileStatus.set(`the files hold ${onDisk.length} ${onDisk.length === 1 ? "entry" : "entries"}`);
  } catch (error) {
    fileStatus.set(codeOf(error));
  }
}

/** Drains the file writer, and reports it or the error's code. */
async function flushFiles(): Promise<void> {
  try {
    await files.flush();
    fileStatus.set("the files are flushed");
  } catch (error) {
    fileStatus.set(codeOf(error));
  }
}

/** Writes through the zero-timeout logger, whose every call answers with a code. */
function writeInvalid(): void {
  invalid.info("never written").then(
    () => invalidStatus.set("written, which the options forbid"),
    (error: unknown) => invalidStatus.set(codeOf(error)),
  );
}

function App() {
  return (
    <View class="flex-1 gap-4 p-4">
      <Text testID="title" class="text-foreground">nsh-logging</Text>

      <View class="gap-2">
        <Text class="text-foreground">A logger in memory: levels, an exception, redaction, read-back and clean.</Text>
        <View class="flex-row gap-2">
          <Text testID="write-debug" role="button" onPress={() => write("debug", "a debug line")} class="rounded-(--radius) bg-muted px-3 py-2 text-foreground">
            debug
          </Text>
          <Text testID="write-info" role="button" onPress={() => write("info", "an info line")} class="rounded-(--radius) bg-muted px-3 py-2 text-foreground">
            info
          </Text>
          <Text testID="write-warning" role="button" onPress={() => write("warning", "a warning line")} class="rounded-(--radius) bg-muted px-3 py-2 text-foreground">
            warning
          </Text>
          <Text testID="write-error" role="button" onPress={() => write("error", "an error line")} class="rounded-(--radius) bg-muted px-3 py-2 text-foreground">
            error
          </Text>
        </View>
        <View class="flex-row gap-2">
          <Text testID="write-secret" role="button" onPress={() => write("info", "login failed password=hunter2 token=cafe1234")} class="rounded-(--radius) bg-muted px-3 py-2 text-foreground">
            write a secret
          </Text>
          <Text testID="write-exception" role="button" onPress={writeException} class="rounded-(--radius) bg-muted px-3 py-2 text-foreground">
            throw and log
          </Text>
          <Text testID="clean" role="button" onPress={() => memory.clean().then(refresh)} class="rounded-(--radius) bg-muted px-3 py-2 text-foreground">
            clean
          </Text>
        </View>
        <View testID="history" class="gap-1">
          {entries().length === 0 ? (
            <Text testID="empty" class="text-muted-foreground">No entries yet</Text>
          ) : (
            <Each of={entries()} key={(entry) => `${entry.timeMs} ${entry.message}`}>
              {(entry, index) => (
                <Text testID={`entry-${index()}`} class="text-foreground">{`${entry().level} ${entry().message}`}</Text>
              )}
            </Each>
          )}
        </View>
      </View>

      <View class="gap-2">
        <Text class="text-foreground">A logger built with level warning: a write below it is dropped, not an error.</Text>
        <View class="flex-row gap-2">
          <Text testID="write-filter-debug" role="button" onPress={() => writeFiltered("debug")} class="rounded-(--radius) bg-muted px-3 py-2 text-foreground">
            debug through it
          </Text>
          <Text testID="write-filter-warning" role="button" onPress={() => writeFiltered("warning")} class="rounded-(--radius) bg-muted px-3 py-2 text-foreground">
            warning through it
          </Text>
        </View>
        <Text testID="filter-status" class="text-muted-foreground">{filterStatus()}</Text>
      </View>

      <View class="gap-2">
        <Text class="text-foreground">A logger on the Rust crate's files; where no file sink exists it answers with a code.</Text>
        <View class="flex-row gap-2">
          <Text testID="write-file" role="button" onPress={writeFile} class="rounded-(--radius) bg-muted px-3 py-2 text-foreground">
            write to the files
          </Text>
          <Text testID="flush-file" role="button" onPress={flushFiles} class="rounded-(--radius) bg-muted px-3 py-2 text-foreground">
            flush
          </Text>
        </View>
        <Text testID="file-status" class="text-muted-foreground">{fileStatus()}</Text>
      </View>

      <View class="gap-2">
        <Text class="text-foreground">A logger built with a zero timeout: its options are invalid, so every write answers with a code.</Text>
        <View class="flex-row gap-2">
          <Text testID="write-invalid" role="button" onPress={writeInvalid} class="rounded-(--radius) bg-muted px-3 py-2 text-foreground">
            write through it
          </Text>
        </View>
        <Text testID="invalid-status" class="text-muted-foreground">{invalidStatus()}</Text>
      </View>
    </View>
  );
}

mount(App);
