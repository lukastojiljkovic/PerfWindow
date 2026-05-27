#[allow(unused_imports)]
pub use pipe::{ConnectError, PipeSensord};
#[allow(unused_imports)]
pub use process::{SensorState, Sensord, SharedState};
#[allow(unused_imports)]
pub use snapshot::{
    parse_snapshot, BatteryInfo, BoardInfo, CpuInfo, FanInfo, GpuInfo, HealthInfo, NetInfo,
    RamInfo, Snapshot, StorageInfo, VoltageInfo,
};

pub mod connect;
pub mod pipe;
pub mod process;
pub mod snapshot;

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
