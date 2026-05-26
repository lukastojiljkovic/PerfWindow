//! Integration test: spawn `sensord.exe --service --pipe-name X`, connect to
//! the pipe, verify NDJSON snapshots arrive with the new `health` field.
//!
//! Requires sensord.exe to be built; cargo's build.rs copies it next to the
//! release binary, so this picks up the same artifact at runtime.

use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const TEST_PIPE: &str = "PerfWindowSensorTest_v080";

struct GuardedChild(Child);
impl Drop for GuardedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn locate_sensord() -> std::path::PathBuf {
    // cargo runs integration tests with cwd = crate root; the binary the test
    // links into lives under target/<profile>/deps/. sensord.exe sits one
    // level up, next to PerfWindow.exe.
    let exe = std::env::current_exe().unwrap();
    let mut p = exe.clone();
    p.pop(); // drop test binary name
    if p.ends_with("deps") {
        p.pop(); // drop /deps
    }
    p.push("sensord.exe");
    p
}

#[test]
fn sensord_service_emits_snapshots_over_pipe() {
    let sensord = locate_sensord();
    assert!(
        sensord.exists(),
        "sensord.exe missing at {}",
        sensord.display()
    );

    let mut child = GuardedChild(
        Command::new(&sensord)
            .args(["--service", "--pipe-name", TEST_PIPE])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn sensord"),
    );

    // Open the pipe with a retry loop: the pipe doesn't exist until sensord
    // finishes its PawnIO probe (a few seconds on cold boot) and calls
    // WaitForConnectionAsync. While waiting we poll child.try_wait() so a
    // crash during boot is reported with stdout/stderr instead of a generic
    // timeout. We avoid Path::exists() because GetFileAttributes on a named
    // pipe behaves inconsistently w.r.t. the "instance is busy" check.
    use std::fs::OpenOptions;
    use std::io::{BufRead, BufReader, ErrorKind};
    use std::os::windows::fs::OpenOptionsExt;

    let pipe_path = format!(r"\\.\pipe\{TEST_PIPE}");
    let start = Instant::now();
    let deadline = start + Duration::from_secs(15);
    let f = loop {
        match OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(0x4000_0000) // FILE_FLAG_OVERLAPPED
            .open(&pipe_path)
        {
            Ok(f) => break f,
            Err(e)
                if e.kind() == ErrorKind::NotFound
                    || e.raw_os_error() == Some(231) /* ERROR_PIPE_BUSY */ =>
            {
                if Instant::now() >= deadline {
                    panic!(
                        "could not open {pipe_path} within {:?}: last error {e}",
                        start.elapsed()
                    );
                }
                if let Ok(Some(status)) = child.0.try_wait() {
                    let mut out = String::new();
                    let mut err = String::new();
                    if let Some(mut s) = child.0.stdout.take() {
                        use std::io::Read;
                        let _ = s.read_to_string(&mut out);
                    }
                    if let Some(mut s) = child.0.stderr.take() {
                        use std::io::Read;
                        let _ = s.read_to_string(&mut err);
                    }
                    panic!(
                        "sensord exited early with status {status:?} before pipe opened\nstdout:\n{out}\nstderr:\n{err}"
                    );
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => panic!("open test pipe: {e}"),
        }
    };

    let mut lines = BufReader::new(f).lines();
    let line = lines
        .next()
        .expect("at least one line")
        .expect("line read ok");
    assert!(
        line.starts_with("{\"v\":"),
        "snapshot missing version prefix: {line}"
    );
    assert!(
        line.contains("\"health\""),
        "snapshot missing health: {line}"
    );

    drop(child);
}
