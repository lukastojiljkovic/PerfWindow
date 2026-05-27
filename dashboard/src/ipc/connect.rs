//! Background state machine that opens the sensor pipe, elevating once if
//! the service is not yet running. Communicates phase changes and the final
//! result to the UI thread via an mpsc channel.

use crate::ipc::pipe::{elevate_and_start_service, ConnectError, PipeSensord};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

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
const START_DEADLINE_S: u64 = 15;

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

fn run(
    phase: SharedPhase,
    tx: Sender<ConnectEvent>,
    repaint: Arc<dyn Fn() + Send + Sync>,
) {
    // Phase 1: try opening the pipe directly. Covers the case where the
    // service is already running from a previous session.
    set_phase(&phase, &tx, &repaint, ConnectPhase::OpeningPipe);
    if let Ok(p) = PipeSensord::connect_no_elevation(clone_repaint(&repaint)) {
        emit_ready(&phase, &tx, &repaint, p);
        return;
    }

    // Phase 2: request elevation. This blocks on the UAC dialog while it
    // is on screen.
    set_phase(&phase, &tx, &repaint, ConnectPhase::RequestingElevation);
    if elevate_and_start_service().is_err() {
        set_phase(&phase, &tx, &repaint, ConnectPhase::Failed(FailedReason::UacCancelled));
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
                set_phase(&phase, &tx, &repaint, ConnectPhase::Failed(FailedReason::StartTimeout));
                return;
            }
            Err(e) => {
                set_phase(&phase, &tx, &repaint, ConnectPhase::Failed(FailedReason::PipeError(e.to_string())));
                return;
            }
        }
    }
}

fn set_phase(phase: &SharedPhase, tx: &Sender<ConnectEvent>, repaint: &Arc<dyn Fn() + Send + Sync>, p: ConnectPhase) {
    *phase.lock().unwrap() = p.clone();
    let _ = tx.send(ConnectEvent::Phase(p));
    repaint();
}

fn emit_ready(phase: &SharedPhase, tx: &Sender<ConnectEvent>, repaint: &Arc<dyn Fn() + Send + Sync>, p: PipeSensord) {
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
}
