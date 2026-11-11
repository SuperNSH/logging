//! Android: real files under the app's files directory, reached over JNI. NSH does not
//! start `ndk-context`, so the VM comes from `libnativehelper`, the application from
//! `ActivityThread`, and both go to `ndk-context` once — nsh-biometrics' route. A context
//! that is missing or not yet started answers NotAvailable.

use std::{
    error::Error as _,
    ffi::{c_char, c_int, c_void},
    fs,
    future::ready,
    io::{self, Write as _},
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use jni::{
    Env, JavaVM, jni_sig, jni_str,
    objects::{JObject, JString},
    sys::{jint, jsize},
};
use tracing_appender::{
    non_blocking::{ErrorCounter, NonBlocking, NonBlockingBuilder, WorkerGuard},
    rolling::{InitError, RollingFileAppender, Rotation as FileRotation},
};

use crate::{
    backend::{Backend, BackendFuture, FileStat},
    error::{ErrorKind, LoggingError},
    logging::{format_line, LogEntry},
    options::{LoggingOptions, Rotation},
};

/// The backend's own directory under the app's files directory, so the directory it lists
/// and clears holds only its files.
const DIRECTORY_NAME: &str = "nsh-logging";
/// How long a flush waits for the worker to drain; tracing-appender's own shutdown flush
/// waits one second, so this matches it.
const DRAIN_TIMEOUT: Duration = Duration::from_secs(1);
/// How often a waiting flush rechecks the worker's progress.
const DRAIN_POLL: Duration = Duration::from_millis(2);
/// `RTLD_NOW`: resolve `libnativehelper`'s symbols when it is opened.
const RTLD_NOW: c_int = 2;

/// Set once `ndk-context` holds the VM and the application. It can be set only once.
static CONTEXT: Mutex<bool> = Mutex::new(false);

/// The platform's file sink: one directory, one non-blocking writer, and the deletes and
/// the snapshot the core's caps ask for.
pub(crate) struct NativeBackend {
    directory: PathBuf,
    state: Mutex<SinkState>,
    signal: Arc<DrainSignal>,
}

/// The writer's state. `non_blocking` hands lines to tracing-appender's worker thread and
/// `guard` keeps that worker draining until the backend is dropped.
struct SinkState {
    non_blocking: NonBlocking,
    guard: WorkerGuard,
    /// The writer's drop counter, watched by [`SinkState::send`] so a full queue fails
    /// the write with a coded error instead of blocking or losing the line silently.
    dropped: ErrorCounter,
    appender: Arc<Mutex<RollingFileAppender>>,
    prefix: String,
    rotation: Rotation,
    bucket: u64,
    roll: u32,
    written: u64,
    max_file_size: Option<u64>,
    /// Set when a removal deleted the writer's own file, whose open handle would then
    /// write to a deleted inode; the next append swaps in a fresh appender.
    reopen_pending: bool,
}

impl SinkState {
    /// Sends `bytes` without ever blocking: the lossy writer drops the line when its
    /// bounded queue is full, and the drop counter turns that into a coded failure —
    /// never a silent loss, and never the unblockable wait of a full-queue send.
    fn send(&mut self, bytes: &[u8]) -> Result<(), LoggingError> {
        let before = self.dropped.dropped_lines();
        self.non_blocking.write_all(bytes).map_err(LoggingError::from_io)?;
        if self.dropped.dropped_lines() > before {
            return Err(LoggingError::new(ErrorKind::Backend));
        }
        Ok(())
    }
}

impl From<jni::errors::Error> for LoggingError {
    fn from(_: jni::errors::Error) -> Self {
        LoggingError::new(ErrorKind::Backend)
    }
}

impl NativeBackend {
    /// Opens the app's files directory, creating the log directory on demand, and starts
    /// the writer. A missing or not-yet-started Android context answers NotAvailable.
    pub(crate) fn new(options: &LoggingOptions) -> Result<Self, LoggingError> {
        let directory = resolve(&options.directory, files_dir()?);
        fs::create_dir_all(&directory).map_err(LoggingError::from_io)?;
        let signal = Arc::new(DrainSignal::default());
        let state = start(options, &directory, &signal)?;
        Ok(Self {
            directory,
            state: Mutex::new(state),
            signal,
        })
    }

    fn lock(&self) -> MutexGuard<'_, SinkState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Appends one line, rolling mid-interval when the size cap says so.
    fn append_line(&self, entry: &LogEntry) -> Result<(), LoggingError> {
        let mut state = self.lock();
        let mut line = format_line(entry);
        line.push('\n');
        // The appender rolls by its own UTC clock; the bucket only spots the interval
        // change so the roll counter and the byte count start the new file fresh.
        let bucket = interval_bucket(state.rotation);
        if bucket != state.bucket {
            state.bucket = bucket;
            state.roll = 0;
            state.written = 0;
        }
        // A line longer than the cap still lands, so rolling cannot loop on one line.
        if state
            .max_file_size
            .is_some_and(|cap| state.written > 0 && state.written + line.len() as u64 > cap)
        {
            state.roll = state.roll.saturating_add(1);
            reopen(&mut state, &self.directory)?;
        } else if state.reopen_pending {
            reopen(&mut state, &self.directory)?;
        }
        state.send(line.as_bytes())?;
        state.written += line.len() as u64;
        Ok(())
    }

    /// Enqueues a marker on the worker's queue and waits for the worker to pass it; the
    /// queue is FIFO, so every line before the marker is in the file by then. The wait is
    /// bounded: a stuck worker answers Timeout instead of hanging.
    fn drain(&self) -> Result<(), LoggingError> {
        let mut state = self.lock();
        let deadline = Instant::now() + DRAIN_TIMEOUT;
        let marker = self.signal.enqueue();
        state.send(&[])?;
        while !self.signal.observed(marker) {
            if Instant::now() >= deadline {
                return Err(LoggingError::new(ErrorKind::Timeout));
            }
            std::thread::sleep(DRAIN_POLL);
        }
        Ok(())
    }

    /// The directory's regular files in `read_dir`'s order; the core sorts them.
    fn stored_files(&self) -> Result<Vec<FileStat>, LoggingError> {
        let entries = fs::read_dir(&self.directory).map_err(LoggingError::from_io)?;
        let mut files = Vec::new();
        for entry in entries {
            let entry = entry.map_err(LoggingError::from_io)?;
            let metadata = entry.metadata().map_err(LoggingError::from_io)?;
            if !metadata.is_file() {
                continue;
            }
            files.push(FileStat {
                name: entry.file_name().to_string_lossy().into_owned(),
                size: metadata.len(),
                // A file whose times the platform will not tell reads as oldest.
                modified: metadata.modified().unwrap_or(UNIX_EPOCH),
            });
        }
        Ok(files)
    }

    /// Removes the named files. The writer's own file may be among them, so its removal
    /// is remembered: the next append reopens instead of writing on to a deleted file's
    /// inode.
    fn remove_files(&self, names: &[String]) -> Result<(), LoggingError> {
        if names.is_empty() {
            return Ok(());
        }
        let mut state = self.lock();
        let dotted = format!("{}.", state.prefix);
        for name in names {
            if name.as_str() == state.prefix || name.starts_with(&dotted) {
                state.reopen_pending = true;
            }
            match fs::remove_file(self.directory.join(name)) {
                Ok(()) => {}
                // Already gone satisfies the removal.
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(LoggingError::from_io(error)),
            }
        }
        Ok(())
    }
}

impl Backend for NativeBackend {
    fn append(&self, entry: &LogEntry) -> BackendFuture<'_, Result<(), LoggingError>> {
        let answer = self.append_line(entry);
        Box::pin(ready(answer))
    }

    fn flush(&self) -> BackendFuture<'_, Result<(), LoggingError>> {
        let answer = self.drain();
        Box::pin(ready(answer))
    }

    fn remove_all(&self, names: &[String]) -> BackendFuture<'_, Result<(), LoggingError>> {
        let answer = self.remove_files(names);
        Box::pin(ready(answer))
    }

    fn files(&self) -> BackendFuture<'_, Result<Vec<FileStat>, LoggingError>> {
        let answer = self.stored_files();
        Box::pin(ready(answer))
    }
}

/// The directory the backend writes to: the shared default maps to the app's own
/// `nsh-logging` directory under the files directory, an absolute choice is taken as
/// given, and a relative one is anchored there rather than at an undefined cwd.
fn resolve(directory: &str, files_dir: PathBuf) -> PathBuf {
    if directory == LoggingOptions::default().directory {
        return files_dir.join(DIRECTORY_NAME);
    }
    let chosen = Path::new(directory);
    if chosen.is_absolute() {
        chosen.to_path_buf()
    } else {
        files_dir.join(chosen)
    }
}

/// The app's files directory, through the installed context. A null VM or application —
/// the context missing or the runtime not yet started — answers NotAvailable.
fn files_dir() -> Result<PathBuf, LoggingError> {
    ensure_context()?;
    let context = ndk_context::android_context();
    if context.vm().is_null() || context.context().is_null() {
        return Err(LoggingError::new(ErrorKind::NotAvailable));
    }
    // SAFETY: `from_raw` wants a live, non-null VM, which the check above and the
    // process's own runtime guarantee.
    let vm = unsafe { JavaVM::from_raw(context.vm().cast()) };
    vm.attach_current_thread(|env| {
        // SAFETY: the context is the process's own global reference, which lives as long
        // as the process; a `JObject` never deletes on drop.
        let application = unsafe { JObject::from_raw(env, context.context().cast()) };
        let files = env
            .call_method(
                &application,
                jni_str!("getFilesDir"),
                jni_sig!("()Ljava/io/File;"),
                &[],
            )?
            .l()?;
        if files.is_null() {
            return Err(LoggingError::new(ErrorKind::NotAvailable));
        }
        let absolute = env
            .call_method(
                &files,
                jni_str!("getAbsolutePath"),
                jni_sig!("()Ljava/lang/String;"),
                &[],
            )?
            .l()?;
        let path = env.as_cast::<JString>(&absolute)?.try_to_string(env)?;
        Ok::<_, LoggingError>(PathBuf::from(path))
    })
}

/// Gives `ndk-context` the VM and the application, once, so any later caller — this crate
/// or another — reaches Java through one context. A context another runtime installed
/// already is adopted untouched.
fn ensure_context() -> Result<(), LoggingError> {
    let mut ready = CONTEXT.lock().unwrap_or_else(PoisonError::into_inner);
    if *ready {
        return Ok(());
    }
    // `ndk-context` has no try-get: its getter panics while nothing is installed and its
    // initializer asserts on a second call, so caught probes are the only way to tell an
    // existing context apart. Each caught panic prints one stderr line, which is the
    // price of leaving the process-global panic hook to the app.
    if catch_unwind(AssertUnwindSafe(ndk_context::android_context)).is_ok() {
        *ready = true;
        return Ok(());
    }
    let vm = find_vm()?;
    // SAFETY: the pointer came from the runtime's own `JNI_GetCreatedJavaVMs`, and the VM
    // lives as long as the process.
    let context = unsafe { JavaVM::from_raw(vm) }
        .attach_current_thread(|env| {
            let app = application(env)?;
            // The global reference lives as long as the process, which is what
            // `ndk-context` assumes.
            let global = env.new_global_ref(app)?;
            Ok::<_, LoggingError>(global.into_raw().cast::<c_void>())
        })?;
    // A concurrent installer can win the race with the probe above, and the initializer
    // then asserts; a caught install plus a re-probe adopts whatever landed instead of
    // panicking on this runtime path.
    let _ = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: the VM and the application are the process's own, and the probe showed
        // nothing is installed.
        unsafe { ndk_context::initialize_android_context(vm.cast(), context) };
    }));
    if catch_unwind(AssertUnwindSafe(ndk_context::android_context)).is_ok() {
        *ready = true;
        return Ok(());
    }
    Err(LoggingError::new(ErrorKind::NotAvailable))
}

/// The application, which is a context for `Context.getFilesDir`. Null before the runtime
/// has created it.
fn application<'local>(env: &mut Env<'local>) -> Result<JObject<'local>, LoggingError> {
    let application = env
        .call_static_method(
            jni_str!("android/app/ActivityThread"),
            jni_str!("currentApplication"),
            jni_sig!("()Landroid/app/Application;"),
            &[],
        )?
        .l()?;
    if application.is_null() {
        return Err(LoggingError::new(ErrorKind::NotAvailable));
    }
    Ok(application)
}

/// The VM of this process. An NSH app has no JVM of its own, so it is found through the
/// runtime's library; no VM here means this is not an app process, which is NotAvailable.
fn find_vm() -> Result<*mut jni::sys::JavaVM, LoggingError> {
    // SAFETY: the names are valid C strings, and the symbol is called with the signature
    // the JNI specification gives it.
    unsafe {
        let library = dlopen(c"libnativehelper.so".as_ptr(), RTLD_NOW);
        if library.is_null() {
            return Err(LoggingError::new(ErrorKind::NotAvailable));
        }
        let symbol = dlsym(library, c"JNI_GetCreatedJavaVMs".as_ptr());
        if symbol.is_null() {
            return Err(LoggingError::new(ErrorKind::NotAvailable));
        }
        let get_created_vms: GetCreatedVms =
            std::mem::transmute::<*mut c_void, GetCreatedVms>(symbol);
        let mut vm: *mut jni::sys::JavaVM = std::ptr::null_mut();
        let mut count: jsize = 0;
        if get_created_vms(&mut vm, 1, &mut count) != 0 || count < 1 || vm.is_null() {
            return Err(LoggingError::new(ErrorKind::NotAvailable));
        }
        Ok(vm)
    }
}

/// `JNI_GetCreatedJavaVMs`, found by name in `libnativehelper`.
type GetCreatedVms = unsafe extern "C" fn(*mut *mut jni::sys::JavaVM, jsize, *mut jsize) -> jint;

unsafe extern "C" {
    fn dlopen(filename: *const c_char, flags: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
}

/// Opens the writer over the directory's current interval file. `written` counts this
/// session's bytes, so a file carried over from an earlier run rolls on new bytes alone.
fn start(
    options: &LoggingOptions,
    directory: &Path,
    signal: &Arc<DrainSignal>,
) -> Result<SinkState, LoggingError> {
    let appender = Arc::new(Mutex::new(build(
        directory,
        &options.file_base_name,
        options.rotation,
        None,
    )?));
    // Lossy so a full queue never blocks: `send` checks the drop counter and answers a
    // coded error for a dropped line instead of wedging a poll no timeout can preempt.
    let (non_blocking, guard) = NonBlockingBuilder::default()
        .lossy(true)
        .finish(DrainWriter {
            appender: Arc::clone(&appender),
            signal: Arc::clone(signal),
        });
    Ok(SinkState {
        non_blocking,
        guard,
        dropped: non_blocking.error_counter(),
        appender,
        prefix: options.file_base_name.clone(),
        rotation: options.rotation,
        bucket: interval_bucket(options.rotation),
        roll: 0,
        written: 0,
        max_file_size: options.max_file_size_bytes,
        reopen_pending: false,
    })
}

/// Builds the rolling appender for the current interval. The suffix names a mid-interval
/// size-rolled file `prefix.<date>.N`, or `prefix.N` with no rotation.
fn build(
    directory: &Path,
    prefix: &str,
    rotation: Rotation,
    roll: Option<u32>,
) -> Result<RollingFileAppender, LoggingError> {
    let mut builder = RollingFileAppender::builder()
        .rotation(file_rotation(rotation))
        .filename_prefix(prefix);
    if let Some(roll) = roll {
        builder = builder.filename_suffix(roll.to_string());
    }
    builder.build(directory).map_err(init_error)
}

/// Swaps in a fresh appender for `state.roll`; the worker's next line opens the new file
/// and the old handle drops closed.
fn reopen(state: &mut SinkState, directory: &Path) -> Result<(), LoggingError> {
    let roll = (state.roll > 0).then_some(state.roll);
    let fresh = build(directory, &state.prefix, state.rotation, roll)?;
    *locked_appender(&state.appender) = fresh;
    state.written = 0;
    state.reopen_pending = false;
    Ok(())
}

fn file_rotation(rotation: Rotation) -> FileRotation {
    match rotation {
        Rotation::Never => FileRotation::NEVER,
        Rotation::Minutely => FileRotation::MINUTELY,
        Rotation::Hourly => FileRotation::HOURLY,
        Rotation::Daily => FileRotation::DAILY,
    }
}

/// The builder wraps the io error that says why the file could not open; the shared
/// mapping reads it back out.
fn init_error(error: InitError) -> LoggingError {
    match error
        .source()
        .and_then(|source| source.downcast_ref::<io::Error>())
    {
        Some(error) => LoggingError::from_io(error.clone()),
        None => LoggingError::new(ErrorKind::Backend),
    }
}

/// The interval's bucket number. The appender's own UTC clock names the files; this only
/// spots boundaries, so one number per interval is enough.
fn interval_bucket(rotation: Rotation) -> u64 {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |passed| passed.as_secs());
    match rotation {
        Rotation::Never => 0,
        Rotation::Daily => seconds / 86_400,
        Rotation::Hourly => seconds / 3_600,
        Rotation::Minutely => seconds / 60,
    }
}

/// The writer tracing-appender's worker owns. An empty buffer is this backend's flush
/// marker: the worker drains the queue in order, so reaching the marker means every line
/// before it is in the file.
struct DrainWriter {
    appender: Arc<Mutex<RollingFileAppender>>,
    signal: Arc<DrainSignal>,
}

impl io::Write for DrainWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            self.signal.mark();
            return Ok(0);
        }
        locked_appender(&self.appender)
            .write_all(buf)
            .map(|()| buf.len())
    }

    // std's write_all skips `write` for empty buffers, which would swallow the marker.
    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        if buf.is_empty() {
            self.signal.mark();
            return Ok(());
        }
        locked_appender(&self.appender).write_all(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        locked_appender(&self.appender).flush()
    }
}

/// Flush agreement between the backend and the worker: the markers enqueued and the
/// markers observed.
#[derive(Default)]
struct DrainSignal {
    markers: Mutex<(u64, u64)>,
}

impl DrainSignal {
    /// The marker that rides the queue next. Only the writer's holder enqueues, so
    /// markers cannot overtake each other.
    fn enqueue(&self) -> u64 {
        let mut markers = self.lock();
        markers.0 += 1;
        markers.0
    }

    /// The worker, on an empty write: everything enqueued before now is in the file.
    fn mark(&self) {
        let mut markers = self.lock();
        markers.1 = markers.0;
    }

    fn observed(&self, marker: u64) -> bool {
        self.lock().1 >= marker
    }

    fn lock(&self) -> MutexGuard<'_, (u64, u64)> {
        self.markers.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn locked_appender(
    appender: &Arc<Mutex<RollingFileAppender>>,
) -> MutexGuard<'_, RollingFileAppender> {
    appender.lock().unwrap_or_else(PoisonError::into_inner)
}
