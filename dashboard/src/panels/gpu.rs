//! The GPU component panel (also used for integrated GPUs).

use super::{card, panel_title};
use crate::format::{format_gb_pair, format_temp, TempUnit};
use crate::history::GpuHistory;
use crate::ipc::GpuInfo;
use crate::theme::Theme;
use crate::widgets::gauge::donut;
use crate::widgets::sparkline::sparkline;
use crate::widgets::stat::stat_row;
use crate::widgets::{temp_color, TempKind};

/// Render a GPU card: title, a load donut beside temp/VRAM/fan stats and a
/// load-history sparkline.
///
/// The card adapts to `gpu.kind`: a `"discrete"` GPU is titled `"GPU"` and
/// shows used/total VRAM, a fan speed and power draw; an `"integrated"` GPU is
/// titled `"iGPU"` and shows `"shared"` VRAM. There is deliberately no per-unit
/// strip — GPUs do not expose per-core loads. Every absent (`None`) reading
/// renders as `"—"`; nothing here can panic.
pub fn gpu_panel(
    ui: &mut egui::Ui,
    theme: &Theme,
    gpu: &GpuInfo,
    history: Option<&GpuHistory>,
    unit: TempUnit,
) {
    let integrated = gpu.kind == "integrated";
    let title = if integrated { "iGPU" } else { "GPU" };

    card(ui, theme, |ui| {
        panel_title(ui, theme, title, Some(&gpu.name));

        // Load donut on the left, four stat rows on the right.
        ui.horizontal(|ui| {
            donut(ui, theme, gpu.load.unwrap_or(0.0) as f32, "%", "LOAD");
            ui.vertical(|ui| {
                let temp_value = format_temp(gpu.temp, unit);
                let temp_col = gpu.temp.map(|t| temp_color(t, TempKind::Processor, theme));
                stat_row(ui, theme, "TEMP", &temp_value, temp_col);

                let vram = if integrated {
                    "shared".to_string()
                } else {
                    format_gb_pair(gpu.vram_used_mb, gpu.vram_total_mb)
                };
                stat_row(ui, theme, "VRAM", &vram, None);

                stat_row(ui, theme, "FAN", &format_fan(gpu.fan_rpm), None);
                stat_row(ui, theme, "POWER", &format_power(gpu.power_w), None);
            });
        });

        // Load-history sparkline (no per-core strip for GPUs).
        let samples: Vec<f32> = history
            .map(|h| h.load.iter_oldest_first().collect())
            .unwrap_or_default();
        sparkline(ui, theme, &samples, 100.0);
    });
}

/// Fan speed rendered as `"1480 rpm"`, or `"—"` when absent.
fn format_fan(fan_rpm: Option<f64>) -> String {
    match fan_rpm {
        Some(rpm) => format!("{} rpm", rpm.round() as i64),
        None => "—".to_string(),
    }
}

/// Power draw rendered as whole watts, `"16 W"`, or `"—"` when absent.
fn format_power(power_w: Option<f64>) -> String {
    match power_w {
        Some(w) => format!("{} W", w.round() as i64),
        None => "—".to_string(),
    }
}
