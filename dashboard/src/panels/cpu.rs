//! The CPU component panel.

use super::{card, panel_title};
use crate::format::{finite, format_temp, TempUnit};
use crate::history::RingBuffer;
use crate::ipc::CpuInfo;
use crate::theme::Theme;
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

        // Load donut on the left, three stat rows on the right.
        ui.horizontal(|ui| {
            donut(ui, theme, cpu.load.unwrap_or(0.0) as f32, "%", "LOAD");
            ui.vertical(|ui| {
                let temp_value = format_temp(cpu.temp, unit);
                let temp_col = cpu.temp.map(|t| temp_color(t, TempKind::Processor, theme));
                stat_row(ui, theme, "TEMP", &temp_value, temp_col);

                stat_row(ui, theme, "CLOCK", &format_clock(cpu.clock_mhz), None);
                stat_row(ui, theme, "POWER", &format_power(cpu.power_w), None);
                if let Some(v) = finite(cpu.voltage_v) {
                    stat_row(ui, theme, "VCORE", &format!("{v:.3} V"), None);
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
            core_grid(ui, theme, &cores, &temps, unit);
        } else {
            core_strip(ui, theme, &cores);
        }

        // Load-history sparkline.
        let samples: Vec<f32> = history
            .map(|h| h.iter_oldest_first().collect())
            .unwrap_or_default();
        sparkline(ui, theme, &samples, 100.0);
    });
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
