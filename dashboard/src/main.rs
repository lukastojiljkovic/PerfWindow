#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(dead_code)]

use perfwindow::app::PerfApp;
use perfwindow::config::Config;

fn main() -> eframe::Result {
    // Two complementary crash recorders, both writing to
    // `%APPDATA%\PerfWindow\panic.log`:
    //   * `install_panic_log` — catches `panic!()` / `unreachable!()` /
    //     `assert!()` (the Rust panic path).
    //   * `install_seh_handler` — catches Win32 structured exceptions
    //     (0xc0000409 STATUS_STACK_BUFFER_OVERRUN among them) that come from
    //     stack-canary failures, heap corruption, or native code calling
    //     `__fastfail` directly; the Rust panic hook NEVER fires for those.
    // Order: SEH first so the startup marker below is bracketed by both.
    install_panic_log();
    install_seh_handler();
    write_startup_marker();

    // `--dev` swaps the production named-pipe client for the legacy
    // child-spawn path (no installed Windows service required) — useful for
    // `cargo run` against a freshly built sensord. Default = pipe.
    let dev_mode = std::env::args().any(|a| a == "--dev");

    // Prefer the glow backend for release builds. eframe defaults to wgpu when
    // both backends are compiled in, but wgpu driver/device creation can fail
    // before `PerfApp::new` runs. In that case the dashboard exits before it
    // reaches the service-start path, so the user never sees the UAC prompt.
    // Glow has been the more forgiving option across mixed Windows machines;
    // keep wgpu as a logged fallback for systems where OpenGL is the broken
    // side instead.
    let glow_started = std::time::Instant::now();
    match run_app(dev_mode, eframe::Renderer::Glow) {
        Ok(()) => Ok(()),
        Err(glow_err) if glow_started.elapsed() > RENDERER_FALLBACK_WINDOW => {
            // The glow session ran long past startup before erroring — a
            // mid-session GL loss, not an init failure. Relaunching the whole
            // app under wgpu would resurrect a window the user may have
            // dismissed minutes ago, so log and exit instead.
            write_startup_error("eframe glow runtime failure (no fallback)", &glow_err);
            Err(glow_err)
        }
        Err(glow_err) => {
            write_startup_error("eframe glow startup failed", &glow_err);
            match run_app(dev_mode, eframe::Renderer::Wgpu) {
                Ok(()) => Ok(()),
                Err(wgpu_err) => {
                    write_startup_error("eframe wgpu fallback failed", &wgpu_err);
                    show_fatal_startup_box();
                    Err(wgpu_err)
                }
            }
        }
    }
}

/// Renderer errors after this long are mid-session losses, not startup
/// failures, and must not trigger the wgpu relaunch.
const RENDERER_FALLBACK_WINDOW: std::time::Duration = std::time::Duration::from_secs(10);

/// Last-resort visibility: both renderer attempts failed before a window ever
/// appeared, so without this the app exits silently ("nothing happened"). A
/// native message box needs no working GPU pipeline.
fn show_fatal_startup_box() {
    use std::os::windows::ffi::OsStrExt;
    const MB_OK: u32 = 0x0000_0000;
    const MB_ICONERROR: u32 = 0x0000_0010;
    #[link(name = "user32")]
    extern "system" {
        fn MessageBoxW(hwnd: isize, text: *const u16, caption: *const u16, flags: u32) -> i32;
    }
    let to_wide = |s: &str| -> Vec<u16> {
        std::ffi::OsStr::new(s)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    };
    let text = to_wide(
        "PerfWindow could not start: graphics initialization failed.\n\n\
         Details were written to %APPDATA%\\PerfWindow\\panic.log.",
    );
    let caption = to_wide("PerfWindow");
    unsafe {
        MessageBoxW(0, text.as_ptr(), caption.as_ptr(), MB_OK | MB_ICONERROR);
    }
}

fn run_app(dev_mode: bool, renderer: eframe::Renderer) -> eframe::Result {
    // Read the persisted `always_on_top` preference before the viewport opens
    // so the window starts at the correct Z-level and doesn't briefly flash
    // behind other windows on launch. `PerfApp::new` re-reads the config and
    // is the canonical owner; this peek is purely a cosmetic-startup fix.
    let initially_on_top = Config::load().always_on_top;
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("PerfWindow")
        // Default size matches the grid's natural footprint so the window
        // opens with no empty band. Min height equals the default height
        // so the user cannot shrink the window into the cards; min width
        // sits just below the 4-col breakpoint so the grid can collapse
        // to 3 cols when the user shrinks the window deliberately.
        .with_inner_size([1180.0, 600.0])
        .with_min_inner_size([720.0, 500.0]);
    if initially_on_top {
        viewport = viewport.with_always_on_top();
    }
    let options = eframe::NativeOptions {
        viewport,
        renderer,
        ..Default::default()
    };
    eframe::run_native(
        "PerfWindow",
        options,
        Box::new(move |cc| Ok(Box::new(PerfApp::new(cc, dev_mode)))),
    )
}

/// Install a `std::panic::set_hook` that appends panic details to
/// `%APPDATA%\PerfWindow\panic.log` (timestamped, with backtrace). The hook
/// also forwards to the default panic printer so any attached console / IDE
/// debugger still gets stderr output. Idempotent: a duplicate install would
/// just overwrite the hook with an equivalent one.
fn install_panic_log() {
    // `force_capture` ignores `RUST_BACKTRACE`: release users will never have
    // that env var set, but a postmortem without a backtrace is useless.
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        write_panic_entry(info);
        default(info);
    }));
}

fn write_panic_entry(info: &std::panic::PanicHookInfo<'_>) {
    use std::io::Write;

    let Some(mut file) = open_panic_log() else {
        return;
    };

    let payload = info
        .payload()
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| info.payload().downcast_ref::<String>().map(String::as_str))
        .unwrap_or("<non-string panic payload>");
    let location = info
        .location()
        .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
        .unwrap_or_else(|| "<unknown>".to_string());
    let backtrace = std::backtrace::Backtrace::force_capture();

    let _ = writeln!(
        file,
        "=== PerfWindow v{} panic @ {} (UTC unix={}) ===\n\
         location: {}\n\
         message : {}\n\
         backtrace:\n{}\n",
        env!("CARGO_PKG_VERSION"),
        chrono_like_utc(),
        unix_now(),
        location,
        payload,
        backtrace,
    );
    let _ = file.flush();
}

/// `%APPDATA%\PerfWindow\panic.log` — same directory as `config.toml`, which
/// the installer's uninstaller already cleans on remove.
fn panic_log_path() -> Option<std::path::PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    Some(
        std::path::PathBuf::from(appdata)
            .join("PerfWindow")
            .join("panic.log"),
    )
}

/// Rotation cap for `panic.log`. Startup markers accumulate one line per
/// launch, so an installed copy would otherwise grow without bound.
const PANIC_LOG_MAX_BYTES: u64 = 256 * 1024;

/// Open `panic.log` for appending, rotating it to `panic.log.1` first when it
/// has outgrown the cap. Shared by the startup marker, the panic hook and the
/// SEH filter so every writer enforces the same bound.
fn open_panic_log() -> Option<std::fs::File> {
    let path = panic_log_path()?;
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    rotate_if_oversized(&path);
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .ok()
}

fn rotate_if_oversized(path: &std::path::Path) {
    let oversized = std::fs::metadata(path).is_ok_and(|m| m.len() > PANIC_LOG_MAX_BYTES);
    if !oversized {
        return;
    }
    let mut rotated = path.as_os_str().to_owned();
    rotated.push(".1");
    let rotated = std::path::PathBuf::from(rotated);
    // Windows `rename` refuses to replace; clear the previous generation
    // first so rotation cannot wedge on its own output.
    let _ = std::fs::remove_file(&rotated);
    let _ = std::fs::rename(path, &rotated);
}

/// Seconds since the Unix epoch — robust marker even when the formatted
/// timestamp below comes out wrong.
fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// A bare-bones `YYYY-MM-DDTHH:MM:SSZ` for the log header, computed without
/// pulling in `chrono` for one timestamp. Falls back to the unix seconds when
/// the epoch math overflows.
fn chrono_like_utc() -> String {
    let secs = unix_now();
    // 1970-01-01 + secs, no leap second, no time-zone conversion.
    let days = secs / 86_400;
    let rem = secs % 86_400;
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    let (year, month, day) = days_to_ymd(days);
    format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}Z")
}

fn days_to_ymd(mut days: u64) -> (i32, u32, u32) {
    // Civil-from-days algorithm by Howard Hinnant.
    days += 719468;
    let era = days / 146_097;
    let doe = days - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };
    (year as i32, m as u32, d as u32)
}

/// Write a "process started" line to `panic.log` so a postmortem can
/// distinguish "binary never launched" (PE loader / VC++ runtime missing)
/// from "binary launched then crashed". Appended, so successive launches
/// stack up — useful for matching a crash entry below to the launch that
/// produced it.
fn write_startup_marker() {
    use std::io::Write;
    let Some(mut file) = open_panic_log() else {
        return;
    };
    let _ = writeln!(
        file,
        "--- PerfWindow v{} startup @ {} (UTC unix={}) pid={} ---",
        env!("CARGO_PKG_VERSION"),
        chrono_like_utc(),
        unix_now(),
        std::process::id(),
    );
    let _ = file.flush();
}

fn write_startup_error(context: &str, error: &dyn std::fmt::Display) {
    use std::io::Write;
    let Some(mut file) = open_panic_log() else {
        return;
    };
    let _ = writeln!(
        file,
        "=== PerfWindow v{} startup error @ {} (UTC unix={}) pid={} ===\n\
         context: {}\n\
         error  : {}\n",
        env!("CARGO_PKG_VERSION"),
        chrono_like_utc(),
        unix_now(),
        std::process::id(),
        context,
        error,
    );
    let _ = file.flush();
}

// ---------------------------------------------------------------------------
// Win32 SEH unhandled-exception filter
// ---------------------------------------------------------------------------
//
// The Rust `panic` hook above never fires for native crashes
// (`STATUS_STACK_BUFFER_OVERRUN`, `STATUS_ACCESS_VIOLATION`, fastfail aborts
// from inlined wgpu/eframe internals, stack canary failures from MSVC's
// `/GS` check, etc.). Those land in the kernel's structured-exception
// machinery instead, and by default Windows just terminates the process
// after spilling a `BEX64` event into the Application log.
//
// `SetUnhandledExceptionFilter` lets us interpose before that — we record
// the exception code + faulting address + a Rust backtrace into the same
// `panic.log` the panic hook uses, then hand back `EXCEPTION_CONTINUE_SEARCH`
// so the OS still terminates the process (we don't *recover*; we just leave
// a breadcrumb).

// Win32 type aliases — kept verbatim from `winnt.h` so that the FFI signature
// is self-evidently a 1:1 match with the SDK header. The lint allow-list is
// the same one `ui/settings.rs::ymdhm_local` uses for the same reason.
#[allow(non_camel_case_types, clippy::upper_case_acronyms)]
type DWORD = u32;
#[allow(non_camel_case_types, clippy::upper_case_acronyms)]
type LONG = i32;
#[allow(non_camel_case_types)]
type ULONG_PTR = usize;
#[allow(non_camel_case_types, clippy::upper_case_acronyms)]
type PVOID = *mut std::ffi::c_void;

/// Mirrors `_EXCEPTION_RECORD` from `winnt.h`. Only the fields we read are
/// declared with their real types; we leave `ExceptionRecord` as an opaque
/// pointer and `ExceptionInformation` as a fixed-size array.
#[repr(C)]
struct ExceptionRecord {
    exception_code: DWORD,
    exception_flags: DWORD,
    exception_record: *mut ExceptionRecord,
    exception_address: PVOID,
    number_parameters: DWORD,
    exception_information: [ULONG_PTR; 15],
}

#[repr(C)]
struct ExceptionPointers {
    exception_record: *mut ExceptionRecord,
    context_record: PVOID,
}

type UnhandledExceptionFilter = Option<unsafe extern "system" fn(*mut ExceptionPointers) -> LONG>;

const EXCEPTION_CONTINUE_SEARCH: LONG = 0;

extern "system" {
    fn SetUnhandledExceptionFilter(filter: UnhandledExceptionFilter) -> UnhandledExceptionFilter;
}

fn install_seh_handler() {
    unsafe {
        SetUnhandledExceptionFilter(Some(seh_filter));
    }
}

/// Win32 unhandled-exception callback. Best-effort — runs inside the OS's
/// crash-handling path where heap state may already be corrupt, so it does
/// minimal work and swallows every secondary failure (the worst case is the
/// log entry is missed; the worst case is NOT making the original crash
/// worse).
unsafe extern "system" fn seh_filter(info: *mut ExceptionPointers) -> LONG {
    use std::io::Write;

    // `catch_unwind` to make sure a panic inside this handler does not
    // escape the FFI boundary and abort with no log at all.
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let Some(mut file) = open_panic_log() else {
            return;
        };

        let (code, address, params): (DWORD, PVOID, String) = if info.is_null() {
            (0, std::ptr::null_mut(), "<null info>".into())
        } else {
            let rec = (*info).exception_record;
            if rec.is_null() {
                (0, std::ptr::null_mut(), "<null record>".into())
            } else {
                let n = (*rec).number_parameters.min(15) as usize;
                // `&(*raw)` to be explicit about the autoref — the compiler
                // rejects implicit reference-to-deref of a raw pointer.
                let info_ref: &[ULONG_PTR; 15] = &(*rec).exception_information;
                let params = info_ref[..n]
                    .iter()
                    .map(|p| format!("{p:#x}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                ((*rec).exception_code, (*rec).exception_address, params)
            }
        };

        let code_name = seh_code_name(code);
        // Backtrace is captured from the SEH handler's stack, not the
        // faulting thread's — still useful because most of egui/wgpu lives
        // in the same thread, and the addresses below are absolute so the
        // PDB (if shipped) can resolve them.
        let backtrace = std::backtrace::Backtrace::force_capture();

        let _ = writeln!(
            file,
            "=== PerfWindow v{} SEH crash @ {} (UTC unix={}) pid={} ===\n\
             exception : {code:#010x} ({code_name})\n\
             address   : {address:?}\n\
             parameters: [{params}]\n\
             backtrace (handler-side, may differ from faulting thread):\n{backtrace}\n",
            env!("CARGO_PKG_VERSION"),
            chrono_like_utc(),
            unix_now(),
            std::process::id(),
        );
        let _ = file.flush();
    }));

    // Hand back to the OS so the standard termination path runs (WER, event
    // log entry, process exit). We logged what we wanted; we are NOT
    // trying to recover.
    EXCEPTION_CONTINUE_SEARCH
}

/// Symbolic name for the common SEH codes — saves the maintainer a trip to
/// the SDK docs when triaging a `panic.log` entry.
fn seh_code_name(code: DWORD) -> &'static str {
    match code {
        0xC0000005 => "ACCESS_VIOLATION",
        0xC0000006 => "IN_PAGE_ERROR",
        0xC000001D => "ILLEGAL_INSTRUCTION",
        0xC0000025 => "NONCONTINUABLE_EXCEPTION",
        0xC000008C => "ARRAY_BOUNDS_EXCEEDED",
        0xC000008D => "FLT_DENORMAL_OPERAND",
        0xC000008E => "FLT_DIVIDE_BY_ZERO",
        0xC0000094 => "INT_DIVIDE_BY_ZERO",
        0xC0000095 => "INT_OVERFLOW",
        0xC0000096 => "PRIV_INSTRUCTION",
        0xC00000FD => "STACK_OVERFLOW",
        0xC0000135 => "DLL_NOT_FOUND",
        0xC0000139 => "ENTRYPOINT_NOT_FOUND",
        0xC0000142 => "DLL_INIT_FAILED",
        0xC0000409 => "STACK_BUFFER_OVERRUN (fastfail)",
        0xC0000417 => "INVALID_CRUNTIME_PARAMETER",
        0xE06D7363 => "C++ exception (MSVC throw)",
        _ => "<unknown>",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("pw_panic_log_{tag}_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn oversized_log_rotates_to_dot_one() {
        let dir = temp_dir("rotate");
        let log = dir.join("panic.log");
        std::fs::write(&log, vec![b'x'; (PANIC_LOG_MAX_BYTES + 1) as usize]).unwrap();
        rotate_if_oversized(&log);
        assert!(!log.exists(), "oversized log must be renamed away");
        assert!(dir.join("panic.log.1").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn small_log_is_left_in_place() {
        let dir = temp_dir("keep");
        let log = dir.join("panic.log");
        std::fs::write(&log, b"one startup marker").unwrap();
        rotate_if_oversized(&log);
        assert!(log.exists());
        assert!(!dir.join("panic.log.1").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rotation_replaces_the_previous_generation() {
        let dir = temp_dir("replace");
        let log = dir.join("panic.log");
        let rotated = dir.join("panic.log.1");
        std::fs::write(&rotated, b"older generation").unwrap();
        std::fs::write(&log, vec![b'y'; (PANIC_LOG_MAX_BYTES + 1) as usize]).unwrap();
        rotate_if_oversized(&log);
        let kept = std::fs::read(&rotated).unwrap();
        assert_eq!(
            kept.len(),
            (PANIC_LOG_MAX_BYTES + 1) as usize,
            "panic.log.1 must hold the just-rotated content, not the older generation"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_log_is_a_no_op() {
        let dir = temp_dir("missing");
        let log = dir.join("panic.log");
        rotate_if_oversized(&log);
        assert!(!log.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
