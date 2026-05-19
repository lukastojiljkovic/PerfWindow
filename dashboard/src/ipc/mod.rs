#[allow(unused_imports)]
pub use process::{SensorState, Sensord, SharedState};
#[allow(unused_imports)]
pub use snapshot::{
    parse_snapshot, BoardInfo, CpuInfo, FanInfo, GpuInfo, NetInfo, RamInfo, Snapshot, StorageInfo,
    VoltageInfo,
};

pub mod process;
pub mod snapshot;
