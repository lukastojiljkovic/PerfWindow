//! Shared test fixtures.
//!
//! Exposed to the crate's own unit tests and to integration tests via the
//! `test-support` feature (which the crate's `[dev-dependencies]`
//! self-enables), so production builds never compile this module.

use crate::ipc::snapshot::{D3DEngineLoad, DimmTemp};
use crate::ipc::{
    BatteryInfo, BoardInfo, CpuInfo, FanInfo, GpuInfo, NetInfo, RamInfo, RamModule, Snapshot,
    StorageInfo, VoltageInfo,
};

/// A snapshot exercising the widest, tallest reading every panel can render:
/// a discrete GPU with 24 GB of VRAM, an iGPU, a 24-thread CPU, 64 GB of RAM,
/// gigabit-class network throughput, a battery, four disks, and a board with
/// four fans and four voltage rails.
///
/// Every string is a realistic worst case, so the geometry tests can assert
/// that no card clips, overlaps or elides anything at any window size. Kept as
/// a plain function (not a `const`) because the sensor types are not `const`-
/// constructible; callers that need mutation can clone the result.
pub fn worst_case_snapshot() -> Snapshot {
    Snapshot {
        v: 1,
        ts: 1_747_645_200,
        ts_ms: Some(1_747_645_200_123),
        cpu: Some(cpu()),
        gpu: Some(vec![discrete_gpu()]),
        igpu: Some(integrated_gpu()),
        ram: Some(ram()),
        storage: Some(storage()),
        board: Some(board()),
        fans: Some(fans()),
        voltages: Some(voltages()),
        net: Some(net()),
        battery: Some(battery()),
        uptime_sec: Some(3 * 86_400 + 14 * 3_600),
        atk_fans: None,
        health: None,
    }
}

fn cpu() -> CpuInfo {
    let cores: Vec<f64> = (0..24).map(|i| ((i * 17) % 100) as f64 + 0.5).collect();
    let core_temps: Vec<Option<f64>> = (0..24)
        .map(|i| Some(58.0 + ((i * 3) % 38) as f64))
        .collect();
    let core_clocks: Vec<Option<f64>> = (0..24)
        .map(|i| Some(5187.0 - ((i % 8) as f64) * 300.0))
        .collect();
    CpuInfo {
        name: "13th Gen Intel(R) Core(TM) i9-13900HX".into(),
        load: Some(87.5),
        cores: Some(cores),
        temp: Some(96.0),
        clock_mhz: Some(5187.0),
        power_w: Some(115.4),
        core_temps: Some(core_temps),
        voltage_v: Some(1.312),
        distance_to_tjmax_c: Some(4.0),
        power_cores_w: Some(78.5),
        power_memory_w: Some(18.2),
        power_platform_w: Some(12.4),
        p_core_count: Some(8),
        e_core_count: Some(16),
        bus_clock_mhz: Some(99.8),
        core_clocks_mhz: Some(core_clocks),
    }
}

fn gpu_shell(name: &str, kind: &str) -> GpuInfo {
    GpuInfo {
        name: name.into(),
        kind: kind.into(),
        load: None,
        temp: None,
        vram_used_mb: None,
        vram_total_mb: None,
        clock_mhz: None,
        fan_rpm: None,
        power_w: None,
        memory_load: None,
        hot_spot_temp: None,
        memory_junction_temp_c: None,
        pcie_rx_bps: None,
        pcie_tx_bps: None,
        dedicated_vram_used_mb: None,
        shared_vram_used_mb: None,
        voltage_v: None,
        d3d_engines: None,
        memory_clock_mhz: None,
        video_engine_load: None,
    }
}

/// The worst-case discrete GPU: 23.9 / 24.0 GB of VRAM, every sensor present,
/// and an engine breakdown so the LOAD donut carries its hover detail.
fn discrete_gpu() -> GpuInfo {
    GpuInfo {
        load: Some(99.0),
        temp: Some(87.0),
        vram_used_mb: Some(23.9 * 1024.0),
        vram_total_mb: Some(24.0 * 1024.0),
        clock_mhz: Some(2610.0),
        fan_rpm: Some(4200.0),
        power_w: Some(175.0),
        memory_load: Some(98.0),
        hot_spot_temp: Some(95.0),
        memory_junction_temp_c: Some(102.0),
        pcie_rx_bps: Some(1023.9 * 1024.0 * 1024.0),
        pcie_tx_bps: Some(1023.9 * 1024.0 * 1024.0),
        dedicated_vram_used_mb: Some(23.9 * 1024.0),
        shared_vram_used_mb: Some(4096.0),
        voltage_v: Some(1.050),
        d3d_engines: Some(vec![
            D3DEngineLoad {
                name: "D3D 3D".into(),
                load: 99.0,
            },
            D3DEngineLoad {
                name: "D3D Copy".into(),
                load: 42.0,
            },
        ]),
        memory_clock_mhz: Some(10501.0),
        video_engine_load: Some(42.0),
        ..gpu_shell("NVIDIA GeForce RTX 4090 Laptop GPU", "discrete")
    }
}

/// The integrated GPU rides alongside the discrete one with shared memory
/// only; the iGPU card renders the `MEM USE` families without dedicated VRAM.
fn integrated_gpu() -> GpuInfo {
    GpuInfo {
        load: Some(12.0),
        temp: Some(58.0),
        clock_mhz: Some(1650.0),
        memory_load: Some(24.0),
        shared_vram_used_mb: Some(2048.0),
        voltage_v: Some(0.900),
        ..gpu_shell("Intel(R) UHD Graphics", "integrated")
    }
}

fn ram() -> RamInfo {
    RamInfo {
        used_mb: Some(63.8 * 1024.0),
        total_mb: Some(64.0 * 1024.0),
        available_mb: Some(204.8),
        load: Some(99.7),
        cached_mb: Some(4096.0),
        pagefile_used_mb: Some(8192.0),
        pagefile_total_mb: Some(16384.0),
        dimm_temps: Some(vec![
            DimmTemp {
                label: "DIMM #0".into(),
                temp_c: 61.5,
            },
            DimmTemp {
                label: "DIMM #1".into(),
                temp_c: 63.0,
            },
        ]),
        modules: Some(vec![
            RamModule {
                label: "Kingston KF556S40 #0".into(),
                capacity_gb: Some(32.0),
                temp_c: Some(61.5),
                timings: Some("CL40-39-39 @ 5600 MT/s".into()),
            },
            RamModule {
                label: "Kingston KF556S40 #1".into(),
                capacity_gb: Some(32.0),
                temp_c: Some(63.0),
                timings: Some("CL40-39-39 @ 5600 MT/s".into()),
            },
        ]),
    }
}

fn storage() -> Vec<StorageInfo> {
    let names = [
        ("Samsung SSD 990 PRO 2TB", "nvme", 2.0),
        ("WD_BLACK SN850X 4000GB", "nvme", 4.0),
        ("Crucial MX500 2TB", "ssd", 2.0),
        ("Seagate Barracuda 8TB", "hdd", 8.0),
    ];
    names
        .iter()
        .enumerate()
        .map(|(i, (name, kind, tb))| StorageInfo {
            name: (*name).into(),
            kind: (*kind).into(),
            temp: Some(48.0 + i as f64),
            activity: Some(82.0),
            used_gb: Some(tb * 1000.0 * 0.72),
            total_gb: Some(tb * 1000.0),
            health: Some(99.0),
            read_bps: Some(1023.9 * 1024.0 * 1024.0),
            write_bps: Some(1023.9 * 1024.0 * 1024.0),
            power_on_hours: Some(5057),
            power_on_count: Some(412),
            available_spare_pct: Some(100.0),
            percentage_used_pct: Some(3.4),
            temp_warn_c: Some(82.0),
            temp_crit_c: Some(85.0),
            data_read_gb: Some(20_480.0),
            data_written_gb: Some(20_480.0),
        })
        .collect()
}

fn board() -> BoardInfo {
    BoardInfo {
        temp: Some(42.0),
        vrm_temp: Some(78.0),
        name: Some("ASUS ROG Strix X670E-E GAMING WIFI".into()),
        bios_version: Some("2402".into()),
        bios_date: Some("2024-01-11".into()),
    }
}

fn fans() -> Vec<FanInfo> {
    [
        "CPU Fan",
        "Chassis Fan #1",
        "Chassis Fan #2",
        "AIO Pump Fan",
    ]
    .iter()
    .enumerate()
    .map(|(i, name)| FanInfo {
        name: (*name).into(),
        rpm: Some(1_200.0 + i as f64 * 137.0),
    })
    .collect()
}

fn voltages() -> Vec<VoltageInfo> {
    [
        ("+12V", 12.08),
        ("+5V", 5.02),
        ("+3.3V", 3.31),
        ("VCCSA", 1.21),
    ]
    .iter()
    .map(|(name, volts)| VoltageInfo {
        name: (*name).into(),
        volts: Some(*volts),
    })
    .collect()
}

fn net() -> NetInfo {
    NetInfo {
        adapter: "Ethernet".into(),
        down_bps: Some(1023.9 * 1024.0 * 1024.0),
        up_bps: Some(1023.9 * 1024.0 * 1024.0),
        link_bps: Some(10_000_000_000),
        down_pct: Some(99.9),
        up_pct: Some(42.0),
        wifi: None,
    }
}

fn battery() -> BatteryInfo {
    BatteryInfo {
        charge_pct: Some(99.0),
        rate_w: Some(-45.2),
        voltage_v: Some(15.2),
        design_capacity_mwh: Some(90_000.0),
        full_capacity_mwh: Some(82_000.0),
        time_remaining_sec: Some(2 * 3_600 + 14 * 60),
    }
}
