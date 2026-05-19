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
    exe_path: std::path::PathBuf,
    pub state: SharedState,
}

impl Sensord {
    /// Extract the embedded executable to the temp directory, spawn it, and
    /// start the reader thread. `repaint` is called whenever a new snapshot
    /// arrives so the UI wakes up.
    pub fn spawn(repaint: impl Fn() + Send + 'static) -> std::io::Result<Sensord> {
        let exe_path =
            std::env::temp_dir().join(format!("PerfWindow-sensord-{}.exe", std::process::id()));
        std::fs::write(&exe_path, SENSORD_BYTES)?;

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
            exe_path,
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
        let _ = std::fs::remove_file(&self.exe_path);
    }
}
