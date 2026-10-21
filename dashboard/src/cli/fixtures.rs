//! Test fixtures shared by the report and the dashboard.

use crate::displays::{DisplayMode, RefreshRate};
use crate::ipc::{parse_snapshot, Snapshot};

/// One sample of every section the window shows: two discrete GPUs and an
/// iGPU, RAM with modules, two drives, a Wi-Fi link, a battery, fans,
/// voltages and a degraded-free health block.
pub const FULL: &str = r#"{"v":1,"ts":1747645200,"ts_ms":1747645200123,
  "cpu":{"name":"Test CPU","load":34.2,"cores":[38.1,55.0],"temp":58.0,"clock_mhz":4400,
         "power_w":52.0,"core_temps":[60.0,null],"distance_to_tjmax_c":12.0,"voltage_v":1.25,
         "bus_clock_mhz":99.8,"core_clocks_mhz":[4400.0,null]},
  "gpu":[{"name":"RTX 4070","kind":"discrete","load":51.0,"temp":71.0,"vram_used_mb":6348,
          "vram_total_mb":12288,"clock_mhz":2610,"power_w":140.0,"memory_load":42.7,
          "memory_clock_mhz":10501.0,"pcie_rx_bps":1048576,"pcie_tx_bps":524288,
          "video_engine_load":12.5,"d3d_engines":[{"name":"3D","load":47.0},{"name":"Copy","load":0.2}]},
         {"name":"RTX 3060","kind":"discrete","load":4.0,"temp":45.0,"vram_used_mb":512,"vram_total_mb":12288}],
  "igpu":{"name":"Intel UHD 770","kind":"integrated","load":3.0,"shared_vram_used_mb":332},
  "ram":{"used_mb":15462,"total_mb":32768,"available_mb":17306,"load":47.2,"cached_mb":4403,
         "pagefile_used_mb":2150,"pagefile_total_mb":8192,
         "dimm_temps":[{"label":"DIMM 0","temp_c":41.0},{"label":"DIMM 1","temp_c":44.2}],
         "modules":[{"label":"Kingston KF556S40 #0","capacity_gb":16.0,"temp_c":44.2,"timings":"CL40-39-39 @ 5600 MT/s"},
                    {"label":"Kingston KF556S40 #1","capacity_gb":null,"temp_c":null,"timings":null}]},
  "storage":[{"name":"Samsung 980 Pro","kind":"nvme","temp":48.0,"activity":22.0,"health":97.0,
              "used_gb":412.0,"total_gb":931.5,"read_bps":1048576,"write_bps":2097152,
              "power_on_hours":1200,"power_on_count":310,"percentage_used_pct":3.0,
              "available_spare_pct":100.0,"data_written_gb":20480.0},
             {"name":"WD Blue","kind":"hdd","temp":36.0,"activity":1.0,"health":88.0,
              "used_gb":1200.0,"total_gb":2000.0}],
  "board":{"temp":38.0,"vrm_temp":61.0,"name":"ASUS FX507VI","bios_version":"16.0302","bios_date":"2023-11-15"},
  "fans":[{"name":"CPU Fan","rpm":920}],
  "atk_fans":[{"name":"Chassis Fan","rpm":1500}],
  "voltages":[{"name":"+12V","volts":12.08}],
  "net":{"adapter":"Wi-Fi","down_bps":4404019,"up_bps":629145,"link_bps":866000000,
         "down_pct":35.2,"up_pct":5.0,
         "wifi":{"ssid":"HomeNet","signal_pct":86.0,"phy_mbps":866.7,"band":"5 GHz"}},
  "battery":{"charge_pct":64.0,"rate_w":-8.1,"design_capacity_mwh":50000.0,
             "full_capacity_mwh":46000.0,"time_remaining_sec":11640},
  "uptime_sec":273600,
  "health":{"pawnio":"ok","degraded":false}}"#;

/// `json` parsed as one sample off the sensor feed.
pub fn snapshot(json: &str) -> Snapshot {
    parse_snapshot(json).expect("fixture parses")
}

/// A primary 2560 x 1440 monitor at 170 Hz.
pub fn display() -> DisplayMode {
    DisplayMode {
        gdi_name: r"\\.\DISPLAY1".to_string(),
        model: Some("ROG XG27AQ".to_string()),
        width: 2560,
        height: 1440,
        refresh: RefreshRate {
            numerator: 170,
            denominator: 1,
        },
        position: (0, 0),
        primary: true,
    }
}
