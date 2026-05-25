//! The GPU component panel (also used for integrated GPUs).

use super::{card, panel_title};
use crate::format::{finite, format_gb_pair, format_temp, TempUnit};
use crate::history::GpuHistory;
use crate::ipc::GpuInfo;
use crate::theme::Theme;
use crate::widgets::gauge::donut;
use crate::widgets::stat::stat_row;
use crate::widgets::{temp_color, TempKind};

/// Render a GPU card: title, a load donut beside temp/memory/fan/power stats,
/// and a dual-line history (compute load and memory controller load) on the
/// same sparkline canvas.
///
/// The card adapts to `gpu.kind`: a `"discrete"` GPU is titled `"GPU"`; an
/// `"integrated"` GPU is titled `"iGPU"` and shows `"shared"` VRAM. Every
/// absent (`None`) reading renders as `"—"`; nothing here can panic.
pub fn gpu_panel(
    ui: &mut egui::Ui,
    theme: &Theme,
    gpu: &GpuInfo,
    history: Option<&GpuHistory>,
    unit: TempUnit,
    min_h: f32,
) {
    let integrated = gpu.kind == "integrated";
    let title = if integrated { "iGPU" } else { "GPU" };

    card(ui, theme, min_h, |ui| {
        panel_title(ui, theme, title, Some(&gpu.name));

        // Load donut on the left, five stat rows on the right.
        ui.horizontal(|ui| {
            donut(ui, theme, gpu.load.unwrap_or(0.0) as f32, "%", "LOAD");
            ui.vertical(|ui| {
                let temp_value = format_temp(gpu.temp, unit);
                let temp_col = gpu.temp.map(|t| temp_color(t, TempKind::Processor, theme));
                stat_row(ui, theme, "TEMP", &temp_value, temp_col);

                if let Some(hot) = finite(gpu.hot_spot_temp) {
                    let hot_value = format_temp(Some(hot), unit);
                    let hot_col = Some(temp_color(hot, TempKind::Processor, theme));
                    stat_row(ui, theme, "HOTSPOT", &hot_value, hot_col);
                }

                stat_row(
                    ui,
                    theme,
                    "MEM USE",
                    &format_percent_unit(gpu.memory_load),
                    None,
                );

                let vram = if integrated {
                    "shared".to_string()
                } else {
                    format_gb_pair(gpu.vram_used_mb, gpu.vram_total_mb)
                };
                stat_row(ui, theme, "VRAM", &vram, None);

                stat_row(ui, theme, "POWER", &format_power(gpu.power_w), None);
            });
        });

        // Legend strip (only when there is a memory series to label).
        let memory_samples: Vec<f32> = history
            .map(|h| h.memory_load.iter_oldest_first().collect())
            .unwrap_or_default();
        if memory_samples.iter().any(|&v| v > 0.0) {
            dual_legend(ui, theme);
        }

        // Load + memory-load history.
        let load_samples: Vec<f32> = history
            .map(|h| h.load.iter_oldest_first().collect())
            .unwrap_or_default();
        crate::widgets::sparkline::dual_sparkline(ui, theme, &load_samples, &memory_samples, 100.0);
    });
}

/// `"73 %"`, or `"—"` when absent.
fn format_percent_unit(load: Option<f64>) -> String {
    match load {
        Some(v) if v.is_finite() => format!("{} %", v.round() as i64),
        _ => "—".to_string(),
    }
}

/// Two-square legend immediately above the dual sparkline: `■ LOAD ◧ MEMORY`.
fn dual_legend(ui: &mut egui::Ui, theme: &Theme) {
    use crate::format::letter_spaced;
    use egui::RichText;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.label(
            RichText::new("\u{25A0} ")
                .family(theme.font_data.egui())
                .size(10.0)
                .color(theme.accent),
        );
        ui.label(
            RichText::new(letter_spaced("LOAD"))
                .family(theme.font_display.egui())
                .size(9.0)
                .color(theme.dim),
        );
        ui.add_space(8.0);
        ui.label(
            RichText::new("\u{25A7} ")
                .family(theme.font_data.egui())
                .size(10.0)
                .color(theme.accent.gamma_multiply(0.45)),
        );
        ui.label(
            RichText::new(letter_spaced("MEMORY"))
                .family(theme.font_display.egui())
                .size(9.0)
                .color(theme.dim),
        );
    });
}

/// Power draw rendered as whole watts, `"16 W"`, or `"—"` when absent.
fn format_power(power_w: Option<f64>) -> String {
    match power_w {
        Some(w) => format!("{} W", w.round() as i64),
        None => "—".to_string(),
    }
}
