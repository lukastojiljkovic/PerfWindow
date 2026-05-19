use crate::ipc::{parse_snapshot, Snapshot};
use std::io::{BufRead, BufReader, Write};
use std::os::windows::process::CommandExt;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

/// The embedded sensord executable, staged into OUT_DIR by build.rs.
const SENSORD_BYTES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/sensord.exe"));

/// Shared between the reader thread and the UI: the most recent snapshot, and
/// whether the sensor process is still alive.
#[derive(Default)]
pub struct SensorState {
    pub latest: Option<Snapshot>,
    pub alive: bool,
}

pub type SharedState = Arc<Mutex<SensorState>>;

/// A running (or crashed) `sensord` child and the resources to talk to it.
pub struct Sensord {
    child: Child,
    stdin: Option<ChildStdin>,
    reader: Option<JoinHandle<()>>,
    pub state: SharedState,
}

impl Sensord {
    /// Stage the embedded executable into `%LOCALAPPDATA%\PerfWindow`, spawn
    /// it, and start the reader thread. `repaint` is called whenever a new
    /// snapshot arrives so the UI wakes up.
    pub fn spawn(repaint: impl Fn() + Send + 'static) -> std::io::Result<Sensord> {
        let exe_path = stage_sensord()?;

        // sensord is a console-subsystem process; without CREATE_NO_WINDOW
        // Windows opens a console window for the spawned child.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let mut child = Command::new(&exe_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()?;

        let stdout = child.stdout.take().expect("piped stdout");
        let stdin = child.stdin.take();
        let state: SharedState = Arc::new(Mutex::new(SensorState {
            latest: None,
            alive: true,
        }));

        let reader_state = Arc::clone(&state);
        let reader = std::thread::spawn(move || {
            let lines = BufReader::new(stdout).lines();
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
            // stdout closed or errored -> sensord exited.
            if let Ok(mut s) = reader_state.lock() {
                s.alive = false;
            }
            repaint();
        });

        Ok(Sensord {
            child,
            stdin,
            reader: Some(reader),
            state,
        })
    }

    /// Send a refresh-interval change to sensord.
    pub fn set_interval(&mut self, ms: u32) {
        if let Some(stdin) = &mut self.stdin {
            let _ = writeln!(stdin, "{{\"interval_ms\":{ms}}}");
        }
    }

    /// `true` while the child is still producing snapshots.
    pub fn is_alive(&self) -> bool {
        self.state.lock().map(|s| s.alive).unwrap_or(false)
    }
}

/// `%LOCALAPPDATA%\PerfWindow` — the per-user directory the staged `sensord`
/// executable lives in.
fn sensord_dir() -> std::io::Result<std::path::PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .map(|base| std::path::PathBuf::from(base).join("PerfWindow"))
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "LOCALAPPDATA is not set"))
}

/// Write the embedded `sensord` executable to its per-user location and return
/// the path. The bytes are rewritten on every launch so the staged copy always
/// matches this build; if another PerfWindow instance is already running it
/// (so the file is locked), the existing identical copy is reused instead.
fn stage_sensord() -> std::io::Result<std::path::PathBuf> {
    let dir = sensord_dir()?;
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("sensord.exe");
    match std::fs::write(&path, SENSORD_BYTES) {
        Ok(()) => Ok(path),
        Err(e) if path.exists() => {
            eprintln!("PerfWindow: reusing the staged sensord ({e})");
            Ok(path)
        }
        Err(e) => Err(e),
    }
}

impl Drop for Sensord {
    fn drop(&mut self) {
        // Closing stdin makes sensord's stdin reach EOF; it then exits its poll
        // loop cleanly (see sensord Program.cs).
        self.stdin.take();
        // Give it a moment, then force-kill if still running.
        for _ in 0..20 {
            match self.child.try_wait() {
                Ok(Some(_)) => break,
                _ => std::thread::sleep(std::time::Duration::from_millis(25)),
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
        // The staged executable in %LOCALAPPDATA% is left in place — the next
        // launch reuses it (and re-stages it on demand).
    }
}
