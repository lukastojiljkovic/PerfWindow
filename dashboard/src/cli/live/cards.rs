//! The dashboard's cards, one function each, with the stats the window's
//! card shows in the order it ranks them.

use super::widgets::{
    bar_line, card, chart, fit, grid_rows, samples, sparkline, stat_grid, stat_line, Stat,
    STAT_WIDTH,
};
use super::Model;
use crate::cli::report::{
    battery_health, battery_state, clean, gigabytes, gpu_title, link, megahertz, module_value,
    percent, rate, rpm, swap, temp, volts, watts,
};
use crate::format::{finite, format_uptime};
use crate::history::RingBuffer;
use crate::ipc::{BatteryInfo, CpuInfo, GpuInfo, NetInfo, RamInfo, StorageInfo};
use crate::panels::battery::format_rate;
use crate::panels::cpu::headroom_as_temp;
use crate::panels::ram::hottest_dimm;
use crate::panels::storage::{disk_temp_severity, format_capacity, kind_label};
use crate::widgets::bars::heat_cell;
use crate::widgets::{health_severity, temp_severity, TempKind};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Paragraph, Row, Table};
use ratatui::Frame;

/// Cells one per-core load takes in the core grid, its gap included.
const CORE_CELL: u16 = 4;

/// A temperature stat, coloured on the window's bands for `kind`.
fn temp_stat(label: &str, celsius: Option<f64>, kind: TempKind, model: &Model) -> Option<Stat> {
    let p = &model.palette;
    let celsius = finite(celsius)?;
    let style = p.rated(temp_severity(celsius, kind), p.theme.ok);
    Some(Stat::new(label, &temp(Some(celsius), model.unit)?, style))
}

/// A stat in bold ink, when the reading is present.
fn plain_stat(label: &str, value: Option<String>, model: &Model) -> Option<Stat> {
    value.map(|value| Stat::new(label, &value, model.palette.value()))
}

/// The CPU's stats.
fn cpu_stats(model: &Model, cpu: &CpuInfo) -> Vec<Stat> {
    let p = &model.palette;
    let headroom = finite(cpu.distance_to_tjmax_c).map(|d| {
        let severity = temp_severity(headroom_as_temp(d), TempKind::Processor);
        Stat::new(
            "HEADROOM",
            &format!("{d:.0} °C"),
            p.rated(severity, p.theme.ok),
        )
    });
    [
        plain_stat("LOAD", percent(cpu.load), model),
        temp_stat("TEMP", cpu.temp, TempKind::Processor, model),
        plain_stat("CLOCK", megahertz(cpu.clock_mhz), model),
        plain_stat("POWER", watts(cpu.power_w), model),
        headroom,
        plain_stat("VCORE", volts(cpu.voltage_v), model),
        plain_stat("BUS", megahertz(cpu.bus_clock_mhz), model),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// CPU: the load history chart, the stats, and each core's load.
pub(super) fn cpu(frame: &mut Frame, area: Rect, model: &Model, cpu: &CpuInfo) {
    let p = &model.palette;
    let inner = card(frame, area, "CPU", Some(&cpu.name), p);
    let loads = cpu.cores.as_deref().unwrap_or(&[]);
    let core_rows = inner.height.saturating_sub(1).max(1);
    let core_columns = (loads.len() as u16).div_ceil(core_rows);
    let cores_width = match core_columns {
        0 => 0,
        // "CORES" over the cells, which are three wide with a gap between.
        n => (n * CORE_CELL - 1).max(5).min(inner.width / 3),
    };
    let [history, _, stats, _, cores] = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(2),
        Constraint::Length(STAT_WIDTH + 2),
        Constraint::Length(if cores_width > 0 { 2 } else { 0 }),
        Constraint::Length(cores_width),
    ])
    .areas(inner);
    let load = samples(model.history.cpu.as_ref());
    chart(frame, history, &[(load, p.fg(p.theme.accent))], p);
    stat_grid(frame, stats, &cpu_stats(model, cpu), p);
    core_grid(frame, cores, loads, model);
}

/// Each core's load in digits on its heat-map colour, as the window's core
/// grid: a `CORES` heading, then the cells row by row.
fn core_grid(frame: &mut Frame, area: Rect, loads: &[f64], model: &Model) {
    let p = &model.palette;
    if area.width < CORE_CELL - 1 || loads.is_empty() {
        return;
    }
    let columns = usize::from((area.width + 1) / CORE_CELL).max(1);
    let mut lines = vec![Line::styled("CORES", p.label())];
    lines.extend(loads.chunks(columns).map(|row| {
        let mut spans = Vec::new();
        for (i, &load) in row.iter().enumerate() {
            if i > 0 {
                spans.push(Span::raw(" "));
            }
            let (fill, ink) = heat_cell(&p.theme, load as f32);
            let style = Style::new().bg(p.color(fill)).fg(p.color(ink));
            spans.push(Span::styled(format!("{:>3}", load.round() as i64), style));
        }
        Line::from(spans)
    }));
    frame.render_widget(Paragraph::new(lines), area);
}

/// A GPU's stats.
fn gpu_stats(model: &Model, gpu: &GpuInfo) -> Vec<Stat> {
    let vram = if gpu.kind == "integrated" {
        finite(gpu.shared_vram_used_mb).map(|mb| format!("{mb:.0} MB"))
    } else {
        finite(gpu.vram_used_mb)
            .zip(finite(gpu.vram_total_mb))
            .map(|(used, total)| format!("{:.1}/{:.1} GB", used / 1024.0, total / 1024.0))
    };
    let pcie = [gpu.pcie_rx_bps, gpu.pcie_tx_bps]
        .into_iter()
        .filter_map(finite)
        .reduce(|rx, tx| rx + tx);
    let processor = TempKind::Processor;
    [
        plain_stat("LOAD", percent(gpu.load), model),
        temp_stat("TEMP", gpu.temp, processor, model),
        plain_stat("VRAM", vram, model),
        plain_stat("POWER", watts(gpu.power_w), model),
        plain_stat("CLOCK", megahertz(gpu.clock_mhz), model),
        plain_stat("MEM USE", percent(gpu.memory_load), model),
        temp_stat("HOTSPOT", gpu.hot_spot_temp, processor, model),
        temp_stat("JUNCTION", gpu.memory_junction_temp_c, processor, model),
        plain_stat("MEM CLK", megahertz(gpu.memory_clock_mhz), model),
        plain_stat("PCIE", rate(pcie), model),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// A GPU or the iGPU: its stats beside a load and temperature chart.
pub(super) fn gpu(
    frame: &mut Frame,
    area: Rect,
    model: &Model,
    gpu: &GpuInfo,
    load: Option<&RingBuffer>,
    temp: Option<&RingBuffer>,
) {
    let p = &model.palette;
    let inner = card(frame, area, gpu_title(gpu), Some(&gpu.name), p);
    let [stats, _, right] = Layout::horizontal([
        Constraint::Length(STAT_WIDTH),
        Constraint::Length(2),
        Constraint::Fill(1),
    ])
    .areas(inner);
    stat_grid(frame, stats, &gpu_stats(model, gpu), p);

    let [legend, history] =
        Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).areas(right);
    let load_style = p.fg(p.theme.accent);
    let temp_style = p.fg(p.theme.ink);
    let mut series = vec![(samples(load), load_style)];
    let mut key = vec![
        Span::styled("━ ", load_style),
        Span::styled("LOAD", p.label()),
    ];
    let temps = samples(temp);
    if temps.iter().any(|t| t.is_finite()) {
        series.push((temps, temp_style));
        key.extend([
            Span::raw("  "),
            Span::styled("━ ", temp_style),
            Span::styled("TEMP °C", p.label()),
        ]);
    }
    frame.render_widget(Paragraph::new(Line::from(key)), legend);
    chart(frame, history, &series, p);
}

/// RAM's stats below its bars.
pub(super) fn ram_stats(model: &Model, ram: &RamInfo) -> Vec<Stat> {
    let hottest = hottest_dimm(ram.dimm_temps.as_deref().unwrap_or(&[])).map(|(c, _)| c);
    [
        plain_stat("FREE", gigabytes(ram.available_mb), model),
        plain_stat("CACHED", gigabytes(ram.cached_mb), model),
        temp_stat("DIMM", hottest, TempKind::Processor, model),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// RAM: used and swap bars, the stats, then the modules and the load
/// history in whatever room is left.
pub(super) fn ram(frame: &mut Frame, area: Rect, model: &Model, ram: &RamInfo) {
    let p = &model.palette;
    let inner = card(frame, area, "RAM", gigabytes(ram.total_mb).as_deref(), p);
    let width = inner.width;
    let mut bars = Vec::new();
    if let (Some(used), Some(total)) = (finite(ram.used_mb), finite(ram.total_mb)) {
        let value = format!("{:.1}/{:.1} GB", used / 1024.0, total / 1024.0);
        let fraction = if total > 0.0 { used / total } else { 0.0 };
        bars.push(bar_line(
            "USED",
            fraction,
            &value,
            p.fg(p.theme.accent),
            width,
            p,
        ));
    }
    if let Some((value, fraction, severity)) = swap(ram) {
        let fill = p.rated(severity, p.theme.accent);
        bars.push(bar_line(
            "SWAP",
            fraction,
            &value.replace(" / ", "/"),
            fill,
            width,
            p,
        ));
    }
    let stats = ram_stats(model, ram);
    let [bars_area, grid, rest] = Layout::vertical([
        Constraint::Length(bars.len() as u16),
        Constraint::Length(grid_rows(stats.len(), width)),
        Constraint::Fill(1),
    ])
    .areas(inner);
    frame.render_widget(Paragraph::new(bars), bars_area);
    stat_grid(frame, grid, &stats, p);

    let modules: Vec<Line> = ram
        .modules
        .iter()
        .flatten()
        .filter_map(|module| module_value(module, model.unit))
        .take(usize::from(rest.height))
        .map(|text| Line::styled(fit(&clean(&text), usize::from(width)), p.label()))
        .collect();
    let [listed, history] = Layout::vertical([
        Constraint::Length(modules.len() as u16),
        Constraint::Fill(1),
    ])
    .areas(rest);
    frame.render_widget(Paragraph::new(modules), listed);
    let load = samples(model.history.ram.as_ref());
    chart(frame, history, &[(load, p.fg(p.theme.accent))], p);
}

/// Network: download and upload rates beside their sparklines, then the
/// link or Wi-Fi line.
pub(super) fn network(frame: &mut Frame, area: Rect, model: &Model, net: &NetInfo) {
    let p = &model.palette;
    let inner = card(frame, area, "NETWORK", Some(&net.adapter), p);
    let link = link(net);
    let [down, up, link_row] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Fill(1),
        Constraint::Length(u16::from(link.is_some())),
    ])
    .areas(inner);
    let history = model.history.network.as_ref();
    let directions = [
        (
            down,
            "↓ DOWN",
            net.down_bps,
            history.map(|h| &h.down),
            p.theme.accent,
        ),
        (up, "↑ UP", net.up_bps, history.map(|h| &h.up), p.theme.ink),
    ];
    for (row, label, bps, buffer, color) in directions {
        let [text, _, spark] = Layout::horizontal([
            Constraint::Length(11),
            Constraint::Length(1),
            Constraint::Fill(1),
        ])
        .areas(row);
        let lines = vec![
            Line::styled(label, p.label()),
            Line::styled(rate(bps).unwrap_or_default(), p.value()),
        ];
        frame.render_widget(Paragraph::new(lines), text);
        sparkline(frame, spark, buffer, p.fg(color));
    }
    if let Some((label, value)) = link {
        let stat = Stat::new(&label.to_uppercase(), &value, p.value());
        let line = Line::from(stat_line(&stat, link_row.width, p));
        frame.render_widget(Paragraph::new(line), link_row);
    }
}

/// The battery's stats below its charge bar.
pub(super) fn battery_stats(model: &Model, battery: &BatteryInfo) -> Vec<Stat> {
    let p = &model.palette;
    let watts = finite(battery.rate_w);
    let state = watts.map(|w| {
        let (name, severity) = battery_state(w);
        Stat::new("STATE", name, p.rated(severity, p.theme.ok))
    });
    let health = battery_health(battery).map(|h| {
        let style = p.rated(health_severity(h), p.theme.ok);
        Stat::new("HEALTH", &format!("{h:.1} %"), style)
    });
    let time = battery.time_remaining_sec.filter(|s| *s > 0);
    [
        state,
        plain_stat("RATE", watts.map(|w| format_rate(Some(w))), model),
        plain_stat("TIME", time.map(format_uptime), model),
        health,
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// Battery: the charge bar and the stats.
pub(super) fn battery(frame: &mut Frame, area: Rect, model: &Model, battery: &BatteryInfo) {
    let p = &model.palette;
    let inner = card(frame, area, "BATTERY", None, p);
    let [charge, stats] =
        Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).areas(inner);
    if let Some(pct) = finite(battery.charge_pct) {
        let value = format!("{pct:.0} %");
        let line = bar_line(
            "CHARGE",
            pct / 100.0,
            &value,
            p.fg(p.theme.accent),
            inner.width,
            p,
        );
        frame.render_widget(Paragraph::new(line), charge);
    }
    stat_grid(frame, stats, &battery_stats(model, battery), p);
}

/// The storage table's columns: heading, width (0 for the disk name, which
/// takes the rest) and whether it is right-aligned. READ and WRITE, then
/// TYPE, are left out when the card is too narrow for them.
fn storage_columns(width: u16) -> Vec<(&'static str, u16, bool)> {
    /// The fewest cells the disk name keeps.
    const NAME: u16 = 10;
    let mut columns = vec![
        ("DISK", 0, false),
        ("TYPE", 4, false),
        ("CAPACITY", 14, true),
        ("TEMP", 6, true),
        ("READ", 10, true),
        ("WRITE", 10, true),
        ("HEALTH", 6, true),
    ];
    for dropped in [&["READ", "WRITE"][..], &["TYPE"]] {
        let needed: u16 = columns.iter().map(|c| c.1 + 1).sum::<u16>() + NAME;
        if needed > width {
            columns.retain(|c| !dropped.contains(&c.0));
        }
    }
    columns
}

/// Storage: one row per disk, with its type, capacity, temperature, read and
/// write rates and health.
pub(super) fn storage(frame: &mut Frame, area: Rect, model: &Model, disks: &[StorageInfo]) {
    let p = &model.palette;
    let count = format!("{} DISKS", disks.len());
    let inner = card(frame, area, "STORAGE", Some(&count), p);
    let columns = storage_columns(inner.width);
    let cell = |text: String, right: bool, style: Style| {
        let line = Line::styled(text, style);
        Cell::from(if right { line.right_aligned() } else { line })
    };
    let rows = disks.iter().map(|disk| {
        Row::new(columns.iter().map(|&(name, _, right)| {
            let (text, style) = match name {
                "DISK" => (Some(clean(&disk.name)), p.fg(p.theme.ink)),
                "TYPE" => (Some(kind_label(&disk.kind)), p.label()),
                "CAPACITY" => (
                    finite(disk.used_gb)
                        .and(finite(disk.total_gb))
                        .map(|_| format_capacity(disk.used_gb, disk.total_gb)),
                    p.value(),
                ),
                "TEMP" => {
                    let celsius = finite(disk.temp);
                    let severity = celsius
                        .and_then(|c| disk_temp_severity(c, disk.temp_warn_c, disk.temp_crit_c));
                    (temp(celsius, model.unit), p.rated(severity, p.theme.ok))
                }
                "READ" => (rate(disk.read_bps), p.value()),
                "WRITE" => (rate(disk.write_bps), p.value()),
                _ => {
                    let health = finite(disk.health);
                    let severity = health.and_then(health_severity);
                    (percent(health), p.rated(severity, p.theme.ok))
                }
            };
            cell(text.unwrap_or_default(), right, style)
        }))
    });
    let header = Row::new(
        columns
            .iter()
            .map(|&(name, _, right)| cell(name.to_string(), right, p.label())),
    );
    let widths = columns.iter().map(|&(_, width, _)| match width {
        0 => Constraint::Fill(1),
        width => Constraint::Length(width),
    });
    frame.render_widget(Table::new(rows, widths).header(header), inner);
}

/// The Sensors card's stats: board and VRM temperatures, fans, voltages.
pub(super) fn sensor_stats(model: &Model) -> Vec<Stat> {
    let snapshot = &model.snapshot;
    let board = snapshot.board.as_ref();
    let fans = snapshot.fans.iter().flatten().map(|f| (&f.name, f.rpm));
    let atk_fans = snapshot.atk_fans.iter().flatten().map(|f| (&f.name, f.rpm));
    let voltages = snapshot.voltages.iter().flatten();
    [
        temp_stat("BOARD", board.and_then(|b| b.temp), TempKind::Board, model),
        temp_stat(
            "VRM",
            board.and_then(|b| b.vrm_temp),
            TempKind::Board,
            model,
        ),
    ]
    .into_iter()
    .chain(
        fans.chain(atk_fans)
            .map(|(name, value)| plain_stat(&name.to_uppercase(), rpm(value), model)),
    )
    .chain(voltages.map(|v| plain_stat(&v.name.to_uppercase(), volts(v.volts), model)))
    .flatten()
    .collect()
}

/// Sensors: the board's temperatures, fans and voltages in a stat grid.
pub(super) fn sensors(frame: &mut Frame, area: Rect, model: &Model) {
    let p = &model.palette;
    let board = model
        .snapshot
        .board
        .as_ref()
        .and_then(|b| b.name.as_deref());
    let inner = card(frame, area, "SENSORS", board, p);
    stat_grid(frame, inner, &sensor_stats(model), p);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn narrow_storage_tables_drop_the_rates_then_the_type() {
        let names = |width| -> Vec<&str> { storage_columns(width).iter().map(|c| c.0).collect() };
        assert_eq!(names(80).len(), 7);
        assert_eq!(names(50), ["DISK", "TYPE", "CAPACITY", "TEMP", "HEALTH"]);
        assert_eq!(names(40), ["DISK", "CAPACITY", "TEMP", "HEALTH"]);
    }
}
