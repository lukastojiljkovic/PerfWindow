//! The GPU component panel (also used for integrated GPUs).

use super::{card, panel_title};
use crate::format::{finite, format_bytes_per_sec, format_gb_pair, format_temp, TempUnit};
use crate::history::GpuHistory;
use crate::ipc::GpuInfo;
use crate::theme::Theme;
use crate::ui::tooltips::tip;
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

        // Load donut on the left, several stat rows on the right.
        ui.horizontal(|ui| {
            donut(ui, theme, gpu.load.unwrap_or(0.0) as f32, "%", "LOAD").on_hover_text(
                "GPU compute utilisation, 0–100 %. Reflects the 3D / compute \
                     engine; idle desktops sit close to 0 even with the GPU clocked up.",
            );
            ui.vertical(|ui| {
                let temp_value = format_temp(gpu.temp, unit);
                let temp_col = gpu.temp.map(|t| temp_color(t, TempKind::Processor, theme));
                tip(stat_row(ui, theme, "TEMP", &temp_value, temp_col), "TEMP");

                if let Some(hot) = finite(gpu.hot_spot_temp) {
                    let hot_value = format_temp(Some(hot), unit);
                    let hot_col = Some(temp_color(hot, TempKind::Processor, theme));
                    tip(
                        stat_row(ui, theme, "HOTSPOT", &hot_value, hot_col),
                        "HOTSPOT",
                    );
                }

                if let Some(junction) = finite(gpu.memory_junction_temp_c) {
                    let val = format_temp(Some(junction), unit);
                    let col = Some(temp_color(junction, TempKind::Processor, theme));
                    tip(stat_row(ui, theme, "JUNCTION", &val, col), "JUNCTION");
                }

                tip(
                    stat_row(
                        ui,
                        theme,
                        "MEM USE",
                        &format_percent_unit(gpu.memory_load),
                        None,
                    ),
                    "MEM USE",
                );

                // VRAM row. For discrete GPUs we show dedicated MB on the row
                // and a `dedicated + shared` breakdown on hover; iGPUs share
                // system RAM, so the value reads "shared" and the tooltip
                // explains why a precise figure is not meaningful.
                let vram_value = if integrated {
                    "shared".to_string()
                } else {
                    format_gb_pair(gpu.vram_used_mb, gpu.vram_total_mb)
                };
                let vram_resp = stat_row(ui, theme, "VRAM", &vram_value, None);
                let vram_resp = tip(vram_resp, "VRAM");
                if let Some(breakdown) = vram_breakdown_tooltip(gpu, integrated) {
                    vram_resp.on_hover_text(breakdown);
                }

                // PCIe link throughput — sum of Rx and Tx for a single
                // headline number; full breakdown on hover.
                if let Some(total) = sum_optional(gpu.pcie_rx_bps, gpu.pcie_tx_bps) {
                    let val = format_bytes_per_sec(total);
                    let pcie_resp = stat_row(ui, theme, "PCIE", &val, None);
                    let pcie_resp = tip(pcie_resp, "PCIE");
                    if let Some(detail) = pcie_breakdown_tooltip(gpu) {
                        pcie_resp.on_hover_text(detail);
                    }
                }

                tip(
                    stat_row(ui, theme, "POWER", &format_power(gpu.power_w), None),
                    "POWER",
                );

                if let Some(v) = finite(gpu.voltage_v) {
                    tip(stat_row(ui, theme, "V", &format!("{v:.3} V"), None), "V");
                }
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

/// Sum two optional readings into a single `Option<f64>`. Returns the value
/// when at least one operand is present; `None` only when both are absent.
fn sum_optional(a: Option<f64>, b: Option<f64>) -> Option<f64> {
    match (a, b) {
        (Some(x), Some(y)) => Some(x + y),
        (Some(x), None) | (None, Some(x)) => Some(x),
        (None, None) => None,
    }
}

/// Hover text for the PCIe row when at least one direction has data.
fn pcie_breakdown_tooltip(gpu: &GpuInfo) -> Option<String> {
    let rx = finite(gpu.pcie_rx_bps);
    let tx = finite(gpu.pcie_tx_bps);
    if rx.is_none() && tx.is_none() {
        return None;
    }
    let mut out = String::from("PCIe link throughput, bytes per second:\n");
    if let Some(r) = rx {
        out.push_str(&format!("  Receive   {}\n", format_bytes_per_sec(r)));
    }
    if let Some(t) = tx {
        out.push_str(&format!("  Transmit  {}\n", format_bytes_per_sec(t)));
    }
    Some(out.trim_end().to_string())
}

/// Hover text for the VRAM row when the driver exposes dedicated / shared
/// breakdown values. `integrated` shifts the language for iGPUs that have no
/// dedicated VRAM at all.
fn vram_breakdown_tooltip(gpu: &GpuInfo, integrated: bool) -> Option<String> {
    let dedicated = finite(gpu.dedicated_vram_used_mb);
    let shared = finite(gpu.shared_vram_used_mb);
    if dedicated.is_none() && shared.is_none() {
        return None;
    }
    let mut out = if integrated {
        String::from(
            "iGPU uses system RAM through DXGI; both figures are MB of system memory mapped to the GPU.\n",
        )
    } else {
        String::from(
            "Discrete GPU memory split between on-card VRAM and DXGI-shared system RAM (MB):\n",
        )
    };
    if let Some(v) = dedicated {
        out.push_str(&format!("  Dedicated   {:>6.0} MB\n", v));
    }
    if let Some(v) = shared {
        out.push_str(&format!("  Shared      {:>6.0} MB\n", v));
    }
    Some(out.trim_end().to_string())
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
