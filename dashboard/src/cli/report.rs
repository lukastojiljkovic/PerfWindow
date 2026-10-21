//! The report `perfwindow-cli` prints: plain text for people and LLM agents,
//! and JSON for scripts. Both carry what the window shows, section by section
//! in the window's order. A reading the sensors did not send is left out
//! rather than written as a dash, and a section with no readings is omitted.

use crate::displays::DisplayMode;
use crate::format::{
    finite, format_bytes_per_sec, format_gb_from_mb, format_gb_pair, format_link, format_uptime,
    TempUnit,
};
use crate::ipc::{
    BatteryInfo, BoardInfo, CpuInfo, GpuInfo, NetInfo, RamInfo, RamModule, Snapshot, StorageInfo,
};
use crate::panels::network::wifi_value;
use crate::panels::ram::hottest_dimm;
use crate::panels::sensors::has_content;
use crate::panels::storage::{
    format_capacity, format_data_volume, format_power_on_hours, kind_label,
};
use serde_json::{json, Value};
use std::fmt::Display;

/// The plain-text report: a header line, then one block per section.
pub fn text(snapshot: &Snapshot, displays: &[DisplayMode], unit: TempUnit) -> String {
    let mut blocks: Vec<String> = Vec::new();
    blocks.extend(snapshot.cpu.as_ref().and_then(|cpu| cpu_block(cpu, unit)));
    blocks.extend(
        snapshot
            .gpu
            .iter()
            .flatten()
            .filter_map(|gpu| gpu_block(gpu_title(gpu), gpu, unit)),
    );
    blocks.extend(
        snapshot
            .igpu
            .as_ref()
            .and_then(|gpu| gpu_block("iGPU", gpu, unit)),
    );
    blocks.extend(snapshot.ram.as_ref().and_then(|ram| ram_block(ram, unit)));
    blocks.extend(
        snapshot
            .storage
            .iter()
            .flatten()
            .filter_map(|disk| storage_block(disk, unit)),
    );
    blocks.extend(snapshot.net.as_ref().and_then(network_block));
    blocks.extend(snapshot.battery.as_ref().and_then(battery_block));
    blocks.extend(sensors_block(snapshot, unit));
    blocks.extend(system_block(snapshot));
    blocks.extend(displays_block(displays));

    let header = format!("PerfWindow {} — sensor readings", env!("CARGO_PKG_VERSION"));
    std::iter::once(header)
        .chain(blocks)
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// The JSON report. The snapshot is serialized as the sensors sent it, with
/// absent readings absent and temperatures in Celsius whatever the setting.
pub fn json(snapshot: &Snapshot, displays: &[DisplayMode]) -> Value {
    json!({
        "perfwindow": env!("CARGO_PKG_VERSION"),
        "temperature_unit": "C",
        "snapshot": snapshot,
        "displays": displays,
    })
}

/// A section: its title line and its readings. `None` when no reading is
/// present, so the section drops out of the report.
fn block(title: impl Display, rows: Vec<Option<String>>) -> Option<String> {
    let rows: Vec<String> = rows.into_iter().flatten().collect();
    (!rows.is_empty()).then(|| format!("{title}\n{}", rows.join("\n")))
}

/// One `  Label: value` line, or `None` when the reading is absent.
fn row(label: impl Into<String>, value: Option<String>) -> Option<String> {
    value.map(|value| format!("  {}: {value}", label.into()))
}

/// A per-core list, `38 %, n/a, 40 %`. An absent entry reads `n/a`.
fn list(values: Vec<Option<String>>) -> Option<String> {
    (!values.is_empty()).then(|| {
        values
            .into_iter()
            .map(|value| value.unwrap_or_else(|| "n/a".to_string()))
            .collect::<Vec<_>>()
            .join(", ")
    })
}

fn percent(value: Option<f64>) -> Option<String> {
    finite(value).map(|v| format!("{v:.0} %"))
}

fn temp(celsius: Option<f64>, unit: TempUnit) -> Option<String> {
    let symbol = match unit {
        TempUnit::Celsius => "°C",
        TempUnit::Fahrenheit => "°F",
    };
    finite(celsius).map(|c| format!("{:.0} {symbol}", unit.convert(c)))
}

fn megahertz(value: Option<f64>) -> Option<String> {
    finite(value).map(|v| format!("{v:.0} MHz"))
}

fn watts(value: Option<f64>) -> Option<String> {
    finite(value).map(|v| format!("{v:.1} W"))
}

fn volts(value: Option<f64>) -> Option<String> {
    finite(value).map(|v| format!("{v:.3} V"))
}

fn rpm(value: Option<f64>) -> Option<String> {
    finite(value).map(|v| format!("{v:.0} RPM"))
}

fn rate(bytes_per_sec: Option<f64>) -> Option<String> {
    finite(bytes_per_sec).map(format_bytes_per_sec)
}

fn gigabytes(mb: Option<f64>) -> Option<String> {
    finite(mb).map(|_| format!("{} GB", format_gb_from_mb(mb)))
}

fn non_empty(text: Option<&str>) -> Option<String> {
    text.map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
}

fn cpu_block(cpu: &CpuInfo, unit: TempUnit) -> Option<String> {
    block(
        format!("CPU — {}", cpu.name),
        vec![
            row("Load", percent(cpu.load)),
            row("Temperature", temp(cpu.temp, unit)),
            row(
                "Headroom to TjMax",
                finite(cpu.distance_to_tjmax_c).map(|d| format!("{d:.0} °C")),
            ),
            row("Clock", megahertz(cpu.clock_mhz)),
            row("Bus clock", megahertz(cpu.bus_clock_mhz)),
            row("Power", watts(cpu.power_w)),
            row("Core voltage", volts(cpu.voltage_v)),
            row(
                "Per-core load",
                cpu.cores
                    .as_ref()
                    .and_then(|cores| list(cores.iter().map(|&v| percent(Some(v))).collect())),
            ),
            row(
                "Per-core clock",
                cpu.core_clocks_mhz
                    .as_ref()
                    .and_then(|clocks| list(clocks.iter().map(|&c| megahertz(c)).collect())),
            ),
            row(
                "Per-core temperature",
                cpu.core_temps
                    .as_ref()
                    .and_then(|temps| list(temps.iter().map(|&t| temp(t, unit)).collect())),
            ),
        ],
    )
}

/// The window titles an integrated GPU `iGPU` and every other one `GPU`.
fn gpu_title(gpu: &GpuInfo) -> &'static str {
    if gpu.kind == "integrated" {
        "iGPU"
    } else {
        "GPU"
    }
}

fn gpu_block(title: &str, gpu: &GpuInfo, unit: TempUnit) -> Option<String> {
    let integrated = gpu.kind == "integrated";
    let vram = if integrated {
        finite(gpu.shared_vram_used_mb).map(|mb| format!("{mb:.0} MB shared"))
    } else {
        finite(gpu.vram_used_mb)
            .zip(finite(gpu.vram_total_mb))
            .map(|_| format_gb_pair(gpu.vram_used_mb, gpu.vram_total_mb))
    };
    let pcie = [gpu.pcie_rx_bps, gpu.pcie_tx_bps]
        .into_iter()
        .filter_map(finite)
        .reduce(|rx, tx| rx + tx);
    let engines = gpu
        .d3d_engines
        .iter()
        .flatten()
        .filter(|e| e.load.is_finite() && e.load >= 0.5)
        .map(|e| format!("{} {:.0} %", e.name, e.load))
        .collect::<Vec<_>>();

    block(
        format!("{title} — {}", gpu.name),
        vec![
            row("Load", percent(gpu.load)),
            row("Temperature", temp(gpu.temp, unit)),
            row("Hot spot", temp(gpu.hot_spot_temp, unit)),
            row("Memory junction", temp(gpu.memory_junction_temp_c, unit)),
            row("VRAM", vram),
            row("Power", watts(gpu.power_w)),
            row("Clock", megahertz(gpu.clock_mhz)),
            row("Memory clock", megahertz(gpu.memory_clock_mhz)),
            row("Memory controller load", percent(gpu.memory_load)),
            row("PCIe", rate(pcie)),
            row("Core voltage", volts(gpu.voltage_v)),
            row("Video engine load", percent(gpu.video_engine_load)),
            row("Engines", (!engines.is_empty()).then(|| engines.join(", "))),
        ],
    )
}

fn ram_block(ram: &RamInfo, unit: TempUnit) -> Option<String> {
    let swap = match (finite(ram.pagefile_used_mb), finite(ram.pagefile_total_mb)) {
        (Some(_), Some(total)) if total > 0.0 => {
            Some(format_gb_pair(ram.pagefile_used_mb, ram.pagefile_total_mb))
        }
        _ => None,
    };
    let hottest = hottest_dimm(ram.dimm_temps.as_deref().unwrap_or(&[]))
        .and_then(|(celsius, _)| temp(Some(celsius), unit));

    let mut rows = vec![
        row("Load", percent(ram.load)),
        row("Used", gigabytes(ram.used_mb)),
        row("Free", gigabytes(ram.available_mb)),
        row("Cached", gigabytes(ram.cached_mb)),
        row("Hottest DIMM", hottest),
        row("Swap", swap),
    ];
    rows.extend(
        ram.modules
            .iter()
            .flatten()
            .map(|module| module_row(module, unit)),
    );
    block("RAM", rows)
}

/// One installed module: its label, then capacity, temperature and timings
/// where the sensors report them.
fn module_row(module: &RamModule, unit: TempUnit) -> Option<String> {
    let parts: Vec<String> = [
        finite(module.capacity_gb).map(|gb| format!("{gb:.0} GB")),
        temp(module.temp_c, unit),
        non_empty(module.timings.as_deref()),
    ]
    .into_iter()
    .flatten()
    .collect();
    row(
        format!("Module {}", module.label.trim()),
        (!parts.is_empty()).then(|| parts.join(" · ")),
    )
}

fn storage_block(disk: &StorageInfo, unit: TempUnit) -> Option<String> {
    block(
        format!("Storage — {}", disk.name),
        vec![
            row("Type", Some(kind_label(&disk.kind))),
            row("Temperature", temp(disk.temp, unit)),
            row("Activity", percent(disk.activity)),
            row("Health", percent(disk.health)),
            row(
                "Capacity",
                finite(disk.used_gb)
                    .zip(finite(disk.total_gb))
                    .map(|_| format_capacity(disk.used_gb, disk.total_gb)),
            ),
            row("Read", rate(disk.read_bps)),
            row("Write", rate(disk.write_bps)),
            row(
                "Power-on hours",
                disk.power_on_hours.map(format_power_on_hours),
            ),
            row(
                "Power-on cycles",
                disk.power_on_count.map(|n| n.to_string()),
            ),
            row(
                "Data read",
                finite(disk.data_read_gb).map(format_data_volume),
            ),
            row(
                "Data written",
                finite(disk.data_written_gb).map(format_data_volume),
            ),
            row("NVMe wear", percent(disk.percentage_used_pct)),
            row("NVMe spare", percent(disk.available_spare_pct)),
        ],
    )
}

fn network_block(net: &NetInfo) -> Option<String> {
    let use_percent = [net.down_pct, net.up_pct]
        .into_iter()
        .filter_map(finite)
        .reduce(f64::max);
    // The window's LINK slot carries the Wi-Fi association on a wireless adapter.
    let link = match net.wifi.as_ref().and_then(wifi_value) {
        Some(wifi) => row("Wi-Fi", Some(wifi)),
        None => row("Link", net.link_bps.map(|_| format_link(net.link_bps))),
    };
    block(
        format!("Network — {}", net.adapter),
        vec![
            row("Use", percent(use_percent)),
            row("Download", rate(net.down_bps)),
            row("Upload", rate(net.up_bps)),
            link,
        ],
    )
}

fn battery_block(battery: &BatteryInfo) -> Option<String> {
    // Same thresholds as the window's STATE row.
    let state = finite(battery.rate_w).map(|w| {
        if w > 0.05 {
            "CHARGING"
        } else if w < -0.05 {
            "DISCHARGING"
        } else {
            "IDLE"
        }
    });
    let health = match (
        finite(battery.design_capacity_mwh),
        finite(battery.full_capacity_mwh),
    ) {
        (Some(design), Some(full)) if design > 0.0 => Some(format!(
            "{:.1} %",
            (full / design * 100.0).clamp(0.0, 100.0)
        )),
        _ => None,
    };
    block(
        "Battery",
        vec![
            row("Charge", percent(battery.charge_pct)),
            row("State", state.map(str::to_string)),
            // Positive while charging, negative while discharging.
            row("Rate", finite(battery.rate_w).map(|w| format!("{w:+.1} W"))),
            row(
                "Time remaining",
                battery
                    .time_remaining_sec
                    .filter(|s| *s > 0)
                    .map(format_uptime),
            ),
            row("Health", health),
        ],
    )
}

fn sensors_block(snapshot: &Snapshot, unit: TempUnit) -> Option<String> {
    let board = snapshot.board.as_ref();
    let fans = snapshot.fans.as_deref().unwrap_or(&[]);
    let voltages = snapshot.voltages.as_deref().unwrap_or(&[]);
    // Mirrors the window: identity alone does not make a Sensors section.
    if !has_content(board, fans, voltages) {
        return None;
    }

    let mut rows = vec![
        row("Board", board.and_then(|b| non_empty(b.name.as_deref()))),
        row("BIOS", board.and_then(bios)),
        row("Board temperature", board.and_then(|b| temp(b.temp, unit))),
        row(
            "VRM temperature",
            board.and_then(|b| temp(b.vrm_temp, unit)),
        ),
    ];
    rows.extend(fans.iter().map(|fan| row(&fan.name, rpm(fan.rpm))));
    rows.extend(
        snapshot
            .atk_fans
            .iter()
            .flatten()
            .map(|fan| row(format!("ATK {}", fan.name), rpm(fan.rpm))),
    );
    rows.extend(
        voltages
            .iter()
            .map(|volt| row(&volt.name, volts(volt.volts))),
    );
    block("Sensors", rows)
}

/// `16.0302 (2023-11-15)`: the BIOS version, with its date when it has one.
fn bios(board: &BoardInfo) -> Option<String> {
    let version = non_empty(board.bios_version.as_deref())?;
    Some(match non_empty(board.bios_date.as_deref()) {
        Some(date) => format!("{version} ({date})"),
        None => version,
    })
}

fn system_block(snapshot: &Snapshot) -> Option<String> {
    let health = snapshot.health.as_ref();
    block(
        "System",
        vec![
            row("Uptime", snapshot.uptime_sec.map(format_uptime)),
            row("PawnIO driver", health.map(|h| h.pawnio.clone())),
            row(
                "Degraded",
                health.map(|h| if h.degraded { "yes" } else { "no" }.to_string()),
            ),
            row(
                "Health notes",
                health.and_then(|h| non_empty(h.notes.as_deref())),
            ),
        ],
    )
}

fn displays_block(displays: &[DisplayMode]) -> Option<String> {
    block(
        "Displays",
        displays
            .iter()
            .map(|display| {
                let model = non_empty(display.model.as_deref())
                    .unwrap_or_else(|| "Unknown monitor".to_string());
                let primary = if display.primary { ", primary" } else { "" };
                row(
                    format!("{model} ({})", display.gdi_name),
                    Some(format!(
                        "{} x {} @ {} Hz{primary}",
                        display.width,
                        display.height,
                        display.refresh.label()
                    )),
                )
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::displays::RefreshRate;
    use crate::ipc::parse_snapshot;

    /// One sample of every section the window shows: two discrete GPUs and an
    /// iGPU, RAM with modules, two drives, a Wi-Fi link, a battery, fans,
    /// voltages and a degraded-free health block.
    const FULL: &str = r#"{"v":1,"ts":1747645200,"ts_ms":1747645200123,
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

    fn snapshot(json: &str) -> Snapshot {
        parse_snapshot(json).expect("fixture parses")
    }

    fn display() -> DisplayMode {
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

    #[test]
    fn text_has_a_header_and_every_section_in_window_order() {
        let report = text(&snapshot(FULL), &[display()], TempUnit::Celsius);
        assert!(report.starts_with(&format!(
            "PerfWindow {} — sensor readings",
            env!("CARGO_PKG_VERSION")
        )));
        let headers = [
            "CPU — Test CPU",
            "GPU — RTX 4070",
            "GPU — RTX 3060",
            "iGPU — Intel UHD 770",
            "RAM\n",
            "Storage — Samsung 980 Pro",
            "Storage — WD Blue",
            "Network — Wi-Fi",
            "Battery\n",
            "Sensors\n",
            "System\n",
            "Displays\n",
        ];
        let positions: Vec<usize> = headers
            .iter()
            .map(|h| report.find(h).unwrap_or_else(|| panic!("missing {h:?}")))
            .collect();
        assert!(positions.windows(2).all(|w| w[0] < w[1]), "{positions:?}");
    }

    #[test]
    fn text_writes_labelled_lines_with_units() {
        let report = text(&snapshot(FULL), &[], TempUnit::Celsius);
        for line in [
            "  Load: 34 %",
            "  Temperature: 58 °C",
            "  Headroom to TjMax: 12 °C",
            "  Clock: 4400 MHz",
            "  Bus clock: 100 MHz",
            "  Power: 52.0 W",
            "  Core voltage: 1.250 V",
            "  Per-core load: 38 %, 55 %",
            "  Per-core clock: 4400 MHz, n/a",
            "  Per-core temperature: 60 °C, n/a",
            "  VRAM: 6.2 / 12.0 GB",
            "  Engines: 3D 47 %",
            "  PCIe: 1.5 MB/s",
            "  Memory controller load: 43 %",
            "  VRAM: 332 MB shared",
            "  Used: 15.1 GB",
            "  Module Kingston KF556S40 #0: 16 GB · 44 °C · CL40-39-39 @ 5600 MT/s",
            "  Swap: 2.1 / 8.0 GB",
            "  Read: 1.0 MB/s",
            "  Power-on hours: 1200 h",
            "  NVMe wear: 3 %",
            "  Wi-Fi: HomeNet · 86 % · 867 Mbps",
            "  Download: 4.2 MB/s",
            "  Rate: -8.1 W",
            "  State: DISCHARGING",
            "  Time remaining: 3h 14m",
            "  Health: 92.0 %",
            "  CPU Fan: 920 RPM",
            "  ATK Chassis Fan: 1500 RPM",
            "  +12V: 12.080 V",
            "  BIOS: 16.0302 (2023-11-15)",
            "  Uptime: 3d 4h",
            "  PawnIO driver: ok",
            "  Degraded: no",
        ] {
            assert!(report.contains(line), "missing {line:?} in:\n{report}");
        }
    }

    #[test]
    fn text_reports_temperatures_in_the_chosen_unit() {
        let report = text(&snapshot(FULL), &[], TempUnit::Fahrenheit);
        assert!(report.contains("  Temperature: 136 °F"), "{report}");
    }

    #[test]
    fn text_leaves_out_absent_readings_and_empty_sections() {
        let report = text(
            &snapshot(r#"{"v":1,"ts":1,"cpu":{"name":"X","load":10.0}}"#),
            &[],
            TempUnit::Celsius,
        );
        assert!(report.contains("CPU — X\n  Load: 10 %"), "{report}");
        assert!(!report.contains("Temperature"), "{report}");
        assert!(!report.contains("Sensors"), "{report}");
        assert!(!report.contains("Displays"), "{report}");

        let empty = text(
            &snapshot(r#"{"v":1,"ts":1,"cpu":{"name":"X","load":null}}"#),
            &[],
            TempUnit::Celsius,
        );
        assert!(!empty.contains("CPU"), "{empty}");
    }

    #[test]
    fn text_lists_displays_with_resolution_refresh_and_primary() {
        let report = text(&snapshot(FULL), &[display()], TempUnit::Celsius);
        assert!(
            report.contains(r"  ROG XG27AQ (\\.\DISPLAY1): 2560 x 1440 @ 170 Hz, primary"),
            "{report}"
        );
    }

    #[test]
    fn json_keeps_absent_readings_out_and_temperatures_in_celsius() {
        let value = json(
            &snapshot(r#"{"v":1,"ts":1,"cpu":{"name":"X","temp":58.0}}"#),
            &[],
        );
        assert_eq!(value["temperature_unit"], "C");
        assert_eq!(value["snapshot"]["cpu"]["temp"], 58.0);
        assert!(value["snapshot"]["cpu"].get("load").is_none());
        assert!(value["snapshot"].get("gpu").is_none());
        assert_eq!(value["displays"], json!([]));
    }

    #[test]
    fn the_json_snapshot_parses_back_into_the_same_sample() {
        let original = snapshot(FULL);
        let value = json(&original, &[display()]);
        let parsed = parse_snapshot(&value["snapshot"].to_string()).expect("round trip");
        assert_eq!(parsed.ts, original.ts);
        assert_eq!(parsed.ts_ms, original.ts_ms);
        assert_eq!(parsed.cpu.unwrap().name, "Test CPU");
        assert_eq!(value["displays"][0]["primary"], true);
    }
}
