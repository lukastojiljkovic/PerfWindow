//! The CPU component panel.

use super::{card, panel_title};
use crate::format::{finite, format_temp, TempUnit};
use crate::history::{samples_or_empty, RingBuffer};
use crate::ipc::CpuInfo;
use crate::theme::Theme;
use crate::ui::capacity::Capacity;
use crate::ui::stat_priority::{render, StatCandidate};
use crate::widgets::bars::{core_grid, core_strip};
use crate::widgets::gauge::donut;
use crate::widgets::sparkline::{sparkline, MIN_HEIGHT as SPARKLINE_MIN_HEIGHT};
use crate::widgets::{temp_color, TempKind};

/// Render the CPU card: title, a load donut beside temp/clock/power stats,
/// either a per-core load strip or a per-core heat-map (selected by
/// `show_heat_map`), and a load-history sparkline.
///
/// `show_heat_map = false` is the default UI; `true` is opt-in via the
/// `cpu_heat_map` config flag, toggled from the title-bar chip.
///
/// Every absent (`None`) reading renders as `"—"`; nothing here can panic.
#[allow(clippy::too_many_arguments)]
pub fn cpu_panel(
    ui: &mut egui::Ui,
    theme: &Theme,
    cpu: &CpuInfo,
    history: Option<&RingBuffer>,
    unit: TempUnit,
    show_heat_map: bool,
    capacity: Capacity,
    min_h: f32,
) {
    card(ui, theme, min_h, |ui| {
        panel_title(ui, theme, "CPU", Some(&cpu.name));

        // Load donut on the left, priority-ranked stat rows on the right.
        ui.horizontal(|ui| {
            donut(
                ui,
                theme,
                finite(cpu.load).unwrap_or(0.0) as f32,
                "%",
                "LOAD",
            )
            .on_hover_text(
                "CPU utilisation across every logical core, 0–100 %. \
                     100 % means every core is executing instructions every cycle.",
            );
            ui.vertical(|ui| {
                let mut cands: Vec<StatCandidate> = Vec::new();

                let temp = finite(cpu.temp);
                let temp_value = format_temp(temp, unit);
                let temp_col = temp.map(|t| temp_color(t, TempKind::Processor, theme));
                cands.push(StatCandidate {
                    priority: 0,
                    label: "TEMP",
                    value: temp_value,
                    color: temp_col,
                    tooltip_key: "TEMP",
                    hover_extra: None,
                });

                cands.push(StatCandidate {
                    priority: 1,
                    label: "CLOCK",
                    value: format_clock(cpu.clock_mhz),
                    color: None,
                    tooltip_key: "CLOCK",
                    hover_extra: clock_hover(cpu.bus_clock_mhz),
                });

                cands.push(StatCandidate {
                    priority: 2,
                    label: "POWER",
                    value: format_power(cpu.power_w),
                    color: None,
                    tooltip_key: "POWER",
                    hover_extra: None,
                });

                // TJMAX (Distance to TjMax). Only present when sensord reports
                // the per-core MSR. Lower headroom = closer to throttling, so
                // re-use the temp ramp by treating "headroom remaining" as its
                // inverse: 5 °C of headroom ≈ ~90 °C silicon.
                if let Some(d) = finite(cpu.distance_to_tjmax_c) {
                    let pseudo_temp = (95.0_f64 - d).clamp(0.0, 105.0);
                    let col = temp_color(pseudo_temp, TempKind::Processor, theme);
                    cands.push(StatCandidate {
                        priority: 3,
                        label: "TJMAX",
                        value: format!("{d:.0} \u{00B0}C"),
                        color: Some(col),
                        tooltip_key: "TJMAX",
                        hover_extra: None,
                    });
                }

                if let Some(v) = finite(cpu.voltage_v) {
                    cands.push(StatCandidate {
                        priority: 4,
                        label: "VCORE",
                        value: format!("{v:.3} V"),
                        color: None,
                        tooltip_key: "VCORE",
                        hover_extra: None,
                    });
                }

                render(ui, theme, cands, capacity);
            });
        });

        // Per-core display: strip (default) or heat-map (opt-in). Per-core
        // readings are sanitised here: a non-finite load (or one that
        // overflows the f64→f32 cast) becomes 0, a non-finite temperature
        // becomes absent.
        let cores: Vec<f32> = cpu
            .cores
            .as_ref()
            .map(|c| c.iter().map(|&v| finite_f32(v)).collect())
            .unwrap_or_default();
        if show_heat_map {
            let temps: Vec<Option<f32>> = cpu
                .core_temps
                .as_ref()
                .map(|ts| {
                    ts.iter()
                        .map(|t| t.map(|v| v as f32).filter(|f| f.is_finite()))
                        .collect()
                })
                .unwrap_or_default();
            // Per-core clocks feed the cell hover; non-finite entries degrade
            // to absent like the temperatures above.
            let clocks: Vec<Option<f64>> = cpu
                .core_clocks_mhz
                .as_ref()
                .map(|cs| cs.iter().map(|c| c.filter(|v| v.is_finite())).collect())
                .unwrap_or_default();
            // Clamp the heat-map to what the card can hold while keeping the
            // trailing sparkline visible at its minimum height.
            let grid_max_h =
                (ui.available_height() - SPARKLINE_MIN_HEIGHT - ui.spacing().item_spacing.y)
                    .max(0.0);
            core_grid(
                ui,
                theme,
                &cores,
                &temps,
                &clocks,
                unit,
                cpu.p_core_count.map(|n| n as usize),
                grid_max_h,
            );
        } else {
            core_strip(ui, theme, &cores, cpu.p_core_count.map(|n| n as usize));
        }

        // Load-history sparkline.
        sparkline(ui, theme, samples_or_empty(history), 100.0);
    });
}

/// Narrow an `f64` reading to `f32`, mapping anything non-finite — including
/// magnitudes that only overflow during the cast — to 0.
fn finite_f32(v: f64) -> f32 {
    let f = v as f32;
    if f.is_finite() {
        f
    } else {
        0.0
    }
}

/// Build the multi-line hover-text shown over the POWER row when at least one
/// RAPL sub-domain reading is present. Returns `None` when only the package
/// figure is available — the row keeps its standard tooltip in that case.
///
/// Currently unused: the v0.9.0 priority-ranked renderer does not expose
/// per-row responses, so panels cannot attach extra hover text beyond the
/// stat_priority::render base tooltip. Kept for later wiring.
#[allow(dead_code)]
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

/// MHz rendered as `"4.40 GHz"`, or `"—"` when absent or non-finite.
fn format_clock(clock_mhz: Option<f64>) -> String {
    match finite(clock_mhz) {
        Some(mhz) => format!("{:.2} GHz", mhz / 1000.0),
        None => "—".to_string(),
    }
}

/// Extra hover line for the CLOCK row: the chipset bus (base) clock the core
/// multipliers run against. `None` hides the extra line entirely.
fn clock_hover(bus_clock_mhz: Option<f64>) -> Option<String> {
    finite(bus_clock_mhz).map(|mhz| format!("Bus {mhz:.1} MHz"))
}

/// Watts rendered as whole units, `"52 W"`, or `"—"` when absent or non-finite.
fn format_power(power_w: Option<f64>) -> String {
    match finite(power_w) {
        Some(w) => format!("{} W", w.round() as i64),
        None => "—".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_hover_shows_finite_bus_clock_only() {
        assert_eq!(clock_hover(Some(100.6)), Some("Bus 100.6 MHz".to_string()));
        assert_eq!(clock_hover(None), None);
        assert_eq!(clock_hover(Some(f64::NAN)), None);
    }

    #[test]
    fn clock_and_power_render_em_dash_for_non_finite_input() {
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(format_clock(Some(bad)), "—");
            assert_eq!(format_power(Some(bad)), "—");
        }
        assert_eq!(format_clock(Some(4400.0)), "4.40 GHz");
        assert_eq!(format_power(Some(52.4)), "52 W");
        assert_eq!(format_clock(None), "—");
        assert_eq!(format_power(None), "—");
    }

    #[test]
    fn finite_f32_zeroes_cast_overflow_and_non_finite() {
        assert_eq!(finite_f32(50.0), 50.0);
        assert_eq!(finite_f32(f64::NAN), 0.0);
        assert_eq!(finite_f32(f64::INFINITY), 0.0);
        // Finite as f64 but infinite after the f32 cast.
        assert_eq!(finite_f32(1e300), 0.0);
        assert_eq!(finite_f32(-1e300), 0.0);
    }
}
