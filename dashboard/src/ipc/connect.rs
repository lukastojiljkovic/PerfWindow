//! Background state machine that opens the sensor pipe, elevating once if
//! the service is not yet running. Communicates phase changes and the final
//! result to the UI thread via an mpsc channel.

use crate::ipc::pipe::{elevate_and_start_service, ConnectError, PipeSensord};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectPhase {
    OpeningPipe,
    RequestingElevation,
    StartingService,
    LoadingSensors,
    Failed(FailedReason),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FailedReason {
    UacCancelled,
    StartTimeout,
    PipeError(String),
}

pub enum ConnectEvent {
    Phase(ConnectPhase),
    Ready(Box<PipeSensord>),
}

impl std::fmt::Debug for ConnectEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectEvent::Phase(p) => f.debug_tuple("Phase").field(p).finish(),
            // PipeSensord owns file handles + a JoinHandle; print a marker
            // instead of trying to render those fields.
            ConnectEvent::Ready(_) => f.debug_tuple("Ready").field(&"<PipeSensord>").finish(),
        }
    }
}

pub type SharedPhase = Arc<Mutex<ConnectPhase>>;

const POLL_INTERVAL_MS: u64 = 250;
/// How long to poll for the pipe after the elevated `sc start`. Generous on
/// purpose: a cold .NET service on a slow disk can take well over 15 s to
/// reach `WaitForConnectionAsync`. Public so the failure copy on the loading
/// screen can quote the real value.
pub const START_DEADLINE_S: u64 = 30;
/// How long phase 1 retries a busy pipe. sensord keeps its single instance
/// for up to 3.5 s after a client leaves, while it ends that session, so a
/// RESPAWN or a CLI run right after another one must not read as another
/// window being connected.
const BUSY_RETRY_MS: u64 = 4000;

/// Spawn the connect state machine on a worker thread. Returns the shared
/// phase handle (for UI polling) plus an mpsc receiver carrying every
/// event (phase change and `Ready`). `repaint` wakes egui on each phase
/// transition so the loading screen re-renders without waiting for the
/// next refresh tick.
#[allow(dead_code)]
pub fn spawn_connect(
    repaint: impl Fn() + Send + Sync + 'static,
) -> (SharedPhase, Receiver<ConnectEvent>) {
    let phase: SharedPhase = Arc::new(Mutex::new(ConnectPhase::OpeningPipe));
    let (tx, rx) = mpsc::channel();
    let phase_th = Arc::clone(&phase);
    // Wrap in Arc<dyn Fn> so the closure can be cheaply cloned into the
    // worker thread + into every PipeSensord::connect_no_elevation call.
    let repaint: Arc<dyn Fn() + Send + Sync> = Arc::new(repaint);
    let repaint_th = Arc::clone(&repaint);
    std::thread::spawn(move || run(phase_th, tx, repaint_th));
    (phase, rx)
}

fn run(phase: SharedPhase, tx: Sender<ConnectEvent>, repaint: Arc<dyn Fn() + Send + Sync>) {
    // Phase 1: try opening the pipe directly. Covers the case where the
    // service is already running from a previous session.
    set_phase(&phase, &tx, &repaint, ConnectPhase::OpeningPipe);
    match open_waiting_out_busy(
        || PipeSensord::connect_no_elevation(clone_repaint(&repaint)),
        std::thread::sleep,
    ) {
        Ok(p) => {
            emit_ready(&phase, &tx, &repaint, p);
            return;
        }
        Err(e) => {
            if let Some(reason) = unrecoverable_open_failure(&e) {
                set_phase(&phase, &tx, &repaint, ConnectPhase::Failed(reason));
                return;
            }
            // NotFound / transient I/O: the service is probably not running —
            // fall through to the elevated start below.
        }
    }

    // Phase 2: request elevation. This blocks on the UAC dialog while it
    // is on screen.
    set_phase(&phase, &tx, &repaint, ConnectPhase::RequestingElevation);
    if elevate_and_start_service().is_err() {
        set_phase(
            &phase,
            &tx,
            &repaint,
            ConnectPhase::Failed(FailedReason::UacCancelled),
        );
        return;
    }

    // Phase 3: poll the pipe for up to START_DEADLINE_S.
    set_phase(&phase, &tx, &repaint, ConnectPhase::StartingService);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(START_DEADLINE_S);
    loop {
        std::thread::sleep(std::time::Duration::from_millis(POLL_INTERVAL_MS));
        match PipeSensord::connect_no_elevation(clone_repaint(&repaint)) {
            Ok(p) => {
                emit_ready(&phase, &tx, &repaint, p);
                return;
            }
            Err(ConnectError::NotFound) if std::time::Instant::now() < deadline => continue,
            Err(ConnectError::NotFound) => {
                set_phase(
                    &phase,
                    &tx,
                    &repaint,
                    ConnectPhase::Failed(FailedReason::StartTimeout),
                );
                return;
            }
            Err(e) => {
                set_phase(
                    &phase,
                    &tx,
                    &repaint,
                    ConnectPhase::Failed(FailedReason::PipeError(e.to_string())),
                );
                return;
            }
        }
    }
}

/// Run `open` until it reports anything but [`ConnectError::Busy`], waiting
/// [`POLL_INTERVAL_MS`] between tries for up to [`BUSY_RETRY_MS`]. `open` and
/// `sleep` are passed in so the policy is testable without a pipe.
fn open_waiting_out_busy<T>(
    mut open: impl FnMut() -> Result<T, ConnectError>,
    mut sleep: impl FnMut(Duration),
) -> Result<T, ConnectError> {
    for _ in 0..BUSY_RETRY_MS / POLL_INTERVAL_MS {
        match open() {
            Err(ConnectError::Busy) => sleep(Duration::from_millis(POLL_INTERVAL_MS)),
            result => return result,
        }
    }
    open()
}

/// Open errors the elevated `sc start` step can never fix: another client
/// already holds the single pipe instance, or the ACL refused us. Failing
/// without elevation avoids showing a pointless UAC prompt for those.
fn unrecoverable_open_failure(e: &ConnectError) -> Option<FailedReason> {
    match e {
        ConnectError::Busy | ConnectError::AccessDenied => {
            Some(FailedReason::PipeError(e.to_string()))
        }
        ConnectError::NotFound | ConnectError::Io(_) => None,
    }
}

fn set_phase(
    phase: &SharedPhase,
    tx: &Sender<ConnectEvent>,
    repaint: &Arc<dyn Fn() + Send + Sync>,
    p: ConnectPhase,
) {
    *phase.lock().unwrap() = p.clone();
    let _ = tx.send(ConnectEvent::Phase(p));
    repaint();
}

fn emit_ready(
    phase: &SharedPhase,
    tx: &Sender<ConnectEvent>,
    repaint: &Arc<dyn Fn() + Send + Sync>,
    p: PipeSensord,
) {
    set_phase(phase, tx, repaint, ConnectPhase::LoadingSensors);
    let _ = tx.send(ConnectEvent::Ready(Box::new(p)));
    repaint();
}

/// Build a fresh `Fn() + Send + 'static` from an Arc-wrapped one.
/// Each `PipeSensord::connect_no_elevation` call needs to own its repaint
/// closure; cloning the Arc gives every call a cheap independent handle.
fn clone_repaint(repaint: &Arc<dyn Fn() + Send + Sync>) -> impl Fn() + Send + 'static {
    let r = Arc::clone(repaint);
    move || r()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_reason_variants_distinct() {
        assert_ne!(FailedReason::UacCancelled, FailedReason::StartTimeout);
        assert_ne!(
            FailedReason::PipeError("a".into()),
            FailedReason::PipeError("b".into())
        );
    }

    #[test]
    fn connect_phase_eq_is_structural() {
        assert_eq!(ConnectPhase::OpeningPipe, ConnectPhase::OpeningPipe);
        assert_ne!(ConnectPhase::OpeningPipe, ConnectPhase::StartingService);
        assert_eq!(
            ConnectPhase::Failed(FailedReason::UacCancelled),
            ConnectPhase::Failed(FailedReason::UacCancelled)
        );
    }

    #[test]
    fn busy_and_access_denied_fail_without_elevation() {
        let busy = unrecoverable_open_failure(&ConnectError::Busy);
        assert!(matches!(busy, Some(FailedReason::PipeError(_))));
        let denied = unrecoverable_open_failure(&ConnectError::AccessDenied);
        assert!(matches!(denied, Some(FailedReason::PipeError(_))));
    }

    #[test]
    fn not_found_and_io_errors_proceed_to_elevation() {
        assert!(unrecoverable_open_failure(&ConnectError::NotFound).is_none());
        let io = ConnectError::Io(std::io::Error::other("transient"));
        assert!(unrecoverable_open_failure(&io).is_none());
    }

    /// Feed `open_waiting_out_busy` a scripted sequence of open results and
    /// record the waits between them. Opens past the script stay busy.
    fn open_scripted(
        mut script: Vec<Result<(), ConnectError>>,
    ) -> (Result<(), ConnectError>, usize, Duration) {
        script.reverse();
        let mut opens = 0;
        let mut waited = Duration::ZERO;
        let result = open_waiting_out_busy(
            || {
                opens += 1;
                script.pop().unwrap_or(Err(ConnectError::Busy))
            },
            |wait| waited += wait,
        );
        (result, opens, waited)
    }

    #[test]
    fn a_pipe_busy_while_sensord_ends_its_last_session_is_retried() {
        let script = vec![
            Err(ConnectError::Busy),
            Err(ConnectError::Busy),
            Err(ConnectError::Busy),
            Ok(()),
        ];
        let (result, opens, waited) = open_scripted(script);
        assert!(result.is_ok());
        assert_eq!(opens, 4);
        assert_eq!(waited, Duration::from_millis(3 * POLL_INTERVAL_MS));
    }

    #[test]
    fn a_busy_pipe_that_outlasts_the_retry_window_reports_busy() {
        let (result, _, waited) = open_scripted(Vec::new());
        assert!(matches!(result, Err(ConnectError::Busy)));
        assert_eq!(waited, Duration::from_millis(BUSY_RETRY_MS));
    }

    #[test]
    fn a_busy_pipe_that_disappears_falls_through_to_not_found() {
        // sensord exits once its session has ended, so the instance goes away
        // and the next open reports the pipe missing.
        let script = vec![Err(ConnectError::Busy), Err(ConnectError::NotFound)];
        let (result, opens, _) = open_scripted(script);
        assert!(matches!(result, Err(ConnectError::NotFound)));
        assert_eq!(opens, 2);
    }

    #[test]
    fn other_open_errors_are_not_retried() {
        let (result, opens, waited) = open_scripted(vec![Err(ConnectError::AccessDenied)]);
        assert!(matches!(result, Err(ConnectError::AccessDenied)));
        assert_eq!(opens, 1);
        assert_eq!(waited, Duration::ZERO);
    }
}
