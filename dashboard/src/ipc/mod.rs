#[allow(unused_imports)]
pub use pipe::{ConnectError, PipeSensord};
#[allow(unused_imports)]
pub use process::{SensorState, Sensord, SharedState};
#[allow(unused_imports)]
pub use snapshot::{
    parse_line, parse_snapshot, BatteryInfo, BoardInfo, CpuInfo, FanInfo, GpuInfo, HealthInfo,
    Line, NetInfo, ProgressInfo, RamInfo, RamModule, Snapshot, StorageInfo, VoltageInfo, WifiInfo,
};

pub mod connect;
pub mod pipe;
pub mod process;
pub mod snapshot;

/// One queued message for a sensord control channel (pipe writer half or the
/// dev-mode child's stdin).
pub(crate) enum ControlMsg {
    SetInterval(u32),
    Shutdown,
}

/// Spawn the control-channel writer thread shared by both transports.
///
/// Control writes are blocking pipe/stdin I/O, so they live on this thread
/// instead of the UI thread — a wedged sensord must never freeze a settings
/// click. On a write failure the thread flips `alive` (so the next `ingest`
/// pass surfaces `SensordDown`) and exits, dropping `sink`; the same drop
/// happens after a `Shutdown` message or when the sender side closes the
/// channel, which is what signals EOF to the peer.
pub(crate) fn spawn_control_writer<W: std::io::Write + Send + 'static>(
    mut sink: W,
    rx: std::sync::mpsc::Receiver<ControlMsg>,
    state: SharedState,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        while let Ok(msg) = rx.recv() {
            let (payload, last) = match msg {
                ControlMsg::SetInterval(ms) => (format!("{{\"interval_ms\":{ms}}}"), false),
                ControlMsg::Shutdown => ("{\"shutdown\":true}".to_string(), true),
            };
            if let Err(e) = writeln!(sink, "{payload}") {
                eprintln!("PerfWindow: control write failed: {e}");
                if let Ok(mut s) = state.lock() {
                    s.alive = false;
                }
                return;
            }
            if last {
                return;
            }
        }
    })
}

/// Either a pipe-connected client (production) or a spawned child (dev).
pub enum SensordKind {
    Pipe(pipe::PipeSensord),
    Child(process::Sensord),
}

impl SensordKind {
    pub fn state(&self) -> &SharedState {
        match self {
            Self::Pipe(p) => &p.state,
            Self::Child(c) => &c.state,
        }
    }
    pub fn set_interval(&mut self, ms: u32) {
        match self {
            Self::Pipe(p) => p.set_interval(ms),
            Self::Child(c) => c.set_interval(ms),
        }
    }
    pub fn is_alive(&self) -> bool {
        match self {
            Self::Pipe(p) => p.is_alive(),
            Self::Child(c) => c.is_alive(),
        }
    }
    pub fn shutdown(&mut self) {
        match self {
            Self::Pipe(p) => p.shutdown(),
            Self::Child(c) => c.shutdown(),
        }
    }
}
