//! The CPU component panel.

use super::{card, panel_title};
use crate::format::{finite, format_temp, TempUnit};
use crate::history::RingBuffer;
use crate::ipc::CpuInfo;
use crate::theme::Theme;
use crate::ui::tooltips::tip;
use crate::widgets::bars::{core_grid, core_strip};
use crate::widgets::gauge::donut;
use crate::widgets::sparkline::sparkline;
use crate::widgets::stat::stat_row;
use crate::widgets::{temp_color, TempKind};

/// Render the CPU card: title, a load donut beside temp/clock/power stats,
/// either a per-core load strip or a per-core heat-map (selected by
/// `show_heat_map`), and a load-history sparkline.
///
/// `show_heat_map = false` is the default UI; `true` is opt-in via the
/// `cpu_heat_map` config flag, toggled from the title-bar chip.
///
/// Every absent (`None`) reading renders as `"—"`; nothing here can panic.
pub fn cpu_panel(
    ui: &mut egui::Ui,
    theme: &Theme,
    cpu: &CpuInfo,
    history: Option<&RingBuffer>,
    unit: TempUnit,
    show_heat_map: bool,
    min_h: f32,
) {
    card(ui, theme, min_h, |ui| {
        panel_title(ui, theme, "CPU", Some(&cpu.name));

        // Load donut on the left, four to five stat rows on the right.
        ui.horizontal(|ui| {
            donut(ui, theme, cpu.load.unwrap_or(0.0) as f32, "%", "LOAD").on_hover_text(
                "CPU utilisation across every logical core, 0–100 %. \
                     100 % means every core is executing instructions every cycle.",
            );
            ui.vertical(|ui| {
                let temp_value = format_temp(cpu.temp, unit);
                let temp_col = cpu.temp.map(|t| temp_color(t, TempKind::Processor, theme));
                tip(stat_row(ui, theme, "TEMP", &temp_value, temp_col), "TEMP");

                // TJMAX (Distance to TjMax). Only renders when sensord reports
                // the per-core MSR — laptops on PawnIO; many AMD desktops too.
                if let Some(d) = finite(cpu.distance_to_tjmax_c) {
                    let val = format!("{d:.0} \u{00B0}C");
                    // Lower headroom = closer to throttling. Re-use the
                    // temp-colour ramp by treating "headroom remaining" as
                    // its inverse: 5 °C of headroom ≈ ~90 °C silicon.
                    let pseudo_temp = (95.0_f64 - d).clamp(0.0, 105.0);
                    let col = temp_color(pseudo_temp, TempKind::Processor, theme);
                    tip(stat_row(ui, theme, "TJMAX", &val, Some(col)), "TJMAX");
                }

                tip(
                    stat_row(ui, theme, "CLOCK", &format_clock(cpu.clock_mhz), None),
                    "CLOCK",
                );

                // Package power as the primary "POWER" reading. When the
                // RAPL sub-domains are also exposed, hover reveals the
                // breakdown (Cores / DRAM / Platform).
                let breakdown = power_breakdown_tooltip(cpu);
                let power_resp = stat_row(ui, theme, "POWER", &format_power(cpu.power_w), None);
                let power_resp = tip(power_resp, "POWER");
                if let Some(text) = breakdown {
                    power_resp.on_hover_text(text);
                }

                if let Some(v) = finite(cpu.voltage_v) {
                    tip(
                        stat_row(ui, theme, "VCORE", &format!("{v:.3} V"), None),
                        "VCORE",
                    );
                }
            });
        });

        // Per-core display: strip (default) or heat-map (opt-in).
        let cores: Vec<f32> = cpu
            .cores
            .as_ref()
            .map(|c| c.iter().map(|&v| v as f32).collect())
            .unwrap_or_default();
        if show_heat_map {
            let temps: Vec<Option<f32>> = cpu
                .core_temps
                .as_ref()
                .map(|ts| ts.iter().map(|t| t.map(|v| v as f32)).collect())
                .unwrap_or_default();
            core_grid(
                ui,
                theme,
                &cores,
                &temps,
                unit,
                cpu.p_core_count.map(|n| n as usize),
            );
        } else {
            core_strip(ui, theme, &cores, cpu.p_core_count.map(|n| n as usize));
        }

        // Load-history sparkline.
        let samples: Vec<f32> = history
            .map(|h| h.iter_oldest_first().collect())
            .unwrap_or_default();
        sparkline(ui, theme, &samples, 100.0);
    });
}

/// Build the multi-line hover-text shown over the POWER row when at least one
/// RAPL sub-domain reading is present. Returns `None` when only the package
/// figure is available — the row keeps its standard tooltip in that case.
fn power_breakdown_tooltip(cpu: &CpuInfo) -> Option<String> {
    let cores = finite(cpu.power_cores_w);
    let mem = finite(cpu.power_memory_w);
    let plat = finite(cpu.power_platform_w);
    if cores.is_none() && mem.is_none() && plat.is_none() {
        return None;
    }
    let mut out = String::from("Package power broken down by RAPL domain:\n");
    if let Some(p) = finite(cpu.power_w) {
        out.push_str(&format!("  Package     {p:>5.1} W\n"));
    }
    if let Some(p) = cores {
        out.push_str(&format!("  Cores       {p:>5.1} W\n"));
    }
    if let Some(p) = mem {
        out.push_str(&format!("  DRAM        {p:>5.1} W\n"));
    }
    if let Some(p) = plat {
        out.push_str(&format!("  Platform    {p:>5.1} W\n"));
    }
    Some(out.trim_end().to_string())
}

/// MHz rendered as `"4.40 GHz"`, or `"—"` when absent.
fn format_clock(clock_mhz: Option<f64>) -> String {
    match clock_mhz {
        Some(mhz) => format!("{:.2} GHz", mhz / 1000.0),
        None => "—".to_string(),
    }
}

/// Watts rendered as whole units, `"52 W"`, or `"—"` when absent.
fn format_power(power_w: Option<f64>) -> String {
    match power_w {
        Some(w) => format!("{} W", w.round() as i64),
        None => "—".to_string(),
    }
}
