//! The report `perfwindow-cli` prints: aligned text for people and LLM
//! agents, and JSON for scripts. Both carry what the window shows, section by
//! section in the window's order. A reading the sensors did not send is left
//! out rather than written as a dash, and a section with no readings is
//! omitted.

use crate::displays::DisplayMode;
use crate::format::{
    finite, format_bytes_per_sec, format_gb_from_mb, format_gb_pair, format_link, format_uptime,
    TempUnit,
};
use crate::ipc::{
    BatteryInfo, BoardInfo, CpuInfo, GpuInfo, NetInfo, RamInfo, RamModule, Snapshot, StorageInfo,
};
use crate::panels::cpu::headroom_as_temp;
use crate::panels::network::wifi_value;
use crate::panels::ram::{hottest_dimm, SWAP_WARN};
use crate::panels::sensors::has_content;
use crate::panels::storage::{
    disk_temp_severity, format_capacity, format_data_volume, format_power_on_hours, kind_label,
    ACTIVITY_WARN,
};
use crate::widgets::{health_severity, temp_severity, Severity, TempKind};
use serde_json::{json, Value};
use std::fmt::{Display, Write};

/// How the text report is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    /// Pure ASCII without escape codes, for pipes, files and LLM agents. A
    /// reader that decodes stdout in a legacy code page, as Windows
    /// PowerShell 5.1 does, gets no mojibake.
    Ascii,
    /// Unicode glyphs without escape codes: a terminal with colour off.
    Plain,
    /// Unicode glyphs, bold titles, dim labels and the window's warning
    /// colours.
    Color,
}

/// SGR parameters the [`Style::Color`] report uses.
const BOLD: &str = "1";
const DIM: &str = "2";
const YELLOW: &str = "33";
const RED: &str = "31";

impl Style {
    /// `text` as this style writes it: in ASCII by [`ascii`], or as it is.
    fn text(self, text: &str) -> String {
        match self {
            Style::Ascii => ascii(text),
            Style::Plain | Style::Color => text.to_string(),
        }
    }

    /// `text` wrapped in the SGR parameter `sgr` when this style colours.
    fn paint(self, text: &str, sgr: Option<&str>) -> String {
        match (self, sgr) {
            (Style::Color, Some(sgr)) => format!("\x1b[{sgr}m{text}\x1b[0m"),
            _ => text.to_string(),
        }
    }
}

/// The SGR colour of a reading at `severity`: the window's warn is amber,
/// its hot is red.
fn severity_sgr(severity: Severity) -> &'static str {
    match severity {
        Severity::Warn => YELLOW,
        Severity::Hot => RED,
    }
}

/// One reading: its label, its value and, where the window colours it, how
/// far past normal it is.
struct Row {
    label: String,
    value: String,
    severity: Option<Severity>,
}

/// One section: its title and its readings.
struct Section {
    title: String,
    rows: Vec<Row>,
}

/// The plain-text report: a header line naming the version and when the
/// sample was taken, then one section per component in the window's order.
pub fn text(snapshot: &Snapshot, displays: &[DisplayMode], unit: TempUnit, style: Style) -> String {
    let mut sections: Vec<Section> = Vec::new();
    sections.extend(snapshot.cpu.as_ref().and_then(|cpu| cpu_section(cpu, unit)));
    sections.extend(
        snapshot
            .gpu
            .iter()
            .flatten()
            .filter_map(|gpu| gpu_section(gpu_title(gpu), gpu, unit)),
    );
    sections.extend(
        snapshot
            .igpu
            .as_ref()
            .and_then(|gpu| gpu_section("iGPU", gpu, unit)),
    );
    sections.extend(snapshot.ram.as_ref().and_then(|ram| ram_section(ram, unit)));
    sections.extend(snapshot.net.as_ref().and_then(network_section));
    sections.extend(snapshot.battery.as_ref().and_then(battery_section));
    sections.extend(
        snapshot
            .storage
            .iter()
            .flatten()
            .filter_map(|disk| storage_section(disk, unit)),
    );
    sections.extend(sensors_section(snapshot, unit));
    sections.extend(system_section(snapshot));
    sections.extend(displays_section(displays));

    let taken = local_time(snapshot)
        .map(|(fields, offset)| format!(" at {}", timestamp(fields, offset)))
        .unwrap_or_default();
    let product = format!("PerfWindow {}", env!("CARGO_PKG_VERSION"));
    let header = style.paint(&product, Some(BOLD))
        + &style.paint(
            &style.text(&format!(" — sensor readings{taken}")),
            Some(DIM),
        );
    render(header, &sections, style)
}

/// `header`, then each section after a blank line: its title, then one
/// reading per line, indented, with every label padded to the section's
/// widest so the values line up.
fn render(header: String, sections: &[Section], style: Style) -> String {
    let mut out = header;
    for section in sections {
        let labels: Vec<String> = section
            .rows
            .iter()
            .map(|row| style.text(&format!("{}:", row.label)))
            .collect();
        let width = labels.iter().map(|l| l.chars().count()).max().unwrap_or(0);
        let title = style.paint(&style.text(&section.title), Some(BOLD));
        let _ = write!(out, "\n\n{title}");
        for (row, label) in section.rows.iter().zip(&labels) {
            let pad = width - label.chars().count();
            let label = style.paint(label, Some(DIM));
            let value = style.paint(&style.text(&row.value), row.severity.map(severity_sgr));
            let _ = write!(out, "\n  {label}{:pad$} {value}", "");
        }
    }
    out
}

/// `text` in pure ASCII. The report's own glyphs get ASCII stand-ins (`°`
/// drops out of `58 °C`, `—` reads `-`, ` · ` reads `, `, `≈` reads `~`), and
/// any other character outside ASCII, from a name the sensors sent, reads
/// `?`; `--json` carries such names exactly.
fn ascii(text: &str) -> String {
    text.replace(" · ", ", ")
        .chars()
        .filter(|&c| c != '°')
        .map(|c| match c {
            '—' => '-',
            '≈' => '~',
            c if c.is_ascii() => c,
            _ => '?',
        })
        .collect()
}

/// `text` without control characters (C0 such as ESC, CR and LF, and C1).
/// Every title, label and value goes through it, because names come from the
/// sensors and from Windows: a Wi-Fi network can name itself with escape
/// sequences, or with a newline and a fake `Label: value` line, which would
/// otherwise reach the terminal or an agent reading the report.
pub(super) fn clean(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).collect()
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

/// The JSON report as text: indented, or compact on one line for `--watch`,
/// in ASCII by [`escape_non_ascii`].
pub fn json_text(snapshot: &Snapshot, displays: &[DisplayMode], pretty: bool) -> String {
    let value = json(snapshot, displays);
    let text = if pretty {
        serde_json::to_string_pretty(&value).unwrap_or_default()
    } else {
        value.to_string()
    };
    escape_non_ascii(&text)
}

/// `json` with every character outside ASCII written as a `\uXXXX` escape.
/// Such characters only occur inside JSON strings, where the escape stands
/// for the same character, so every JSON reader gets the same document, and
/// one that decodes stdout in a legacy code page gets no mojibake.
fn escape_non_ascii(json: &str) -> String {
    let mut out = String::with_capacity(json.len());
    for c in json.chars() {
        if c.is_ascii() {
            out.push(c);
        } else {
            for unit in c.encode_utf16(&mut [0; 2]) {
                let _ = write!(out, "\\u{unit:04x}");
            }
        }
    }
    out
}

/// When `snapshot` was taken, in local time: `[year, month, day, hour,
/// minute, second]` and the offset from UTC in minutes. `None` when Windows
/// cannot convert it.
#[cfg(windows)]
pub(super) fn local_time(snapshot: &Snapshot) -> Option<([u16; 6], i64)> {
    use windows::Win32::Foundation::{FILETIME, SYSTEMTIME};
    use windows::Win32::System::Time::{
        FileTimeToSystemTime, SystemTimeToFileTime, SystemTimeToTzSpecificLocalTime,
    };
    /// 100 ns ticks from 1601-01-01, where a `FILETIME` counts from, to 1970-01-01.
    const UNIX_EPOCH_TICKS: i64 = 116_444_736_000_000_000;
    const TICKS_PER_MINUTE: i64 = 600_000_000;

    let unix_ms = snapshot.ts_ms.unwrap_or(snapshot.ts.saturating_mul(1000));
    let ticks = unix_ms
        .checked_mul(10_000)?
        .checked_add(UNIX_EPOCH_TICKS)
        .filter(|t| *t >= 0)?;
    let utc_file = FILETIME {
        dwLowDateTime: ticks as u32,
        dwHighDateTime: (ticks >> 32) as u32,
    };
    let (mut utc, mut local, mut local_file) = (
        SYSTEMTIME::default(),
        SYSTEMTIME::default(),
        FILETIME::default(),
    );
    // SAFETY: each call reads and writes only the locals it is handed.
    unsafe {
        FileTimeToSystemTime(&utc_file, &mut utc).ok()?;
        SystemTimeToTzSpecificLocalTime(None, &utc, &mut local).ok()?;
        SystemTimeToFileTime(&local, &mut local_file).ok()?;
    }
    let local_ticks = ((local_file.dwHighDateTime as i64) << 32) | local_file.dwLowDateTime as i64;
    let fields = [
        local.wYear,
        local.wMonth,
        local.wDay,
        local.wHour,
        local.wMinute,
        local.wSecond,
    ];
    Some((fields, (local_ticks - ticks) / TICKS_PER_MINUTE))
}

#[cfg(not(windows))]
pub(super) fn local_time(_snapshot: &Snapshot) -> Option<([u16; 6], i64)> {
    None
}

/// `2025-05-19 11:00:00 +02:00`: local time fields and the offset from UTC.
fn timestamp([year, month, day, hour, minute, second]: [u16; 6], offset_min: i64) -> String {
    let sign = if offset_min < 0 { '-' } else { '+' };
    let offset = offset_min.abs();
    format!(
        "{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02} {sign}{:02}:{:02}",
        offset / 60,
        offset % 60
    )
}

/// A section, or `None` when none of its readings is present, so it drops
/// out of the report.
fn section(title: impl Display, rows: Vec<Option<Row>>) -> Option<Section> {
    let rows: Vec<Row> = rows.into_iter().flatten().collect();
    (!rows.is_empty()).then(|| Section {
        title: clean(&title.to_string()),
        rows,
    })
}

/// A reading, or `None` when it is absent.
fn row(label: impl AsRef<str>, value: Option<String>) -> Option<Row> {
    rated(label, value, None)
}

/// A reading the window colours at `severity`, or `None` when it is absent.
fn rated(label: impl AsRef<str>, value: Option<String>, severity: Option<Severity>) -> Option<Row> {
    value.map(|value| Row {
        label: clean(label.as_ref()),
        value: clean(&value),
        severity,
    })
}

/// A temperature in `unit`, rated on the window's bands for `kind`.
fn temp_row(label: &str, celsius: Option<f64>, kind: TempKind, unit: TempUnit) -> Option<Row> {
    let celsius = finite(celsius);
    rated(
        label,
        temp(celsius, unit),
        celsius.and_then(|c| temp_severity(c, kind)),
    )
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

/// `34 %`: a percentage, whole.
pub(super) fn percent(value: Option<f64>) -> Option<String> {
    finite(value).map(|v| format!("{v:.0} %"))
}

/// `58 °C`: a Celsius reading in `unit`, whole degrees.
pub(super) fn temp(celsius: Option<f64>, unit: TempUnit) -> Option<String> {
    let symbol = match unit {
        TempUnit::Celsius => "°C",
        TempUnit::Fahrenheit => "°F",
    };
    finite(celsius).map(|c| format!("{:.0} {symbol}", unit.convert(c)))
}

/// `4400 MHz`.
pub(super) fn megahertz(value: Option<f64>) -> Option<String> {
    finite(value).map(|v| format!("{v:.0} MHz"))
}

/// `52.0 W`.
pub(super) fn watts(value: Option<f64>) -> Option<String> {
    finite(value).map(|v| format!("{v:.1} W"))
}

/// `1.250 V`.
pub(super) fn volts(value: Option<f64>) -> Option<String> {
    finite(value).map(|v| format!("{v:.3} V"))
}

/// `920 RPM`.
pub(super) fn rpm(value: Option<f64>) -> Option<String> {
    finite(value).map(|v| format!("{v:.0} RPM"))
}

/// `2.0 MB/s`: bytes per second, as the window writes them.
pub(super) fn rate(bytes_per_sec: Option<f64>) -> Option<String> {
    finite(bytes_per_sec).map(format_bytes_per_sec)
}

/// `16.9 GB`: megabytes as gigabytes, as the window writes them.
pub(super) fn gigabytes(mb: Option<f64>) -> Option<String> {
    finite(mb).map(|_| format!("{} GB", format_gb_from_mb(mb)))
}

fn non_empty(text: Option<&str>) -> Option<String> {
    text.map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
}

fn cpu_section(cpu: &CpuInfo, unit: TempUnit) -> Option<Section> {
    let headroom = finite(cpu.distance_to_tjmax_c);
    section(
        format!("CPU — {}", cpu.name),
        vec![
            row("Load", percent(cpu.load)),
            temp_row("Temperature", cpu.temp, TempKind::Processor, unit),
            rated(
                "Headroom to TjMax",
                headroom.map(|d| format!("{d:.0} °C")),
                headroom.and_then(|d| temp_severity(headroom_as_temp(d), TempKind::Processor)),
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
pub(super) fn gpu_title(gpu: &GpuInfo) -> &'static str {
    if gpu.kind == "integrated" {
        "iGPU"
    } else {
        "GPU"
    }
}

fn gpu_section(title: &str, gpu: &GpuInfo, unit: TempUnit) -> Option<Section> {
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

    section(
        format!("{title} — {}", gpu.name),
        vec![
            row("Load", percent(gpu.load)),
            temp_row("Temperature", gpu.temp, TempKind::Processor, unit),
            temp_row("Hot spot", gpu.hot_spot_temp, TempKind::Processor, unit),
            temp_row(
                "Memory junction",
                gpu.memory_junction_temp_c,
                TempKind::Processor,
                unit,
            ),
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

/// The pagefile in use as `2.1 / 8.0 GB`, warn from the window's SWAP bar
/// threshold. `None` when no pagefile is configured.
pub(super) fn swap(ram: &RamInfo) -> Option<(String, f64, Option<Severity>)> {
    match (finite(ram.pagefile_used_mb), finite(ram.pagefile_total_mb)) {
        (Some(used), Some(total)) if total > 0.0 => {
            let fraction = (used / total).clamp(0.0, 1.0);
            Some((
                format_gb_pair(ram.pagefile_used_mb, ram.pagefile_total_mb),
                fraction,
                (fraction >= SWAP_WARN).then_some(Severity::Warn),
            ))
        }
        _ => None,
    }
}

fn ram_section(ram: &RamInfo, unit: TempUnit) -> Option<Section> {
    let swap = swap(ram);
    let hottest = hottest_dimm(ram.dimm_temps.as_deref().unwrap_or(&[])).map(|(c, _)| c);

    let mut rows = vec![
        row("Load", percent(ram.load)),
        row("Used", gigabytes(ram.used_mb)),
        row("Free", gigabytes(ram.available_mb)),
        row("Cached", gigabytes(ram.cached_mb)),
        temp_row("Hottest DIMM", hottest, TempKind::Processor, unit),
        rated(
            "Swap",
            swap.as_ref().map(|(text, ..)| text.clone()),
            swap.and_then(|(.., severity)| severity),
        ),
    ];
    rows.extend(
        ram.modules
            .iter()
            .flatten()
            .map(|module| row("Module", module_value(module, unit))),
    );
    section("RAM", rows)
}

/// One installed module: its label, then capacity, temperature and timings
/// where the sensors report them, `Kingston KF556S40 #0 · 16 GB · 44 °C`.
pub(super) fn module_value(module: &RamModule, unit: TempUnit) -> Option<String> {
    let parts: Vec<String> = [
        non_empty(Some(&module.label)),
        finite(module.capacity_gb).map(|gb| format!("{gb:.0} GB")),
        temp(module.temp_c, unit),
        non_empty(module.timings.as_deref()),
    ]
    .into_iter()
    .flatten()
    .collect();
    (!parts.is_empty()).then(|| parts.join(" · "))
}

fn storage_section(disk: &StorageInfo, unit: TempUnit) -> Option<Section> {
    let celsius = finite(disk.temp);
    let activity = finite(disk.activity);
    let health = finite(disk.health);
    section(
        format!("Storage — {}", disk.name),
        vec![
            row("Type", Some(kind_label(&disk.kind))),
            rated(
                "Temperature",
                temp(celsius, unit),
                celsius.and_then(|c| disk_temp_severity(c, disk.temp_warn_c, disk.temp_crit_c)),
            ),
            rated(
                "Activity",
                percent(activity),
                activity
                    .filter(|a| *a >= ACTIVITY_WARN)
                    .map(|_| Severity::Warn),
            ),
            rated("Health", percent(health), health.and_then(health_severity)),
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

/// The window's LINK slot: the Wi-Fi association on a wireless adapter, the
/// link speed otherwise, with the row label each one takes.
pub(super) fn link(net: &NetInfo) -> Option<(&'static str, String)> {
    match net.wifi.as_ref().and_then(wifi_value) {
        Some(wifi) => Some(("Wi-Fi", wifi)),
        None => net.link_bps.map(|_| ("Link", format_link(net.link_bps))),
    }
}

fn network_section(net: &NetInfo) -> Option<Section> {
    let use_percent = [net.down_pct, net.up_pct]
        .into_iter()
        .filter_map(finite)
        .reduce(f64::max);
    let link = link(net);
    section(
        format!("Network — {}", net.adapter),
        vec![
            row("Use", percent(use_percent)),
            row("Download", rate(net.down_bps)),
            row("Upload", rate(net.up_bps)),
            link.and_then(|(label, value)| row(label, Some(value))),
        ],
    )
}

/// The window's STATE for a battery charge rate in watts, and its colour: a
/// battery running down is the one state it paints warn.
pub(super) fn battery_state(rate_w: f64) -> (&'static str, Option<Severity>) {
    if rate_w > 0.05 {
        ("CHARGING", None)
    } else if rate_w < -0.05 {
        ("DISCHARGING", Some(Severity::Warn))
    } else {
        ("IDLE", None)
    }
}

/// The battery's full-charge capacity against its design capacity, in
/// percent: the window's HEALTH.
pub(super) fn battery_health(battery: &BatteryInfo) -> Option<f64> {
    match (
        finite(battery.design_capacity_mwh),
        finite(battery.full_capacity_mwh),
    ) {
        (Some(design), Some(full)) if design > 0.0 => {
            Some((full / design * 100.0).clamp(0.0, 100.0))
        }
        _ => None,
    }
}

fn battery_section(battery: &BatteryInfo) -> Option<Section> {
    let state = finite(battery.rate_w).map(battery_state);
    let health = battery_health(battery);
    section(
        "Battery",
        vec![
            row("Charge", percent(battery.charge_pct)),
            rated(
                "State",
                state.map(|(name, _)| name.to_string()),
                state.and_then(|(_, severity)| severity),
            ),
            // Positive while charging, negative while discharging.
            row("Rate", finite(battery.rate_w).map(|w| format!("{w:+.1} W"))),
            row(
                "Time remaining",
                battery
                    .time_remaining_sec
                    .filter(|s| *s > 0)
                    .map(format_uptime),
            ),
            rated(
                "Health",
                health.map(|h| format!("{h:.1} %")),
                health.and_then(health_severity),
            ),
        ],
    )
}

fn sensors_section(snapshot: &Snapshot, unit: TempUnit) -> Option<Section> {
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
        temp_row(
            "Board temperature",
            board.and_then(|b| b.temp),
            TempKind::Board,
            unit,
        ),
        temp_row(
            "VRM temperature",
            board.and_then(|b| b.vrm_temp),
            TempKind::Board,
            unit,
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
    section("Sensors", rows)
}

/// `16.0302 (2023-11-15)`: the BIOS version, with its date when it has one.
fn bios(board: &BoardInfo) -> Option<String> {
    let version = non_empty(board.bios_version.as_deref())?;
    Some(match non_empty(board.bios_date.as_deref()) {
        Some(date) => format!("{version} ({date})"),
        None => version,
    })
}

fn system_section(snapshot: &Snapshot) -> Option<Section> {
    let health = snapshot.health.as_ref();
    section(
        "System",
        vec![
            row("Uptime", snapshot.uptime_sec.map(format_uptime)),
            row("PawnIO driver", health.map(|h| h.pawnio.clone())),
            rated(
                "Degraded",
                health.map(|h| if h.degraded { "yes" } else { "no" }.to_string()),
                health.filter(|h| h.degraded).map(|_| Severity::Warn),
            ),
            row(
                "Health notes",
                health.and_then(|h| non_empty(h.notes.as_deref())),
            ),
        ],
    )
}

fn displays_section(displays: &[DisplayMode]) -> Option<Section> {
    section(
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
    use crate::cli::fixtures::{display, snapshot, FULL};

    fn full(style: Style) -> String {
        text(&snapshot(FULL), &[display()], TempUnit::Celsius, style)
    }

    /// Every indented `Label: value` reading of a report, with the padding
    /// between label and value taken out.
    fn readings(report: &str) -> Vec<(String, String)> {
        report
            .lines()
            .filter_map(|line| line.strip_prefix("  "))
            .filter_map(|line| line.split_once(": "))
            .map(|(label, value)| (label.to_string(), value.trim_start().to_string()))
            .collect()
    }

    fn assert_readings(report: &str, expected: &[(&str, &str)]) {
        let found = readings(report);
        for (label, value) in expected {
            assert!(
                found.iter().any(|(l, v)| l == label && v == value),
                "missing {label}: {value} in:\n{report}"
            );
        }
    }

    #[test]
    fn text_has_a_header_and_every_section_in_window_order() {
        let report = full(Style::Plain);
        assert!(
            report.starts_with(&format!(
                "PerfWindow {} — sensor readings at 2025-05-1",
                env!("CARGO_PKG_VERSION")
            )),
            "{report}"
        );
        let titles = [
            "\nCPU — Test CPU\n",
            "\nGPU — RTX 4070\n",
            "\nGPU — RTX 3060\n",
            "\niGPU — Intel UHD 770\n",
            "\nRAM\n",
            "\nNetwork — Wi-Fi\n",
            "\nBattery\n",
            "\nStorage — Samsung 980 Pro\n",
            "\nStorage — WD Blue\n",
            "\nSensors\n",
            "\nSystem\n",
            "\nDisplays\n",
        ];
        let positions: Vec<usize> = titles
            .iter()
            .map(|t| report.find(t).unwrap_or_else(|| panic!("missing {t:?}")))
            .collect();
        assert!(positions.windows(2).all(|w| w[0] < w[1]), "{positions:?}");
    }

    #[test]
    fn text_writes_labelled_lines_with_units() {
        assert_readings(
            &full(Style::Plain),
            &[
                ("Load", "34 %"),
                ("Temperature", "58 °C"),
                ("Headroom to TjMax", "12 °C"),
                ("Clock", "4400 MHz"),
                ("Bus clock", "100 MHz"),
                ("Power", "52.0 W"),
                ("Core voltage", "1.250 V"),
                ("Per-core load", "38 %, 55 %"),
                ("Per-core clock", "4400 MHz, n/a"),
                ("Per-core temperature", "60 °C, n/a"),
                ("VRAM", "6.2 / 12.0 GB"),
                ("Engines", "3D 47 %"),
                ("PCIe", "1.5 MB/s"),
                ("Memory controller load", "43 %"),
                ("VRAM", "332 MB shared"),
                ("Used", "15.1 GB"),
                (
                    "Module",
                    "Kingston KF556S40 #0 · 16 GB · 44 °C · CL40-39-39 @ 5600 MT/s",
                ),
                ("Module", "Kingston KF556S40 #1"),
                ("Swap", "2.1 / 8.0 GB"),
                ("Read", "1.0 MB/s"),
                ("Power-on hours", "1200 h  (≈ 0 y 50 d)"),
                ("NVMe wear", "3 %"),
                ("Wi-Fi", "HomeNet · 86 % · 867 Mbps"),
                ("Download", "4.2 MB/s"),
                ("Rate", "-8.1 W"),
                ("State", "DISCHARGING"),
                ("Time remaining", "3h 14m"),
                ("Health", "92.0 %"),
                ("CPU Fan", "920 RPM"),
                ("ATK Chassis Fan", "1500 RPM"),
                ("+12V", "12.080 V"),
                ("BIOS", "16.0302 (2023-11-15)"),
                ("Uptime", "3d 4h"),
                ("PawnIO driver", "ok"),
                ("Degraded", "no"),
            ],
        );
    }

    #[test]
    fn labels_are_padded_to_the_widest_in_their_section() {
        let report = full(Style::Plain);
        let cpu = report
            .split("\n\n")
            .find(|s| s.starts_with("CPU"))
            .expect("CPU section");
        let columns: Vec<usize> = cpu
            .lines()
            .skip(1)
            .map(|line| {
                let after = &line[line.find(':').expect("a label") + 1..];
                line.len() - after.trim_start().len()
            })
            .collect();
        // "  Per-core temperature: " is the widest label and sets the column.
        assert_eq!(columns.len(), 10, "{cpu}");
        assert!(columns.iter().all(|&c| c == 24), "{columns:?}\n{cpu}");
    }

    #[test]
    fn text_reports_temperatures_in_the_chosen_unit() {
        let report = text(&snapshot(FULL), &[], TempUnit::Fahrenheit, Style::Plain);
        assert_readings(&report, &[("Temperature", "136 °F")]);
    }

    #[test]
    fn text_leaves_out_absent_readings_and_empty_sections() {
        let report = text(
            &snapshot(r#"{"v":1,"ts":1,"cpu":{"name":"X","load":10.0}}"#),
            &[],
            TempUnit::Celsius,
            Style::Plain,
        );
        assert!(report.contains("\n\nCPU — X\n  Load: 10 %"), "{report}");
        assert!(!report.contains("Temperature"), "{report}");
        assert!(!report.contains("Sensors"), "{report}");
        assert!(!report.contains("Displays"), "{report}");

        let empty = text(
            &snapshot(r#"{"v":1,"ts":1,"cpu":{"name":"X","load":null}}"#),
            &[],
            TempUnit::Celsius,
            Style::Plain,
        );
        assert!(!empty.contains("CPU"), "{empty}");
    }

    #[test]
    fn text_lists_displays_with_resolution_refresh_and_primary() {
        assert!(full(Style::Plain)
            .contains(r"  ROG XG27AQ (\\.\DISPLAY1): 2560 x 1440 @ 170 Hz, primary"));
    }

    #[test]
    fn the_ascii_style_is_pure_ascii_with_the_same_readings() {
        let report = full(Style::Ascii);
        assert!(report.is_ascii(), "{report}");
        assert!(report.starts_with(&format!(
            "PerfWindow {} - sensor readings at 2025-05-1",
            env!("CARGO_PKG_VERSION")
        )));
        assert!(report.contains("\n\nCPU - Test CPU\n"), "{report}");
        assert_readings(
            &report,
            &[
                ("Temperature", "58 C"),
                ("Headroom to TjMax", "12 C"),
                ("Wi-Fi", "HomeNet, 86 %, 867 Mbps"),
                (
                    "Module",
                    "Kingston KF556S40 #0, 16 GB, 44 C, CL40-39-39 @ 5600 MT/s",
                ),
                ("Power-on hours", "1200 h  (~ 0 y 50 d)"),
            ],
        );
    }

    #[test]
    fn a_name_outside_ascii_reads_as_question_marks_in_the_ascii_style() {
        let sample = snapshot(r#"{"v":1,"ts":1,"cpu":{"name":"Ryzen™ 7","load":10.0}}"#);
        let report = text(&sample, &[], TempUnit::Celsius, Style::Ascii);
        assert!(report.contains("\nCPU - Ryzen? 7\n"), "{report}");
    }

    #[test]
    fn only_the_color_style_writes_escape_codes() {
        assert!(!full(Style::Plain).contains('\x1b'));
        assert!(!full(Style::Ascii).contains('\x1b'));
        let color = full(Style::Color);
        assert!(color.starts_with("\x1b[1mPerfWindow "), "{color:?}");
        assert!(
            color.contains("\n\n\x1b[1mCPU — Test CPU\x1b[0m\n"),
            "{color:?}"
        );
        assert!(color.contains("\n  \x1b[2mLoad:\x1b[0m"), "{color:?}");
    }

    #[test]
    fn readings_the_window_flags_carry_its_warning_colours() {
        let color = full(Style::Color);
        // 12 °C of headroom reads as 83 °C silicon, in the window's warn band.
        assert!(color.contains(" \x1b[33m12 °C\x1b[0m\n"), "{color:?}");
        assert!(color.contains(" \x1b[33mDISCHARGING\x1b[0m\n"), "{color:?}");
        // The VRM is past the board's 60 °C warn line; the board itself is calm.
        assert!(color.contains(" \x1b[33m61 °C\x1b[0m\n"), "{color:?}");
        assert!(color.contains(" 38 °C\n"), "{color:?}");

        let hot = text(
            &snapshot(r#"{"v":1,"ts":1,"cpu":{"name":"X","temp":95.0}}"#),
            &[],
            TempUnit::Celsius,
            Style::Color,
        );
        assert!(hot.contains(" \x1b[31m95 °C\x1b[0m"), "{hot:?}");
    }

    #[test]
    fn control_characters_from_the_sensors_are_stripped() {
        let sample = snapshot(
            r#"{"v":1,"ts":1,"net":{"adapter":"Wi\u0007Fi\u009b","down_bps":1024.0,
                "wifi":{"ssid":"Evil\u001b[2J\nFake: 1","signal_pct":50.0}}}"#,
        );
        let report = text(&sample, &[], TempUnit::Celsius, Style::Plain);
        // The report's own line breaks are the only control characters left.
        assert!(
            !report.chars().any(|c| c.is_control() && c != '\n'),
            "{report:?}"
        );
        assert!(report.contains("\n\nNetwork — WiFi\n"), "{report:?}");
        assert!(
            report.contains("Wi-Fi:    Evil[2JFake: 1 · 50 %"),
            "{report:?}"
        );
        assert!(
            !report
                .lines()
                .any(|line| line.trim_start().starts_with("Fake")),
            "{report:?}"
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
        let parsed =
            crate::ipc::parse_snapshot(&value["snapshot"].to_string()).expect("round trip");
        assert_eq!(parsed.ts, original.ts);
        assert_eq!(parsed.ts_ms, original.ts_ms);
        assert_eq!(parsed.cpu.unwrap().name, "Test CPU");
        assert_eq!(value["displays"][0]["primary"], true);
    }

    #[test]
    fn json_text_is_ascii_and_reads_back_as_the_same_document() {
        let sample = snapshot(r#"{"v":1,"ts":1,"cpu":{"name":"Ryzen™ 7 — 😀","load":10.0}}"#);
        for pretty in [true, false] {
            let text = json_text(&sample, &[], pretty);
            assert!(text.is_ascii(), "{text}");
            let value: Value = serde_json::from_str(&text).expect("valid JSON");
            assert_eq!(value, json(&sample, &[]));
            assert!(
                text.contains(r"Ryzen\u2122 7 \u2014 \ud83d\ude00"),
                "{text}"
            );
        }
        assert!(!json_text(&sample, &[], false).contains('\n'));
        assert!(json_text(&sample, &[], true).contains('\n'));
    }

    #[test]
    fn a_timestamp_names_the_local_time_and_its_offset() {
        assert_eq!(
            timestamp([2025, 5, 19, 11, 0, 7], 120),
            "2025-05-19 11:00:07 +02:00"
        );
        assert_eq!(
            timestamp([2025, 1, 2, 3, 4, 5], -330),
            "2025-01-02 03:04:05 -05:30"
        );
        assert_eq!(
            timestamp([2025, 1, 2, 3, 4, 5], 0),
            "2025-01-02 03:04:05 +00:00"
        );
    }

    #[cfg(windows)]
    #[test]
    fn local_time_converts_the_sample_stamp_in_any_zone() {
        // FULL was taken at 2025-05-19 09:00:00.123 UTC.
        let ([year, month, _, hour, minute, second], offset) =
            local_time(&snapshot(FULL)).expect("converts");
        assert_eq!((year, month, second), (2025, 5, 0));
        let utc_minutes = (i64::from(hour) * 60 + i64::from(minute) - offset).rem_euclid(1440);
        assert_eq!(utc_minutes, 9 * 60);
    }
}
