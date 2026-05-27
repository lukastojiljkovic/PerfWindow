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

impl PipeSensord {
    pub fn connect(repaint: impl Fn() + Send + 'static) -> Result<Self, ConnectError> {
        let read = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(FILE_FLAG_OVERLAPPED)
            .open(PIPE_PATH)?;
        let writer = read.try_clone()?;

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
}

impl Drop for PipeSensord {
    fn drop(&mut self) {
        // Drop the writer handle so the server's reader-side sees EOF.
        // We deliberately do NOT join the reader thread: it is blocked in
        // BufReader::lines() and the only way to unblock it would be to
        // close the pipe, but the reader's own File handle (moved into
        // the closure) keeps the client side open after we drop the
        // writer above. Joining would deadlock; we'd need
        // CancelSynchronousIo against the reader thread to truly interrupt
        // the read, which is more complexity than the win is worth.
        //
        // Instead: drop the JoinHandle, which detaches the thread. The OS
        // reaps it on process exit, the reader's pipe handle closes, the
        // server-side I/O returns IOException, and the server cleans up.
        self.writer.take();
        let _ = self.reader.take();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
