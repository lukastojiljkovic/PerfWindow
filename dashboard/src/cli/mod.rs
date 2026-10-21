//! `perfwindow-cli`: a report of every reading the window shows, printed to the
//! terminal. It connects to the sensor service exactly as the window does, so
//! the same UAC prompt applies, waits until every sensor category has loaded,
//! prints the report and disconnects.
//!
//! Exit codes: 0 report printed, 2 bad arguments, 3 command-line readings off
//! in Settings, 4 sensor service unavailable.

pub mod args;
pub mod path;
pub mod report;

use crate::config::Config;
use crate::format::TempUnit;
use crate::ipc::connect::{
    spawn_connect, ConnectEvent, ConnectPhase, FailedReason, START_DEADLINE_S,
};
use crate::ipc::{PipeSensord, SensorState, Snapshot};
use args::{Command, Format, Options};
use std::io::Write;
use std::process::ExitCode;
use std::sync::PoisonError;
use std::time::{Duration, Instant};

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
            eprint!("perfwindow-cli: {message}\n\n{}", args::USAGE);
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

    let Some(mut pipe) = connect() else {
        return ExitCode::from(4);
    };
    pipe.set_interval(options.watch.map_or(1000, |seconds| seconds * 1000));
    let code = report_loop(&mut pipe, options, config.unit);
    pipe.shutdown();
    code
}

/// Open the sensor pipe through the window's own connect path, reporting the
/// elevation step and any failure on stderr. `None` when the service could not
/// be reached.
fn connect() -> Option<PipeSensord> {
    let (_phase, events) = spawn_connect(|| {});
    for event in events {
        match event {
            ConnectEvent::Phase(ConnectPhase::RequestingElevation) => {
                eprintln!("Starting the PerfWindow sensor service — approve the Windows prompt.")
            }
            ConnectEvent::Phase(ConnectPhase::Failed(reason)) => {
                eprintln!("perfwindow-cli: {}", failure_message(&reason));
                return None;
            }
            ConnectEvent::Phase(_) => {}
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

/// Wait for complete readings, then print them. A single report waits for one
/// further sample, so rates computed between two samples have settled; watch
/// mode prints every new complete sample until the service stops.
fn report_loop(pipe: &mut PipeSensord, options: Options, unit: TempUnit) -> ExitCode {
    let started = Instant::now();
    let mut last: Option<Snapshot> = None;
    loop {
        std::thread::sleep(POLL);
        let poll = poll(pipe, last.as_ref());
        if !poll.alive {
            return fail("the sensor service stopped before it finished.");
        }
        if !poll.complete && started.elapsed() > LOAD_TIMEOUT {
            return fail("the sensors did not finish loading within 90 seconds.");
        }
        let Some(snapshot) = poll.fresh else {
            continue;
        };
        if options.watch.is_none() && last.is_none() {
            // The baseline the next sample is measured against.
            last = Some(snapshot);
            continue;
        }
        if let Err(e) = print_report(&snapshot, options, unit, last.is_none()) {
            return fail(&format!("could not write the report: {e}"));
        }
        if options.watch.is_none() {
            return ExitCode::SUCCESS;
        }
        last = Some(snapshot);
    }
}

/// What one look at the shared sensor state found.
struct Poll {
    alive: bool,
    complete: bool,
    /// A complete sample different from the one `last` names.
    fresh: Option<Snapshot>,
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

/// Write one report. In watch mode, text reports after the first are separated
/// by a blank line; JSON is one compact object per line.
fn print_report(
    snapshot: &Snapshot,
    options: Options,
    unit: TempUnit,
    first: bool,
) -> std::io::Result<()> {
    let displays = crate::displays::enumerate();
    let body = match (options.format, options.watch.is_some()) {
        (Format::Text, _) => report::text(snapshot, &displays, unit),
        (Format::Json, true) => report::json(snapshot, &displays).to_string(),
        (Format::Json, false) => {
            serde_json::to_string_pretty(&report::json(snapshot, &displays)).unwrap_or_default()
        }
    };
    let mut out = std::io::stdout().lock();
    if options.watch.is_some() && options.format == Format::Text && !first {
        writeln!(out)?;
    }
    writeln!(out, "{body}")?;
    out.flush()
}

fn fail(message: &str) -> ExitCode {
    eprintln!("perfwindow-cli: {message}");
    ExitCode::from(4)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(json: &str) -> Snapshot {
        crate::ipc::parse_snapshot(json).expect("fixture parses")
    }

    fn state(latest: Option<Snapshot>, partial: bool, progress: Option<&str>) -> SensorState {
        SensorState {
            latest,
            alive: true,
            progress: progress.map(|line| {
                let parsed = crate::ipc::parse_line(line).expect("progress line");
                match parsed {
                    crate::ipc::Line::Progress(p) => p,
                    crate::ipc::Line::Snap(_) => panic!("expected progress"),
                }
            }),
            last_line_at: None,
            latest_partial: partial,
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
}
