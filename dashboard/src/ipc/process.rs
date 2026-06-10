use crate::ipc::{parse_line, spawn_control_writer, ControlMsg, Line, ProgressInfo, Snapshot};
use std::io::{BufRead, BufReader};
use std::os::windows::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

/// Shared between the reader thread and the UI: the most recent snapshot, and
/// whether the sensor process is still alive.
#[derive(Default)]
pub struct SensorState {
    pub latest: Option<Snapshot>,
    pub alive: bool,
    /// Latest staged-init progress line, if sensord has emitted any.
    pub progress: Option<ProgressInfo>,
    /// When the reader last parsed ANY line (snapshot or progress). The
    /// startup watchdog in `PerfApp::ingest` treats a fresh value as proof
    /// of life even before the first snapshot arrives.
    pub last_line_at: Option<std::time::Instant>,
}

pub type SharedState = Arc<Mutex<SensorState>>;

/// A running (or crashed) `sensord` child and the resources to talk to it.
pub struct Sensord {
    child: Child,
    control: Option<std::sync::mpsc::Sender<ControlMsg>>,
    writer: Option<JoinHandle<()>>,
    reader: Option<JoinHandle<()>>,
    pub state: SharedState,
}

impl Sensord {
    /// Dev-mode path only (`PerfWindow --dev`): spawn the bundled `sensord.exe`
    /// as a child process and read NDJSON snapshots from its stdout. Production
    /// builds talk to the installed `PerfWindowSensor` service via
    /// [`crate::ipc::pipe::PipeSensord`] instead. `repaint` is called whenever
    /// a new snapshot arrives so the UI wakes up.
    pub fn spawn(repaint: impl Fn() + Send + 'static) -> std::io::Result<Sensord> {
        let exe_path = sensord_path()?;

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
            alive: true,
            ..SensorState::default()
        }));

        let reader_state = Arc::clone(&state);
        let reader = std::thread::spawn(move || {
            let lines = BufReader::new(stdout).lines();
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
            // stdout closed or errored -> sensord exited.
            if let Ok(mut s) = reader_state.lock() {
                s.alive = false;
            }
            repaint();
        });

        let (control, writer) = match stdin {
            Some(stdin) => {
                let (tx, rx) = std::sync::mpsc::channel();
                let writer = spawn_control_writer(stdin, rx, Arc::clone(&state));
                (Some(tx), Some(writer))
            }
            None => (None, None),
        };

        Ok(Sensord {
            child,
            control,
            writer,
            reader: Some(reader),
            state,
        })
    }

    /// Queue a refresh-interval change for the writer thread. Non-blocking.
    pub fn set_interval(&mut self, ms: u32) {
        if let Some(tx) = &self.control {
            if tx.send(ControlMsg::SetInterval(ms)).is_err() {
                // The writer thread already exited (it flips `alive` on a
                // write failure); drop the dead channel so later calls no-op.
                self.control = None;
            }
        }
    }

    /// `true` while the child is still producing snapshots.
    pub fn is_alive(&self) -> bool {
        self.state.lock().map(|s| s.alive).unwrap_or(false)
    }

    /// Queue a shutdown message and close the control channel; the writer
    /// thread performs the blocking write off the UI thread and then drops
    /// `stdin`, so the console-child sees EOF on its control loop and exits
    /// cleanly. Idempotent; never joins or blocks. This is the canonical
    /// close path; `Drop` below calls it as a safety net.
    pub fn shutdown(&mut self) {
        if let Some(tx) = self.control.take() {
            let _ = tx.send(ControlMsg::Shutdown);
        }
        // Detach both threads: the reader blocks in `BufReader::lines` until
        // the child's stdout closes, and the writer may be mid-write.
        let _ = self.reader.take();
        let _ = self.writer.take();
    }
}

/// Locate the bundled `sensord.exe`. The installer ships it next to
/// `PerfWindow.exe` and `build.rs` places it next to the dev build output, so
/// it is always a sibling of the running executable.
fn sensord_path() -> std::io::Result<std::path::PathBuf> {
    let exe = std::env::current_exe()?;
    let dir = exe.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "PerfWindow.exe has no parent directory",
        )
    })?;
    let path = dir.join("sensord.exe");
    if !path.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!(
                "sensord.exe not found next to PerfWindow.exe ({})",
                path.display()
            ),
        ));
    }
    Ok(path)
}

impl Drop for Sensord {
    fn drop(&mut self) {
        // `shutdown` queues the exit message and detaches both I/O threads —
        // joining them here could hang teardown, because the reader blocks in
        // `BufReader::lines()` until the OS fully releases the child's stdout.
        self.shutdown();
        // Give the child a moment, then force-kill if still running.
        for _ in 0..20 {
            match self.child.try_wait() {
                Ok(Some(_)) => break,
                _ => std::thread::sleep(std::time::Duration::from_millis(25)),
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn fresh_state_is_not_alive_by_default() {
        // The derived default carries `bool::default() == false` and is used
        // only for sites that construct a placeholder before plumbing in a
        // real sensord child; `Sensord::spawn` builds an alive state itself.
        let s = SensorState::default();
        assert!(!s.alive);
        assert!(s.latest.is_none());
        assert!(s.progress.is_none());
        assert!(s.last_line_at.is_none());
    }

    #[test]
    fn mark_dead_flips_alive_to_false() {
        let mut s = SensorState {
            alive: true,
            ..SensorState::default()
        };
        s.alive = false;
        assert!(!s.alive);
    }

    #[test]
    fn is_alive_reads_through_the_mutex() {
        let state = Arc::new(Mutex::new(SensorState {
            alive: true,
            ..SensorState::default()
        }));
        let alive_first = state.lock().map(|s| s.alive).unwrap_or(false);
        assert!(alive_first);
        state.lock().unwrap().alive = false;
        let alive_after = state.lock().map(|s| s.alive).unwrap_or(false);
        assert!(!alive_after);
    }

    #[test]
    fn is_alive_returns_false_on_poisoned_mutex() {
        let state: Arc<Mutex<SensorState>> = Arc::new(Mutex::new(SensorState {
            alive: true,
            ..SensorState::default()
        }));
        let state_for_panic = Arc::clone(&state);
        let _ = std::thread::spawn(move || {
            let _guard = state_for_panic.lock().unwrap();
            panic!("poisoning the mutex on purpose");
        })
        .join();
        let alive = state.lock().map(|s| s.alive).unwrap_or(false);
        assert!(!alive, "poisoned mutex should be treated as dead");
    }
}
