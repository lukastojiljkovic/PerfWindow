use serde::{Deserialize, Serialize};

/// One NDJSON snapshot from `sensord`. Sections are absent when the hardware
/// is. Every field added after the initial 0.1.0 schema uses
/// `#[serde(default)]` so older `sensord` builds parse cleanly.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Snapshot {
    pub v: i32,
    pub ts: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu: Option<CpuInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu: Option<Vec<GpuInfo>>,
    /// The integrated GPU when an iGPU is enumerated alongside a discrete
    /// one. Absent on desktops with no iGPU and on machines whose only GPU
    /// is integrated (it already appears in `gpu` as the sole entry).
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub igpu: Option<GpuInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ram: Option<RamInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage: Option<Vec<StorageInfo>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub board: Option<BoardInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fans: Option<Vec<FanInfo>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voltages: Option<Vec<VoltageInfo>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net: Option<NetInfo>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub battery: Option<BatteryInfo>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uptime_sec: Option<i64>,
    /// Fan readings from the ASUS ATK WMI bridge — only present on ASUS
    /// laptops/desktops with the ATK Package driver installed. Distinct
    /// from `fans` (which carries LHM Super-I/O fan readings) so the
    /// dashboard can surface them independently in the footer.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub atk_fans: Option<Vec<FanInfo>>,
    /// Sensord self-health summary. Absent on older sensord builds that
    /// predate the health probe (pre-0.8.0).
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health: Option<HealthInfo>,
    /// Snapshot timestamp in Unix milliseconds. Emitted alongside the
    /// seconds-resolution `ts` by sensord 0.10.0+ so sub-second refresh
    /// rates carry distinguishable stamps.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ts_ms: Option<i64>,
}

/// Sensord runtime health status. Emitted by sensord 0.8.0+ so the dashboard
/// can surface PawnIO availability and any degraded-mode notes.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HealthInfo {
    pub pawnio: String,
    #[serde(default)]
    pub degraded: bool,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CpuInfo {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub load: Option<f64>,
    /// Each physical core's load in percent, the mean of its hardware
    /// threads: one entry per core, not per thread. `cores`,
    /// `core_clocks_mhz` and `core_temps` are parallel, entry `i` of each
    /// being the same core, with the P-Cores first on hybrid silicon.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cores: Option<Vec<f64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temp: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clock_mhz: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub power_w: Option<f64>,
    /// Per-core temperatures in °C, parallel to `cores`. Individual entries
    /// may be `null`; absent where the CPU has no per-core sensor (AMD).
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub core_temps: Option<Vec<Option<f64>>>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voltage_v: Option<f64>,
    /// °C of headroom the hottest core has before throttling kicks in.
    /// Smaller = closer to TjMax. Populated when LHM exposes per-core
    /// "Distance to TjMax" sensors (Intel; some AMD).
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub distance_to_tjmax_c: Option<f64>,
    /// RAPL sub-domain power draw, watts. Whole-package draw lives in
    /// `power_w`; the three breakdown fields are only present when the
    /// silicon exposes them.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub power_cores_w: Option<f64>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub power_memory_w: Option<f64>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub power_platform_w: Option<f64>,
    /// On Intel hybrid CPUs, the count of P-Cores and E-Cores: the first
    /// `p_core_count` entries of each per-core array are the P-Cores, the
    /// next `e_core_count` the E-Cores. Both `None` on non-hybrid CPUs.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub p_core_count: Option<u32>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub e_core_count: Option<u32>,
    /// Front-side / base bus clock ("Bus Speed"), MHz. The reference clock
    /// the per-core multipliers run against.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bus_clock_mhz: Option<f64>,
    /// Per-core clocks in MHz, parallel to `cores`. Individual entries may
    /// be `null` when a core's clock sensor is missing.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub core_clocks_mhz: Option<Vec<Option<f64>>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GpuInfo {
    pub name: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub load: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temp: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vram_used_mb: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vram_total_mb: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clock_mhz: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fan_rpm: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub power_w: Option<f64>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_load: Option<f64>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hot_spot_temp: Option<f64>,
    /// GDDR/VRAM die hot spot (°C). A separate physical sensor from `temp`
    /// (GPU core) and `hot_spot_temp` (GPU die hot spot).
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_junction_temp_c: Option<f64>,
    /// PCIe link-direction throughput in bytes per second. Useful for
    /// streaming / model-loading bus-saturation diagnosis.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pcie_rx_bps: Option<f64>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pcie_tx_bps: Option<f64>,
    /// VRAM split: dedicated-on-card vs DXGI-shared system RAM, MB. The
    /// shared figure is mostly meaningful on iGPUs and laptops whose dGPU
    /// spills into RAM.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dedicated_vram_used_mb: Option<f64>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shared_vram_used_mb: Option<f64>,
    /// GPU core voltage rail, volts.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voltage_v: Option<f64>,
    /// Per-engine GPU utilisation breakdown (DXGI engines: 3D, Copy,
    /// Video Decode, Video Encode, Optical Flow, etc.). Sorted by load
    /// descending — first entry is the busiest engine right now.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub d3d_engines: Option<Vec<D3DEngineLoad>>,
    /// GDDR/VRAM memory clock ("GPU Memory"), MHz.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_clock_mhz: Option<f64>,
    /// Dedicated video encode/decode engine load ("GPU Video Engine"),
    /// 0–100 %. Independent of the 3D/compute load.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_engine_load: Option<f64>,
}

/// One DXGI engine's utilisation reading, as emitted by sensord.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct D3DEngineLoad {
    pub name: String,
    pub load: f64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RamInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub used_mb: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_mb: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available_mb: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub load: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_mb: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pagefile_used_mb: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pagefile_total_mb: Option<f64>,
    /// Per-module DIMM temperatures (°C). Populated on platforms whose SPD
    /// hub exposes a thermal sensor — every DDR5 SO-DIMM and most DDR4
    /// desktop kits. Older `sensord` builds omit the field.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dimm_temps: Option<Vec<DimmTemp>>,
    /// Per-module identity detail from the LHM SPD reader (sensord 0.10.0+).
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modules: Option<Vec<RamModule>>,
}

/// One memory-module temperature reading, as emitted by sensord.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DimmTemp {
    pub label: String,
    pub temp_c: f64,
}

/// One physical memory module: SPD node name, capacity in GB, module
/// temperature in °C and a CL-style timings summary like
/// `"CL40-39-39 @ 5600 MT/s"` (null when the SPD timing set is incomplete).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RamModule {
    pub label: String,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capacity_gb: Option<f64>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temp_c: Option<f64>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timings: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StorageInfo {
    pub name: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temp: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activity: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub used_gb: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_gb: Option<f64>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health: Option<f64>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_bps: Option<f64>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub write_bps: Option<f64>,
    /// Lifetime metrics — exposed on most NVMe drives and many SATA SSDs.
    /// `power_on_hours` and `power_on_count` come from S.M.A.R.T. attributes;
    /// `available_spare_pct` is the NVMe-spec reserved-blocks reading (0–100;
    /// distinct from the merged `health` figure above).
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub power_on_hours: Option<i64>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub power_on_count: Option<i64>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available_spare_pct: Option<f64>,
    /// NVMe-spec wear figure ("Percentage Used"), 0–100 % where 0 = new.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub percentage_used_pct: Option<f64>,
    /// The drive's own thermal thresholds (°C). When both are present the
    /// TEMP column is coloured against them instead of the fixed bands.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temp_warn_c: Option<f64>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temp_crit_c: Option<f64>,
    /// Lifetime host transfer totals, GB.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_read_gb: Option<f64>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_written_gb: Option<f64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BatteryInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub charge_pct: Option<f64>,
    /// Signed rate: positive when charging, negative when discharging, in watts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_w: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voltage_v: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub design_capacity_mwh: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub full_capacity_mwh: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_remaining_sec: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BoardInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temp: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vrm_temp: Option<f64>,
    /// Motherboard model name from the LHM motherboard node (sensord 0.10.0+).
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// BIOS identity from WMI `Win32_BIOS`; `bios_date` is `yyyy-MM-dd`.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bios_version: Option<String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bios_date: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FanInfo {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rpm: Option<f64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VoltageInfo {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volts: Option<f64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NetInfo {
    pub adapter: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub down_bps: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub up_bps: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link_bps: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub down_pct: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub up_pct: Option<f64>,
    /// Wireless link detail — present only when the active adapter is a
    /// connected 802.11 interface (sensord 0.10.0+).
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wifi: Option<WifiInfo>,
}

/// Wireless link details for the active Wi-Fi adapter. `signal_pct` is
/// 0–100 %; `phy_mbps` is the negotiated PHY rate; `band` is e.g. "5 GHz".
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WifiInfo {
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssid: Option<String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signal_pct: Option<f64>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phy_mbps: Option<f64>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub band: Option<String>,
}

/// Staged-init progress, emitted by sensord 0.10.0+ while sensor categories
/// are still being enabled. Deliberately not snapshot-shaped (no `v`/`ts`) so
/// pre-0.10 dashboards drop the line in their snapshot parser.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProgressInfo {
    /// Category currently being enabled; `None` once all are done.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loading: Option<String>,
    pub done: Vec<String>,
    pub pending: Vec<String>,
}

/// One recognised NDJSON line off the sensor feed. The snapshot is boxed:
/// `Snapshot` is two orders of magnitude larger than `ProgressInfo`, and the
/// value is moved straight into shared state by the reader thread anyway.
#[derive(Debug, Clone)]
pub enum Line {
    Snap(Box<Snapshot>),
    Progress(ProgressInfo),
}

/// Wire shape of a progress line: `{"progress":{...}}`.
#[derive(Deserialize)]
struct ProgressLine {
    progress: ProgressInfo,
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

/// Parse one NDJSON line into a snapshot or a progress message. Unknown or
/// malformed lines yield `None` so the reader threads survive anything the
/// pipe carries (forward compatibility with future message types).
pub fn parse_line(line: &str) -> Option<Line> {
    if let Some(snap) = parse_snapshot(line) {
        return Some(Line::Snap(Box::new(snap)));
    }
    serde_json::from_str::<ProgressLine>(line.trim())
        .ok()
        .map(|p| Line::Progress(p.progress))
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

    /// Every v0.10.0 schema addition in one line, exactly as sensord emits it.
    const FULL_V10: &str = r#"{"v":1,"ts":1747645200,"ts_ms":1747645200123,
      "cpu":{"name":"Test CPU","load":34.2,"cores":[38.1,55.0],"temp":58.0,"clock_mhz":4400,
             "bus_clock_mhz":99.8,"core_clocks_mhz":[4400.0,null,3900.0]},
      "gpu":[{"name":"RTX 4070","kind":"discrete","load":51.0,"memory_clock_mhz":8001.0,"video_engine_load":12.5}],
      "ram":{"used_mb":15462,"total_mb":32768,
             "modules":[{"label":"Kingston KF556S40 #0","capacity_gb":16.0,"temp_c":44.5,"timings":"CL40-39-39 @ 5600 MT/s"},
                        {"label":"Kingston KF556S40 #1","capacity_gb":null,"temp_c":null,"timings":null}]},
      "storage":[{"name":"Samsung 980 Pro","kind":"nvme","temp":48.0,
                  "percentage_used_pct":3.0,"temp_warn_c":82.0,"temp_crit_c":85.0,
                  "data_read_gb":15234.5,"data_written_gb":20480.0}],
      "board":{"temp":38.0,"vrm_temp":61.0,"name":"ASUS FX507VI","bios_version":"16.0302","bios_date":"2023-11-15"},
      "net":{"adapter":"Wi-Fi","down_bps":4404019,"up_bps":629145,"link_bps":866000000,
             "wifi":{"ssid":"HomeNet","signal_pct":86.0,"phy_mbps":866.7,"band":"5 GHz"}}}"#;

    #[test]
    fn parses_every_v10_schema_addition() {
        let s = parse_snapshot(FULL_V10).expect("should parse");
        assert_eq!(s.ts_ms, Some(1_747_645_200_123));

        let cpu = s.cpu.as_ref().unwrap();
        assert_eq!(cpu.bus_clock_mhz, Some(99.8));
        let clocks = cpu.core_clocks_mhz.as_ref().unwrap();
        assert_eq!(clocks.len(), 3);
        assert_eq!(clocks[0], Some(4400.0));
        assert_eq!(clocks[1], None);

        let gpu = &s.gpu.as_ref().unwrap()[0];
        assert_eq!(gpu.memory_clock_mhz, Some(8001.0));
        assert_eq!(gpu.video_engine_load, Some(12.5));

        let modules = s.ram.as_ref().unwrap().modules.as_ref().unwrap();
        assert_eq!(modules.len(), 2);
        assert_eq!(modules[0].label, "Kingston KF556S40 #0");
        assert_eq!(modules[0].capacity_gb, Some(16.0));
        assert_eq!(modules[0].temp_c, Some(44.5));
        assert_eq!(
            modules[0].timings.as_deref(),
            Some("CL40-39-39 @ 5600 MT/s")
        );
        assert!(modules[1].capacity_gb.is_none());
        assert!(modules[1].timings.is_none());

        let disk = &s.storage.as_ref().unwrap()[0];
        assert_eq!(disk.percentage_used_pct, Some(3.0));
        assert_eq!(disk.temp_warn_c, Some(82.0));
        assert_eq!(disk.temp_crit_c, Some(85.0));
        assert_eq!(disk.data_read_gb, Some(15234.5));
        assert_eq!(disk.data_written_gb, Some(20480.0));

        let board = s.board.as_ref().unwrap();
        assert_eq!(board.name.as_deref(), Some("ASUS FX507VI"));
        assert_eq!(board.bios_version.as_deref(), Some("16.0302"));
        assert_eq!(board.bios_date.as_deref(), Some("2023-11-15"));

        let wifi = s.net.as_ref().unwrap().wifi.as_ref().unwrap();
        assert_eq!(wifi.ssid.as_deref(), Some("HomeNet"));
        assert_eq!(wifi.signal_pct, Some(86.0));
        assert_eq!(wifi.phy_mbps, Some(866.7));
        assert_eq!(wifi.band.as_deref(), Some("5 GHz"));
    }

    /// Older `sensord` builds still emit the `display`/`displays` fields for a
    /// short while after the dashboard stops reading them. They must parse and
    /// be ignored, not fail the whole line.
    #[test]
    fn ignores_legacy_display_fields() {
        let s = parse_snapshot(
            r#"{"v":1,"ts":1,
                "display":{"name":"\\\\.\\DISPLAY1","width":2560,"height":1440,"refresh_hz":170,"model":"ROG XG27AQ"},
                "displays":[{"name":"\\\\.\\DISPLAY1","width":2560,"height":1440,"refresh_hz":170,"model":"ROG XG27AQ"}]}"#,
        )
        .expect("legacy display fields should be ignored by serde");
        assert_eq!(s.v, 1);
        assert!(s.cpu.is_none());
    }

    #[test]
    fn pre_v10_snapshots_default_every_new_field_to_none() {
        // `FULL` is the 0.9.x-era sample: none of the v0.10.0 fields appear.
        let s = parse_snapshot(FULL).expect("should parse");
        assert!(s.ts_ms.is_none());
        let cpu = s.cpu.as_ref().unwrap();
        assert!(cpu.bus_clock_mhz.is_none());
        assert!(cpu.core_clocks_mhz.is_none());
        let gpu = &s.gpu.as_ref().unwrap()[0];
        assert!(gpu.memory_clock_mhz.is_none());
        assert!(gpu.video_engine_load.is_none());
        assert!(s.ram.as_ref().unwrap().modules.is_none());
        let disk = &s.storage.as_ref().unwrap()[0];
        assert!(disk.percentage_used_pct.is_none());
        assert!(disk.temp_warn_c.is_none());
        assert!(disk.temp_crit_c.is_none());
        assert!(disk.data_read_gb.is_none());
        assert!(disk.data_written_gb.is_none());
        let board = s.board.as_ref().unwrap();
        assert!(board.name.is_none());
        assert!(board.bios_version.is_none());
        assert!(board.bios_date.is_none());
        assert!(s.net.as_ref().unwrap().wifi.is_none());
    }

    #[test]
    fn parse_line_recognises_the_contract_progress_example() {
        let line = r#"{"progress":{"loading":"gpu","done":["cpu","ram","motherboard"],"pending":["storage","network","controller","battery"]}}"#;
        let Some(Line::Progress(p)) = parse_line(line) else {
            panic!("expected a progress line");
        };
        assert_eq!(p.loading.as_deref(), Some("gpu"));
        assert_eq!(p.done, ["cpu", "ram", "motherboard"]);
        assert_eq!(p.pending, ["storage", "network", "controller", "battery"]);
    }

    #[test]
    fn parse_line_accepts_null_and_absent_loading() {
        let final_line = r#"{"progress":{"loading":null,"done":["cpu","ram"],"pending":[]}}"#;
        let Some(Line::Progress(p)) = parse_line(final_line) else {
            panic!("expected a progress line");
        };
        assert!(p.loading.is_none());
        assert!(p.pending.is_empty());

        let absent = r#"{"progress":{"done":[],"pending":[]}}"#;
        let Some(Line::Progress(p)) = parse_line(absent) else {
            panic!("expected a progress line");
        };
        assert!(p.loading.is_none());
    }

    #[test]
    fn parse_line_still_parses_snapshots() {
        let Some(Line::Snap(s)) = parse_line(FULL) else {
            panic!("expected a snapshot line");
        };
        assert_eq!(s.v, 1);
    }

    #[test]
    fn parse_line_rejects_unknown_input() {
        assert!(parse_line("not json").is_none());
        assert!(parse_line("").is_none());
        assert!(parse_line(r#"{"future_message":42}"#).is_none());
    }
}
