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

    let pipe_path = format!(r"\\.\pipe\{TEST_PIPE}");
    let start = Instant::now();
    let deadline = start + Duration::from_secs(15);
    let f = loop {
        // Synchronous (blocking) open — NO FILE_FLAG_OVERLAPPED. A sync read on
        // an overlapped handle can return ERROR_IO_PENDING and abort the
        // process; see `PipeSensord::open_pipe_at`. This loop reads with
        // `BufReader::lines()`, so it must use a blocking handle too.
        match OpenOptions::new()
            .read(true)
            .write(true)
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

    // Since 0.10.0 the feed starts with `{"progress":...}` staged-init lines;
    // the first `{"v":...}` snapshot follows once CPU+RAM sensors are open.
    let mut lines = BufReader::new(f).lines();
    let mut saw_progress = false;
    let snapshot = loop {
        let line = lines
            .next()
            .expect("stream ended before a snapshot arrived")
            .expect("line read ok");
        if line.starts_with("{\"progress\":") {
            saw_progress = true;
            continue;
        }
        break line;
    };
    assert!(saw_progress, "no progress line preceded the first snapshot");
    assert!(
        snapshot.starts_with("{\"v\":"),
        "snapshot missing version prefix: {snapshot}"
    );
    assert!(
        snapshot.contains("\"health\""),
        "snapshot missing health: {snapshot}"
    );

    drop(child);
}

#[test]
fn pipe_sensord_shutdown_terminates_child() {
    let sensord = locate_sensord();
    assert!(
        sensord.exists(),
        "sensord.exe missing at {}",
        sensord.display()
    );

    let pipe = format!("PerfWindowSensorShutdownTest_{}", std::process::id());

    let mut child = GuardedChild(
        Command::new(&sensord)
            .args(["--service", "--pipe-name", &pipe])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn sensord"),
    );

    // Wait for the pipe to be ready, then connect through PipeSensord::connect_to.
    let pipe_path = format!(r"\\.\pipe\{pipe}");
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut sensord_client = loop {
        std::thread::sleep(Duration::from_millis(200));
        match perfwindow::ipc::pipe::PipeSensord::connect_to(&pipe_path, || {}) {
            Ok(s) => break s,
            Err(_) if Instant::now() < deadline => continue,
            Err(e) => {
                let _ = child.0.kill();
                panic!("pipe never came up within 15s: {e}");
            }
        }
    };

    // Send the shutdown signal.
    sensord_client.shutdown();

    // Drop the client to release our pipe handle entirely (belt-and-braces;
    // shutdown already wrote the message and closed the writer).
    drop(sensord_client);

    // The worker must exit cleanly. Worst case is bounded by sensord's
    // teardown watchdog (3.5 s grace, then forced exit) plus a synchronous
    // LHM call that was already in flight; 8 s keeps the assertion about
    // "exits promptly, never lingers" without flaking when the shutdown
    // lands mid category-enable on a loaded machine.
    let exit_deadline = Instant::now() + Duration::from_secs(8);
    loop {
        match child.0.try_wait() {
            Ok(Some(status)) => {
                assert!(
                    status.success(),
                    "sensord exited non-zero after shutdown: {status:?}"
                );
                return;
            }
            Ok(None) if Instant::now() < exit_deadline => {
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }
            Ok(None) => {
                let _ = child.0.kill();
                panic!("sensord did not exit within 8s after shutdown");
            }
            Err(e) => panic!("try_wait failed: {e}"),
        }
    }
}

/// Regression guard for the 0.9.1–0.9.4 startup crash: the pipe client must
/// open in **synchronous (blocking)** mode. A handle opened with
/// `FILE_FLAG_OVERLAPPED` that is then read synchronously (as the reader thread
/// does via `BufReader::lines()`) can return `ERROR_IO_PENDING`, which the Rust
/// standard library treats as a fatal runtime error and `abort()`s the whole
/// process — surfacing as `0xc0000409` with no panic-hook entry.
///
/// This test stands up a minimal named-pipe server and connects the real
/// `PipeSensord` client, then writes the first snapshot only *after* a delay,
/// so the client's reader is forced to block on an empty pipe — the exact
/// timing that aborted the shipped builds. If `open_pipe_at` ever reopens the
/// pipe overlapped, the reader aborts here and takes the whole test binary
/// down (a hard CI failure), which is the regression we want to catch.
#[test]
fn pipe_client_blocks_on_empty_pipe_without_aborting() {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use std::ptr;

    // Win32 named-pipe server primitives (kernel32 is implicitly linked, same
    // as the FFI in `ui/shell.rs` and `main.rs`).
    extern "system" {
        fn CreateNamedPipeW(
            name: *const u16,
            open_mode: u32,
            pipe_mode: u32,
            max_instances: u32,
            out_buffer_size: u32,
            in_buffer_size: u32,
            default_timeout: u32,
            security_attributes: *mut core::ffi::c_void,
        ) -> isize;
        fn ConnectNamedPipe(handle: isize, overlapped: *mut core::ffi::c_void) -> i32;
        fn WriteFile(
            handle: isize,
            buf: *const u8,
            len: u32,
            written: *mut u32,
            overlapped: *mut core::ffi::c_void,
        ) -> i32;
        fn FlushFileBuffers(handle: isize) -> i32;
        fn CloseHandle(handle: isize) -> i32;
    }
    const PIPE_ACCESS_DUPLEX: u32 = 0x0000_0003;
    const PIPE_TYPE_BYTE: u32 = 0x0000_0000;
    const PIPE_WAIT: u32 = 0x0000_0000;
    const INVALID_HANDLE_VALUE: isize = -1;

    let name = format!(r"\\.\pipe\PerfWindowOverlapRegr_{}", std::process::id());
    let wide: Vec<u16> = OsStr::new(&name)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let server = unsafe {
        CreateNamedPipeW(
            wide.as_ptr(),
            PIPE_ACCESS_DUPLEX,
            PIPE_TYPE_BYTE | PIPE_WAIT,
            1,
            4096,
            4096,
            0,
            ptr::null_mut(),
        )
    };
    assert_ne!(
        server,
        INVALID_HANDLE_VALUE,
        "CreateNamedPipeW failed: {}",
        std::io::Error::last_os_error()
    );

    // Server side: accept the client, wait long enough that the client's first
    // read is issued against an empty pipe, then deliver one snapshot line.
    let server_thread = std::thread::spawn(move || {
        // Returns 0 with ERROR_PIPE_CONNECTED if the client beat us here; both
        // outcomes mean "client attached", so the result is intentionally ignored.
        let _ = unsafe { ConnectNamedPipe(server, ptr::null_mut()) };
        std::thread::sleep(Duration::from_millis(400));
        let line = b"{\"v\":1,\"ts\":1,\"cpu\":{\"name\":\"Regr\",\"load\":1.0}}\n";
        let mut written = 0u32;
        unsafe {
            WriteFile(
                server,
                line.as_ptr(),
                line.len() as u32,
                &mut written,
                ptr::null_mut(),
            );
            FlushFileBuffers(server);
        }
        // Hold the pipe open briefly so the client finishes reading the line.
        std::thread::sleep(Duration::from_millis(300));
        unsafe { CloseHandle(server) };
    });

    // The real client. With the fix this opens a blocking handle and the reader
    // thread waits 400 ms for the delayed write; with the old overlapped flag
    // the reader would abort the process before this returns.
    let client = perfwindow::ipc::pipe::PipeSensord::connect_to(&name, || {})
        .expect("client connects to the test pipe");

    // The snapshot must arrive — proving the blocking read completed instead of
    // aborting on ERROR_IO_PENDING.
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if client
            .state
            .lock()
            .map(|s| s.latest.is_some())
            .unwrap_or(false)
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "no snapshot within 5s — blocking read did not deliver the delayed line"
        );
        std::thread::sleep(Duration::from_millis(25));
    }

    drop(client);
    let _ = server_thread.join();
}
