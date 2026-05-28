use crate::ipc::{parse_snapshot, SensorState, SharedState};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::os::windows::fs::OpenOptionsExt;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

const PIPE_PATH: &str = r"\\.\pipe\PerfWindowSensor";
const FILE_FLAG_OVERLAPPED: u32 = 0x4000_0000;

#[derive(Debug)]
pub enum ConnectError {
    /// The pipe does not exist: service not installed or not running.
    NotFound,
    /// Another dashboard is already connected.
    Busy,
    /// ACL refused access (unexpected for Authenticated Users).
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
    writer: Option<File>,
    reader: Option<JoinHandle<()>>,
    pub state: SharedState,
}

/// Idempotently start the PerfWindowSensor service via sc.exe.
///
/// The installer grants Authenticated Users SERVICE_START on the service
/// ACL, so this does not require elevation when run from a normal user
/// account. If the service is already running, sc returns 1056
/// (ERROR_SERVICE_ALREADY_RUNNING) — we ignore the exit code either way.
fn try_start_service() {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let _ = std::process::Command::new("sc.exe")
        .args(["start", "PerfWindowSensor"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .status();
}

impl PipeSensord {
    /// Legacy single-shot connect path retained for tests: open the pipe,
    /// best-effort start the service if the pipe is missing, retry. Production
    /// callers use the state machine in `connect.rs`, which routes the same
    /// open through a UAC step and surfaces phase updates to the UI.
    pub fn connect(repaint: impl Fn() + Send + 'static) -> Result<Self, ConnectError> {
        let read = match Self::open_pipe_at(PIPE_PATH) {
            Ok(f) => f,
            Err(ConnectError::NotFound) => {
                // Service is not running — start it and wait briefly for the
                // pipe to appear, then retry.
                try_start_service();
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
                loop {
                    std::thread::sleep(std::time::Duration::from_millis(200));
                    match Self::open_pipe_at(PIPE_PATH) {
                        Ok(f) => break f,
                        Err(_) if std::time::Instant::now() >= deadline => {
                            return Err(ConnectError::NotFound);
                        }
                        Err(_) => continue,
                    }
                }
            }
            Err(e) => return Err(e),
        };

        Self::start_reader(read, repaint)
    }

    /// Open the pipe at the default location without trying to elevate.
    /// The connect state machine uses this in every phase that polls.
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
        OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(FILE_FLAG_OVERLAPPED)
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
        let writer = read.try_clone().map_err(ConnectError::from)?;
        let state: SharedState = Arc::new(Mutex::new(SensorState {
            latest: None,
            alive: true,
        }));
        let reader_state = Arc::clone(&state);
        let reader = std::thread::spawn(move || {
            let lines = BufReader::new(read).lines();
            for line in lines {
                match line {
                    Ok(line) => {
                        if let Some(snap) = parse_snapshot(&line) {
                            if let Ok(mut s) = reader_state.lock() {
                                s.latest = Some(snap);
                            }
                            repaint();
                        }
                    }
                    Err(_) => break,
                }
            }
            if let Ok(mut s) = reader_state.lock() {
                s.alive = false;
            }
            repaint();
        });
        Ok(PipeSensord {
            writer: Some(writer),
            reader: Some(reader),
            state,
        })
    }

    pub fn set_interval(&mut self, ms: u32) {
        if let Some(w) = &mut self.writer {
            let _ = writeln!(w, "{{\"interval_ms\":{ms}}}");
        }
    }

    pub fn is_alive(&self) -> bool {
        self.state.lock().map(|s| s.alive).unwrap_or(false)
    }

    /// Tell the worker to exit cleanly: write `{"shutdown":true}` on the
    /// upstream half, then drop our writer to send EOF as a belt-and-braces
    /// signal. Idempotent — calling more than once is a no-op.
    ///
    /// This is the canonical close path. The `Drop` impl below also calls
    /// `shutdown` as a safety net, but the dashboard prefers to invoke this
    /// explicitly from `eframe::App::on_exit` while the frame is still alive
    /// (more reliable than relying on `Drop` during process tear-down).
    pub fn shutdown(&mut self) {
        if let Some(mut w) = self.writer.take() {
            let _ = writeln!(w, "{{\"shutdown\":true}}");
            // `w` drops here, closing our writer half of the pipe.
        }
        // The reader thread is detached (see Drop history); drop the JoinHandle
        // without joining, exactly as Drop has done since v0.8.1.
        let _ = self.reader.take();
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
    crate::ui::shell::shell_exec_runas("sc.exe", "start PerfWindowSensor")
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
        let mut s = PipeSensord {
            writer: None,
            reader: None,
            state: Arc::new(Mutex::new(SensorState {
                latest: None,
                alive: false,
            })),
        };
        s.shutdown();
        s.shutdown(); // must not panic, must not deadlock
    }
}
