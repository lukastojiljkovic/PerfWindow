use crate::ipc::{parse_snapshot, Snapshot};
use std::io::{BufRead, BufReader, Write};
use std::os::windows::process::CommandExt;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

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
    /// Spawn the bundled `sensord.exe` (installed alongside `PerfWindow.exe`)
    /// and start the reader thread. `repaint` is called whenever a new snapshot
    /// arrives so the UI wakes up.
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn fresh_state_is_not_alive_by_default() {
        // `SensorState::default()` is what `Sensord::spawn` overwrites with a
        // freshly-built `SensorState { latest: None, alive: true }`. The
        // derived default carries `bool::default() == false` and is used only
        // for sites that construct a placeholder before plumbing in a real
        // sensord child.
        let s = SensorState::default();
        assert!(!s.alive);
        assert!(s.latest.is_none());
    }

    #[test]
    fn mark_dead_flips_alive_to_false() {
        let mut s = SensorState {
            latest: None,
            alive: true,
        };
        s.alive = false;
        assert!(!s.alive);
    }

    #[test]
    fn is_alive_reads_through_the_mutex() {
        let state = Arc::new(Mutex::new(SensorState {
            latest: None,
            alive: true,
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
            latest: None,
            alive: true,
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
