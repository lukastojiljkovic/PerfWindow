//! `perfwindow-cli`: every reading the window shows, in the terminal. It
//! connects to the sensor service exactly as the window does, so the same UAC
//! prompt applies, and waits until every sensor category has loaded. In an
//! interactive terminal it then opens a live dashboard; otherwise, or with
//! `--once`, `--json` or `--watch`, it prints a report.
//!
//! Exit codes: 0 success, 2 bad arguments, 3 command-line readings off in
//! Settings, 4 sensor service unavailable or stopped.

pub mod args;
#[cfg(test)]
mod fixtures;
mod live;
pub mod path;
pub mod report;
mod term;

use crate::config::Config;
use crate::format::TempUnit;
use crate::ipc::connect::{
    spawn_connect, ConnectEvent, ConnectPhase, FailedReason, START_DEADLINE_S,
};
use crate::ipc::{PipeSensord, ProgressInfo, SensorState, Snapshot};
use args::{Command, Format, Options};
use std::io::Write;
use std::process::ExitCode;
use std::sync::PoisonError;
use std::time::{Duration, Instant};
use term::Status;

/// How often the shared sensor state is looked at while waiting.
const POLL: Duration = Duration::from_millis(100);
/// How long the sensors may take to finish loading before the run gives up.
const LOAD_TIMEOUT: Duration = Duration::from_secs(90);

/// Entry point of the `perfwindow-cli` binary.
pub fn run() -> ExitCode {
    let options = match args::parse(std::env::args().skip(1)) {
        Ok(Command::Help) => {
            print!("{}", args::USAGE);
            return ExitCode::SUCCESS;
        }
        Ok(Command::Version) => {
            println!("perfwindow-cli {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        Ok(Command::Run(options)) => options,
        Err(message) => {
            eprintln!("perfwindow-cli: {message}\nRun perfwindow-cli --help for the options.");
            return ExitCode::from(2);
        }
    };

    let config = Config::load();
    if !config.cli_enabled {
        eprintln!(
            "Command-line readings are off. Turn them on in PerfWindow -> Settings -> Command line."
        );
        return ExitCode::from(3);
    }

    let dashboard = opens_dashboard(options, term::interactive());
    let mut status = Status::new();
    let Some(mut pipe) = connect(&mut status) else {
        return ExitCode::from(4);
    };
    pipe.set_interval(match options.watch {
        Some(seconds) => seconds * 1000,
        None if dashboard => config.refresh.as_millis(),
        None => 1000,
    });
    let result = if dashboard {
        dashboard_loop(&pipe, &mut status, options, &config)
    } else {
        report_loop(&pipe, &mut status, options, config.unit)
    };
    pipe.shutdown();
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            status.finish();
            eprintln!("perfwindow-cli: {message}");
            ExitCode::from(4)
        }
    }
}

/// Whether a run opens the live dashboard: a plain run, without `--once`,
/// `--json` or `--watch`, on an interactive terminal. Anything else prints
/// the report, so pipes, scripts and agents never meet a full-screen view.
fn opens_dashboard(options: Options, interactive: bool) -> bool {
    interactive && !options.once && options.watch.is_none() && options.format == Format::Text
}

/// Open the sensor pipe through the window's own connect path, keeping the
/// status line on what it is doing and reporting the elevation step and any
/// failure on stderr. `None` when the service could not be reached.
fn connect(status: &mut Status) -> Option<PipeSensord> {
    let (_phase, events) = spawn_connect(|| {});
    for event in events {
        match event {
            ConnectEvent::Phase(ConnectPhase::OpeningPipe) => {
                status.show("Connecting to the sensor service…")
            }
            ConnectEvent::Phase(ConnectPhase::RequestingElevation) => {
                status.clear();
                eprintln!("Starting the PerfWindow sensor service: approve the Windows prompt.");
            }
            ConnectEvent::Phase(ConnectPhase::StartingService) => {
                status.show("Starting the sensor service…")
            }
            ConnectEvent::Phase(ConnectPhase::Failed(reason)) => {
                status.finish();
                eprintln!("perfwindow-cli: {}", failure_message(&reason));
                return None;
            }
            ConnectEvent::Phase(ConnectPhase::LoadingSensors) => {}
            ConnectEvent::Ready(pipe) => return Some(*pipe),
        }
    }
    None
}

/// The sentence a failed connect shows, one per reason.
fn failure_message(reason: &FailedReason) -> String {
    match reason {
        FailedReason::UacCancelled => {
            "the Windows prompt was declined, so the sensor service did not start.".to_string()
        }
        FailedReason::StartTimeout => {
            format!("the sensor service did not start within {START_DEADLINE_S} seconds.")
        }
        FailedReason::PipeError(text) if *text == crate::ipc::ConnectError::Busy.to_string() => {
            "PerfWindow's window is reading the sensors; close it and run the command again."
                .to_string()
        }
        FailedReason::PipeError(text) => format!("the sensor service is unavailable: {text}."),
    }
}

/// Print the report: once, from a sample taken after the first complete one
/// so rates computed between two samples have settled, or with `--watch`
/// every new complete sample until the service stops.
fn report_loop(
    pipe: &PipeSensord,
    status: &mut Status,
    options: Options,
    unit: TempUnit,
) -> Result<(), String> {
    let style = term::report_style(options.no_color);
    let started = Instant::now();
    let mut sample = next_sample(pipe, None, started, status)?;
    if options.watch.is_none() {
        sample = next_sample(pipe, Some(&sample), started, status)?;
    }
    status.finish();
    let mut first = true;
    loop {
        print_report(&sample, options, unit, style, first)
            .map_err(|e| format!("could not write the report: {e}"))?;
        if options.watch.is_none() {
            return Ok(());
        }
        first = false;
        sample = next_sample(pipe, Some(&sample), started, status)?;
    }
}

/// Open the live dashboard on the first complete sample and keep it until
/// the user leaves or the service stops.
fn dashboard_loop(
    pipe: &PipeSensord,
    status: &mut Status,
    options: Options,
    config: &Config,
) -> Result<(), String> {
    let first = next_sample(pipe, None, Instant::now(), status)?;
    status.finish();
    live::run(pipe, first, config, term::color_allowed(options.no_color))
}

/// Wait for the next complete sample other than `last`, keeping the status
/// line current. `Err` when the service stops, or when the sensors are still
/// loading [`LOAD_TIMEOUT`] after `started`.
fn next_sample(
    pipe: &PipeSensord,
    last: Option<&Snapshot>,
    started: Instant,
    status: &mut Status,
) -> Result<Snapshot, String> {
    loop {
        std::thread::sleep(POLL);
        let poll = poll(pipe, last);
        if !poll.alive {
            return Err(SERVICE_STOPPED.to_string());
        }
        if !poll.complete && started.elapsed() > LOAD_TIMEOUT {
            return Err("the sensors did not finish loading within 90 seconds.".to_string());
        }
        if let Some(snapshot) = poll.fresh {
            return Ok(snapshot);
        }
        status.show(&poll.status);
    }
}

/// The error a run ends with when the sensor service goes away under it.
const SERVICE_STOPPED: &str = "the sensor service stopped before it finished.";

/// What one look at the shared sensor state found.
struct Poll {
    alive: bool,
    complete: bool,
    /// A complete sample different from the one `last` names.
    fresh: Option<Snapshot>,
    /// The status line for what the service is doing.
    status: String,
}

fn poll(pipe: &PipeSensord, last: Option<&Snapshot>) -> Poll {
    let state = pipe.state.lock().unwrap_or_else(PoisonError::into_inner);
    let complete = is_complete(&state);
    let fresh = state
        .latest
        .as_ref()
        .filter(|snapshot| complete && is_new_sample(last, snapshot))
        .cloned();
    Poll {
        alive: state.alive,
        complete,
        fresh,
        status: loading_status(state.progress.as_ref(), complete),
    }
}

/// The status line while the first report is not ready: the category the
/// service is loading and how many are done, then the wait for a sample.
fn loading_status(progress: Option<&ProgressInfo>, complete: bool) -> String {
    if complete {
        return "Reading the sensors…".to_string();
    }
    let Some(progress) = progress else {
        return "Loading sensors…".to_string();
    };
    let done = progress.done.len();
    let total = done + progress.pending.len() + usize::from(progress.loading.is_some());
    match &progress.loading {
        Some(name) => format!(
            "Loading sensors: {} ({done} of {total} done)",
            report::clean(name)
        ),
        None => format!("Loading sensors ({done} of {total} done)"),
    }
}

/// Every sensor category has loaded: the newest snapshot was not taken
/// mid-load, and the progress line, if any, lists nothing still loading.
fn is_complete(state: &SensorState) -> bool {
    state.latest.is_some()
        && !state.latest_partial
        && state
            .progress
            .as_ref()
            .is_none_or(|p| p.loading.is_none() && p.pending.is_empty())
}

/// Whether `candidate` is a different sample from `last`. Compared by `ts_ms`
/// when both carry it, otherwise by the seconds-resolution `ts`.
fn is_new_sample(last: Option<&Snapshot>, candidate: &Snapshot) -> bool {
    match last {
        None => true,
        Some(last) => match (last.ts_ms, candidate.ts_ms) {
            (Some(a), Some(b)) => a != b,
            _ => last.ts != candidate.ts,
        },
    }
}

/// Write one report. With `--watch`, text reports after the first are
/// separated by a blank line, and JSON is one compact object per line.
fn print_report(
    snapshot: &Snapshot,
    options: Options,
    unit: TempUnit,
    style: report::Style,
    first: bool,
) -> std::io::Result<()> {
    let displays = crate::displays::enumerate();
    let body = match options.format {
        Format::Text => report::text(snapshot, &displays, unit, style),
        Format::Json => report::json_text(snapshot, &displays, options.watch.is_none()),
    };
    let mut out = std::io::stdout().lock();
    if options.watch.is_some() && options.format == Format::Text && !first {
        writeln!(out)?;
    }
    writeln!(out, "{body}")?;
    out.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use fixtures::snapshot;

    fn state(latest: Option<Snapshot>, partial: bool, progress: Option<&str>) -> SensorState {
        SensorState {
            latest,
            alive: true,
            progress: progress.map(progress_info),
            last_line_at: None,
            latest_partial: partial,
        }
    }

    fn progress_info(line: &str) -> ProgressInfo {
        match crate::ipc::parse_line(line).expect("progress line") {
            crate::ipc::Line::Progress(p) => p,
            crate::ipc::Line::Snap(_) => panic!("expected progress"),
        }
    }

    const SAMPLE: &str = r#"{"v":1,"ts":100,"ts_ms":100500}"#;

    #[test]
    fn nothing_is_complete_before_a_snapshot_arrives() {
        assert!(!is_complete(&state(None, false, None)));
    }

    #[test]
    fn a_snapshot_taken_mid_load_is_not_complete() {
        let s = state(Some(snapshot(SAMPLE)), true, None);
        assert!(!is_complete(&s));
    }

    #[test]
    fn progress_still_loading_or_pending_is_not_complete() {
        let loading = r#"{"progress":{"loading":"gpu","done":["cpu"],"pending":[]}}"#;
        let pending = r#"{"progress":{"loading":null,"done":["cpu"],"pending":["battery"]}}"#;
        assert!(!is_complete(&state(
            Some(snapshot(SAMPLE)),
            false,
            Some(loading)
        )));
        assert!(!is_complete(&state(
            Some(snapshot(SAMPLE)),
            false,
            Some(pending)
        )));
    }

    #[test]
    fn a_snapshot_with_finished_or_no_progress_is_complete() {
        let done = r#"{"progress":{"loading":null,"done":["cpu","gpu"],"pending":[]}}"#;
        assert!(is_complete(&state(Some(snapshot(SAMPLE)), false, None)));
        assert!(is_complete(&state(
            Some(snapshot(SAMPLE)),
            false,
            Some(done)
        )));
    }

    #[test]
    fn a_new_sample_is_one_with_a_different_ts_ms() {
        let first = snapshot(SAMPLE);
        let same = snapshot(SAMPLE);
        let next = snapshot(r#"{"v":1,"ts":101,"ts_ms":101500}"#);
        assert!(is_new_sample(None, &first));
        assert!(!is_new_sample(Some(&first), &same));
        assert!(is_new_sample(Some(&first), &next));
    }

    #[test]
    fn without_ts_ms_the_seconds_stamp_decides() {
        let first = snapshot(r#"{"v":1,"ts":100}"#);
        let same_second = snapshot(r#"{"v":1,"ts":100}"#);
        let next_second = snapshot(r#"{"v":1,"ts":101}"#);
        assert!(!is_new_sample(Some(&first), &same_second));
        assert!(is_new_sample(Some(&first), &next_second));
    }

    #[test]
    fn a_failed_pipe_that_is_busy_names_the_open_window() {
        let busy = FailedReason::PipeError(crate::ipc::ConnectError::Busy.to_string());
        assert!(failure_message(&busy).contains("close it and run the command again"));
    }

    #[test]
    fn each_failure_reason_has_its_own_sentence() {
        assert!(failure_message(&FailedReason::UacCancelled).contains("declined"));
        assert!(failure_message(&FailedReason::StartTimeout).contains("30 seconds"));
    }

    #[test]
    fn the_status_names_the_category_loading_and_the_count_done() {
        let loading = progress_info(
            r#"{"progress":{"loading":"gpu","done":["cpu","ram"],"pending":["storage","battery"]}}"#,
        );
        assert_eq!(
            loading_status(Some(&loading), false),
            "Loading sensors: gpu (2 of 5 done)"
        );
        let between =
            progress_info(r#"{"progress":{"loading":null,"done":["cpu"],"pending":["net"]}}"#);
        assert_eq!(
            loading_status(Some(&between), false),
            "Loading sensors (1 of 2 done)"
        );
        assert_eq!(loading_status(None, false), "Loading sensors…");
        assert_eq!(loading_status(None, true), "Reading the sensors…");
    }

    #[test]
    fn only_a_plain_run_on_an_interactive_terminal_opens_the_dashboard() {
        let plain = args::parse(Vec::<String>::new()).unwrap();
        let Command::Run(plain) = plain else {
            panic!("a run")
        };
        assert!(opens_dashboard(plain, true));
        assert!(!opens_dashboard(plain, false));
        for flags in [
            &["--once"][..],
            &["--json"],
            &["--watch"],
            &["--no-color", "--once"],
        ] {
            let Ok(Command::Run(options)) = args::parse(flags.iter().map(|s| s.to_string())) else {
                panic!("a run")
            };
            assert!(!opens_dashboard(options, true), "{flags:?}");
        }
    }
}
