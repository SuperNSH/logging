import { mount, signal, Text, View } from "nsh";
import { Logging, LoggingError, memoryBackend } from "./src";

/** The platform's log files, through the Rust crate's writer. */
const files = new Logging();
/** A logger into memory, so the surface also runs where no file sink exists. */
const memory = new Logging({ backend: memoryBackend() });

const message = signal("nothing yet");

function codeOf(error: unknown): string {
  return error instanceof LoggingError ? error.code : "unknown error";
}

function App() {
  const write = () => {
    Promise.all([memory.info("the demo was opened"), files.info("the demo was opened")]).then(
      () => {
        Promise.all([memory.logs(), files.logs()]).then(
          ([inMemory, onDisk]) => message.set(`memory ${inMemory.length} disk ${onDisk.length}`),
          (error: unknown) => message.set(codeOf(error)),
        );
      },
      (error: unknown) => message.set(codeOf(error)),
    );
  };
  return (
    <View class="flex-1 gap-3 p-4">
      <Text testID="title" class="text-foreground">nsh-logging</Text>
      <Text testID="message" class="text-foreground">{message()}</Text>
      <Text testID="write" role="button" onPress={write} class="rounded-(--radius) bg-muted px-3 py-2">
        Write
      </Text>
    </View>
  );
}

mount(App);
