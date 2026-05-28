#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(dead_code)]

use perfwindow::app::PerfApp;
use perfwindow::config::Config;

fn main() -> eframe::Result {
    // Capture any panic to `%APPDATA%\PerfWindow\panic.log` with a backtrace.
    // Release builds run with `windows_subsystem = "windows"` so stderr goes
    // nowhere and the OS surfaces only an opaque STATUS_STACK_BUFFER_OVERRUN
    // (0xc0000409) in Event Viewer — without this hook a panic is effectively
    // un-diagnosable in the field.
    install_panic_log();

    // `--dev` swaps the production named-pipe client for the legacy
    // child-spawn path (no installed Windows service required) — useful for
    // `cargo run` against a freshly built sensord. Default = pipe.
    let dev_mode = std::env::args().any(|a| a == "--dev");

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

    let Some(path) = panic_log_path() else { return };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    else {
        return;
    };

    let payload = info
        .payload()
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| {
            info.payload()
                .downcast_ref::<String>()
                .map(String::as_str)
        })
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
