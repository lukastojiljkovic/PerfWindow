use serde::Deserialize;

/// One NDJSON snapshot from `sensord`. Sections are absent when the hardware
/// is. Every field added after the initial 0.1.0 schema uses
/// `#[serde(default)]` so older `sensord` builds parse cleanly.
#[derive(Debug, Clone, Deserialize)]
pub struct Snapshot {
    pub v: i32,
    pub ts: i64,
    pub cpu: Option<CpuInfo>,
    pub gpu: Option<Vec<GpuInfo>>,
    /// The integrated GPU when an iGPU is enumerated alongside a discrete
    /// one. Absent on desktops with no iGPU and on machines whose only GPU
    /// is integrated (it already appears in `gpu` as the sole entry).
    #[serde(default)]
    pub igpu: Option<GpuInfo>,
    pub ram: Option<RamInfo>,
    pub storage: Option<Vec<StorageInfo>>,
    pub board: Option<BoardInfo>,
    pub fans: Option<Vec<FanInfo>>,
    pub voltages: Option<Vec<VoltageInfo>>,
    pub net: Option<NetInfo>,
    #[serde(default)]
    pub battery: Option<BatteryInfo>,
    #[serde(default)]
    pub uptime_sec: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CpuInfo {
    pub name: String,
    pub load: Option<f64>,
    pub cores: Option<Vec<f64>>,
    pub temp: Option<f64>,
    pub clock_mhz: Option<f64>,
    pub power_w: Option<f64>,
    #[serde(default)]
    pub core_temps: Option<Vec<Option<f64>>>,
    #[serde(default)]
    pub voltage_v: Option<f64>,
    /// °C of headroom the hottest core has before throttling kicks in.
    /// Smaller = closer to TjMax. Populated when LHM exposes per-core
    /// "Distance to TjMax" sensors (Intel; some AMD).
    #[serde(default)]
    pub distance_to_tjmax_c: Option<f64>,
    /// RAPL sub-domain power draw, watts. Whole-package draw lives in
    /// `power_w`; the three breakdown fields are only present when the
    /// silicon exposes them.
    #[serde(default)]
    pub power_cores_w: Option<f64>,
    #[serde(default)]
    pub power_memory_w: Option<f64>,
    #[serde(default)]
    pub power_platform_w: Option<f64>,
    /// On Intel hybrid CPUs, the count of P-Cores and E-Cores so the
    /// dashboard can section the heat-map. Both `None` on non-hybrid CPUs.
    #[serde(default)]
    pub p_core_count: Option<u32>,
    #[serde(default)]
    pub e_core_count: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GpuInfo {
    pub name: String,
    pub kind: String,
    pub load: Option<f64>,
    pub temp: Option<f64>,
    pub vram_used_mb: Option<f64>,
    pub vram_total_mb: Option<f64>,
    pub clock_mhz: Option<f64>,
    pub fan_rpm: Option<f64>,
    pub power_w: Option<f64>,
    #[serde(default)]
    pub memory_load: Option<f64>,
    #[serde(default)]
    pub hot_spot_temp: Option<f64>,
    /// GDDR/VRAM die hot spot (°C). A separate physical sensor from `temp`
    /// (GPU core) and `hot_spot_temp` (GPU die hot spot).
    #[serde(default)]
    pub memory_junction_temp_c: Option<f64>,
    /// PCIe link-direction throughput in bytes per second. Useful for
    /// streaming / model-loading bus-saturation diagnosis.
    #[serde(default)]
    pub pcie_rx_bps: Option<f64>,
    #[serde(default)]
    pub pcie_tx_bps: Option<f64>,
    /// VRAM split: dedicated-on-card vs DXGI-shared system RAM, MB. The
    /// shared figure is mostly meaningful on iGPUs and laptops whose dGPU
    /// spills into RAM.
    #[serde(default)]
    pub dedicated_vram_used_mb: Option<f64>,
    #[serde(default)]
    pub shared_vram_used_mb: Option<f64>,
    /// GPU core voltage rail, volts.
    #[serde(default)]
    pub voltage_v: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RamInfo {
    pub used_mb: Option<f64>,
    pub total_mb: Option<f64>,
    pub available_mb: Option<f64>,
    pub load: Option<f64>,
    pub cached_mb: Option<f64>,
    pub pagefile_used_mb: Option<f64>,
    pub pagefile_total_mb: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StorageInfo {
    pub name: String,
    pub kind: String,
    pub temp: Option<f64>,
    pub activity: Option<f64>,
    pub used_gb: Option<f64>,
    pub total_gb: Option<f64>,
    #[serde(default)]
    pub health: Option<f64>,
    #[serde(default)]
    pub read_bps: Option<f64>,
    #[serde(default)]
    pub write_bps: Option<f64>,
    /// Lifetime metrics — exposed on most NVMe drives and many SATA SSDs.
    /// `power_on_hours` and `power_on_count` come from S.M.A.R.T. attributes;
    /// `available_spare_pct` is the NVMe-spec reserved-blocks reading (0–100;
    /// distinct from the merged `health` figure above).
    #[serde(default)]
    pub power_on_hours: Option<i64>,
    #[serde(default)]
    pub power_on_count: Option<i64>,
    #[serde(default)]
    pub available_spare_pct: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BatteryInfo {
    pub charge_pct: Option<f64>,
    /// Signed rate: positive when charging, negative when discharging, in watts.
    pub rate_w: Option<f64>,
    pub voltage_v: Option<f64>,
    pub design_capacity_mwh: Option<f64>,
    pub full_capacity_mwh: Option<f64>,
    pub time_remaining_sec: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BoardInfo {
    pub temp: Option<f64>,
    pub vrm_temp: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FanInfo {
    pub name: String,
    pub rpm: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VoltageInfo {
    pub name: String,
    pub volts: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NetInfo {
    pub adapter: String,
    pub down_bps: Option<f64>,
    pub up_bps: Option<f64>,
    pub link_bps: Option<i64>,
    pub down_pct: Option<f64>,
    pub up_pct: Option<f64>,
}

/// Parse one NDJSON line. Returns `None` for blank or malformed input — a bad
/// line must never crash the reader thread.
pub fn parse_snapshot(line: &str) -> Option<Snapshot> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    serde_json::from_str(line).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL: &str = r#"{"v":1,"ts":1747645200,
      "cpu":{"name":"Test CPU","load":34.2,"cores":[38.1,55.0],"temp":58.0,"clock_mhz":4400,"power_w":52.0,"core_temps":[60.0,null,62.0,null]},
      "gpu":[{"name":"RTX 4070","kind":"discrete","load":51.0,"temp":71.0,"vram_used_mb":6348,"vram_total_mb":12288,"clock_mhz":2610,"fan_rpm":1480,"power_w":140.0,"memory_load":42.5}],
      "ram":{"used_mb":15462,"total_mb":32768,"available_mb":17306,"load":47.2,"cached_mb":4403,"pagefile_used_mb":2150,"pagefile_total_mb":8192},
      "storage":[{"name":"Samsung 980 Pro","kind":"nvme","temp":48.0,"activity":22.0,"used_gb":412.0,"total_gb":931.5}],
      "board":{"temp":38.0,"vrm_temp":61.0},
      "fans":[{"name":"CPU","rpm":920}],
      "voltages":[{"name":"+12V","volts":12.08}],
      "net":{"adapter":"Ethernet","down_bps":4404019,"up_bps":629145,"link_bps":1000000000,"down_pct":35.2,"up_pct":5.0}}"#;

    #[test]
    fn parses_a_full_snapshot() {
        let s = parse_snapshot(FULL).expect("should parse");
        assert_eq!(s.v, 1);
        assert_eq!(s.cpu.as_ref().unwrap().name, "Test CPU");
        assert_eq!(s.cpu.as_ref().unwrap().cores.as_ref().unwrap().len(), 2);
        assert_eq!(s.gpu.as_ref().unwrap()[0].kind, "discrete");
        assert_eq!(s.net.as_ref().unwrap().link_bps, Some(1_000_000_000));
    }

    #[test]
    fn parses_a_snapshot_with_sections_omitted() {
        let s = parse_snapshot(r#"{"v":1,"ts":1,"cpu":{"name":"X","load":1.0}}"#)
            .expect("should parse");
        assert!(s.gpu.is_none());
        assert!(s.storage.is_none());
        assert!(s.cpu.as_ref().unwrap().temp.is_none());
    }

    #[test]
    fn rejects_malformed_input() {
        assert!(parse_snapshot("not json").is_none());
        assert!(parse_snapshot("").is_none());
    }

    #[test]
    fn parses_gpu_memory_load() {
        let s = parse_snapshot(FULL).expect("should parse");
        let gpu = &s.gpu.as_ref().unwrap()[0];
        assert_eq!(gpu.memory_load, Some(42.5));
    }

    #[test]
    fn parses_per_core_temperatures_with_gaps() {
        let s = parse_snapshot(FULL).expect("should parse");
        let temps = s.cpu.as_ref().unwrap().core_temps.as_ref().unwrap();
        assert_eq!(temps.len(), 4);
        assert_eq!(temps[0], Some(60.0));
        assert_eq!(temps[1], None);
        assert_eq!(temps[2], Some(62.0));
        assert_eq!(temps[3], None);
    }

    #[test]
    fn parses_a_snapshot_without_new_fields() {
        let s = parse_snapshot(
            r#"{"v":1,"ts":1,"cpu":{"name":"X","load":1.0},
                "gpu":[{"name":"G","kind":"discrete","load":2.0}]}"#,
        )
        .expect("should parse");
        assert!(s.cpu.as_ref().unwrap().core_temps.is_none());
        assert!(s.gpu.as_ref().unwrap()[0].memory_load.is_none());
    }
}
