pub use process::*;
#[allow(unused_imports)]
pub use snapshot::{
    parse_snapshot, BoardInfo, CpuInfo, FanInfo, GpuInfo, NetInfo, RamInfo, Snapshot, StorageInfo,
    VoltageInfo,
};

pub mod process;
pub mod snapshot;
