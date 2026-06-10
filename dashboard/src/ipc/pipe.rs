use crate::ipc::{parse_line, spawn_control_writer, ControlMsg, Line, SensorState, SharedState};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

const PIPE_PATH: &str = r"\\.\pipe\PerfWindowSensor";

#[derive(Debug)]
pub enum ConnectError {
    /// The pipe does not exist: service not installed or not running.
    NotFound,
    /// Another dashboard is already connected.
    Busy,
    /// The pipe's ACL refused access.
    AccessDenied,
    /// Any other I/O error.
    Io(std::io::Error),
}

impl std::fmt::Display for ConnectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectError::NotFound => write!(f, "sensor service is not running"),
            ConnectError::Busy => write!(f, "another PerfWindow window is already connected"),
            ConnectError::AccessDenied => write!(f, "access to sensor pipe was denied"),
            ConnectError::Io(e) => write!(f, "pipe I/O error: {e}"),
        }
    }
}

impl From<std::io::Error> for ConnectError {
    fn from(e: std::io::Error) -> Self {
        use std::io::ErrorKind::*;
        match e.kind() {
            NotFound => ConnectError::NotFound,
            PermissionDenied => ConnectError::AccessDenied,
            _ => match e.raw_os_error() {
                Some(231) /* ERROR_PIPE_BUSY */ => ConnectError::Busy,
                _ => ConnectError::Io(e),
            },
        }
    }
}

pub struct PipeSensord {
    control: Option<std::sync::mpsc::Sender<ControlMsg>>,
    writer: Option<JoinHandle<()>>,
    reader: Option<JoinHandle<()>>,
    pub state: SharedState,
}

impl PipeSensord {
    /// Open the pipe at the default location without trying to elevate.
    /// The connect state machine uses this in every phase that polls; it is
    /// the only production entry point (the elevation step lives in
    /// `connect.rs`, not here).
    #[allow(dead_code)]
    pub fn connect_no_elevation(repaint: impl Fn() + Send + 'static) -> Result<Self, ConnectError> {
        let read = Self::open_pipe_at(PIPE_PATH)?;
        Self::start_reader(read, repaint)
    }

    /// Open the pipe at an explicit path (used by integration tests that
    /// run sensord with `--pipe-name <Custom>`).
    #[allow(dead_code)]
    pub fn connect_to(
        path: &str,
        repaint: impl Fn() + Send + 'static,
    ) -> Result<Self, ConnectError> {
        let read = Self::open_pipe_at(path)?;
        Self::start_reader(read, repaint)
    }

    fn open_pipe_at(path: &str) -> Result<File, ConnectError> {
        // Open the client end of the named pipe in the default *synchronous*
        // (blocking) mode — NO `FILE_FLAG_OVERLAPPED`.
        //
        // The reader thread blocks in `BufReader::lines()` and the writer uses
        // blocking `writeln!`; both are synchronous `ReadFile`/`WriteFile`
        // calls. On a handle opened with `FILE_FLAG_OVERLAPPED`, a synchronous
        // `ReadFile` can return `ERROR_IO_PENDING` (the completion is delivered
        // via the OVERLAPPED structure, which `std::fs::File` never waits on).
        // The Rust standard library treats that as an unrecoverable condition
        // and calls `abort()` — "fatal runtime error: I/O error: operation
        // failed to complete synchronously". That abort surfaces as a Win32
        // `__fastfail` (0xc0000409 STATUS_STACK_BUFFER_OVERRUN), bypassing both
        // the Rust panic hook and `SetUnhandledExceptionFilter`, so it left no
        // breadcrumb in `panic.log` — the 0.9.1–0.9.4 "startup crash". Reads
        // on a blocking handle simply wait for data, which is exactly what the
        // reader thread wants, so the flag was never needed.
        OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .map_err(ConnectError::from)
    }

    fn start_reader(read: File, repaint: impl Fn() + Send + 'static) -> Result<Self, ConnectError> {
        // `DuplicateHandle` can transiently fail (kernel handle exhaustion,
        // a flaky ACL change between open and clone, the peer side dropping
        // the pipe mid-handshake). Propagate as `ConnectError::Io` so the
        // connect state machine surfaces it on the loading screen instead of
        // panicking the worker thread and freezing the UI on "Loading
        // sensors..." until the rx side hangs up.
        let write = read.try_clone().map_err(ConnectError::from)?;
        let state: SharedState = Arc::new(Mutex::new(SensorState {
            alive: true,
            ..SensorState::default()
        }));
        let reader_state = Arc::clone(&state);
        let reader = std::thread::spawn(move || {
            let lines = BufReader::new(read).lines();
            for line in lines {
                let Ok(line) = line else { break };
                let Some(parsed) = parse_line(&line) else {
                    continue;
                };
                if let Ok(mut s) = reader_state.lock() {
                    s.last_line_at = Some(std::time::Instant::now());
                    match parsed {
                        Line::Snap(snap) => s.latest = Some(*snap),
                        Line::Progress(p) => s.progress = Some(p),
                    }
                }
                repaint();
            }
            if let Ok(mut s) = reader_state.lock() {
                s.alive = false;
            }
            repaint();
        });
        let (tx, rx) = std::sync::mpsc::channel();
        let writer = spawn_control_writer(write, rx, Arc::clone(&state));
        Ok(PipeSensord {
            control: Some(tx),
            writer: Some(writer),
            reader: Some(reader),
            state,
        })
    }

    /// Queue a refresh-interval change for the writer thread. Non-blocking:
    /// the UI thread must never wait on pipe I/O — a sensord that stops
    /// draining the control channel used to freeze every settings click.
    pub fn set_interval(&mut self, ms: u32) {
        if let Some(tx) = &self.control {
            if tx.send(ControlMsg::SetInterval(ms)).is_err() {
                // The writer thread already exited (it flips `alive` on a
                // write failure); drop the dead channel so later calls no-op.
                self.control = None;
            }
        }
    }

    pub fn is_alive(&self) -> bool {
        self.state.lock().map(|s| s.alive).unwrap_or(false)
    }

    /// Tell the worker to exit cleanly: queue `{"shutdown":true}` and close
    /// the control channel. The writer thread performs the blocking write off
    /// the UI thread and then drops its pipe handle. Idempotent — calling
    /// more than once is a no-op — and never joins or blocks.
    ///
    /// This is the canonical close path. The `Drop` impl below also calls
    /// `shutdown` as a safety net, but the dashboard prefers to invoke this
    /// explicitly from `eframe::App::on_exit` while the frame is still alive
    /// (more reliable than relying on `Drop` during process tear-down).
    pub fn shutdown(&mut self) {
        if let Some(tx) = self.control.take() {
            let _ = tx.send(ControlMsg::Shutdown);
        }
        // Detach both threads. Joining the reader would deadlock: it blocks
        // in `BufReader::lines` until the service side closes the pipe, which
        // only happens after sensord acts on the shutdown message above. The
        // writer may equally be stuck mid-write on a full pipe.
        let _ = self.reader.take();
        let _ = self.writer.take();
    }
}

#[cfg(any(test, feature = "test-support"))]
impl PipeSensord {
    /// Test-only client with no I/O attached; tests drive it by mutating
    /// `state` directly.
    pub fn detached(state: SharedState) -> Self {
        PipeSensord {
            control: None,
            writer: None,
            reader: None,
            state,
        }
    }
}

impl Drop for PipeSensord {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Show the OS UAC prompt for elevating just `sc.exe start PerfWindowSensor`.
/// Returns `Ok(())` once the OS launches the process (UAC accepted); the
/// success of the *service start* itself is observed by polling the pipe.
pub fn elevate_and_start_service() -> Result<(), String> {
    let sc = std::env::var_os("SystemRoot")
        .map(std::path::PathBuf::from)
        .map(|root| root.join("System32").join("sc.exe"))
        .filter(|path| path.is_file())
        .and_then(|path| path.into_os_string().into_string().ok())
        .unwrap_or_else(|| "sc.exe".to_string());
    crate::ui::shell::shell_exec_runas(&sc, "start PerfWindowSensor")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::SensorState;
    use std::io::{Error, ErrorKind};

    #[test]
    fn not_found_io_error_maps_to_connect_not_found() {
        let err = ConnectError::from(Error::from(ErrorKind::NotFound));
        assert!(matches!(err, ConnectError::NotFound));
    }

    #[test]
    fn permission_denied_maps_to_access_denied() {
        let err = ConnectError::from(Error::from(ErrorKind::PermissionDenied));
        assert!(matches!(err, ConnectError::AccessDenied));
    }

    #[test]
    fn pipe_busy_os_error_231_maps_to_busy() {
        let err = ConnectError::from(Error::from_raw_os_error(231));
        assert!(matches!(err, ConnectError::Busy));
    }

    #[test]
    fn shutdown_twice_is_safe() {
        let mut s = PipeSensord::detached(Arc::new(Mutex::new(SensorState::default())));
        s.shutdown();
        s.shutdown(); // must not panic, must not deadlock
    }

    #[test]
    fn set_interval_returns_promptly_when_pipe_is_never_drained() {
        // Stands in for a pipe whose peer never reads: every write parks
        // until the test releases it.
        struct BlockedSink(std::sync::mpsc::Receiver<()>);
        impl std::io::Write for BlockedSink {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                let _ = self.0.recv();
                Ok(buf.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
        let state: SharedState = Arc::new(Mutex::new(SensorState {
            alive: true,
            ..SensorState::default()
        }));
        let (tx, rx) = std::sync::mpsc::channel();
        let writer =
            crate::ipc::spawn_control_writer(BlockedSink(release_rx), rx, Arc::clone(&state));
        let mut s = PipeSensord {
            control: Some(tx),
            writer: Some(writer),
            reader: None,
            state,
        };

        // Park the writer thread inside its first blocking write...
        s.set_interval(500);
        std::thread::sleep(std::time::Duration::from_millis(50));
        // ...then prove queueing further control traffic does not wait on it.
        let start = std::time::Instant::now();
        s.set_interval(1000);
        s.shutdown();
        assert!(
            start.elapsed() < std::time::Duration::from_millis(100),
            "control sends must not block on pipe I/O (took {:?})",
            start.elapsed()
        );
        // Unpark the writer thread so it can drain the queue and exit.
        drop(release_tx);
    }
}
