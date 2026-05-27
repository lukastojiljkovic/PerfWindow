//! The GPU component panel (also used for integrated GPUs).

use super::{card, panel_title};
use crate::format::{finite, format_bytes_per_sec, format_gb_pair, format_temp, TempUnit};
use crate::history::GpuHistory;
use crate::ipc::GpuInfo;
use crate::theme::Theme;
use crate::ui::capacity::Capacity;
use crate::ui::tooltips::tip;
use crate::widgets::gauge::donut;
use crate::widgets::stat::stat_row;
use crate::widgets::{temp_color, TempKind};

/// Render a GPU card: title, a load donut beside two columns of stat rows,
/// and a dual-line history (compute load and memory controller load) on the
/// same sparkline canvas.
///
/// The card adapts to `gpu.kind`: a `"discrete"` GPU is titled `"GPU"`; an
/// `"integrated"` GPU is titled `"iGPU"` and shows shared system-memory usage
/// instead of dedicated VRAM. Every absent (`None`) reading is *skipped* in
/// the integrated case so the card never carries a wall of em-dashes;
/// discrete GPUs always render the headline rows (TEMP/CLOCK/POWER) even
/// when momentarily absent so the layout stays stable across snapshots.
pub fn gpu_panel(
    ui: &mut egui::Ui,
    theme: &Theme,
    gpu: &GpuInfo,
    history: Option<&GpuHistory>,
    unit: TempUnit,
    capacity: Capacity,
    min_h: f32,
) {
    // Temporary T17 compat: existing body uses a binary "full vs compact"
    // check. T18-T23 will rewrite this block to use StatCandidate + render.
    let full = capacity.rows >= 6;
    let integrated = gpu.kind == "integrated";
    let title = if integrated { "iGPU" } else { "GPU" };

    card(ui, theme, min_h, |ui| {
        panel_title(ui, theme, title, Some(&gpu.name));

        // Load donut on the left, two columns of stat rows on the right.
        ui.horizontal(|ui| {
            let load_resp = donut(ui, theme, gpu.load.unwrap_or(0.0) as f32, "%", "LOAD");
            // Default tooltip — overridden below when D3D engine breakdown is
            // available (which gives a much more actionable explanation than
            // a generic blurb about utilisation).
            if let Some(text) = d3d_breakdown_tooltip(gpu) {
                load_resp.on_hover_text(text);
            } else {
                load_resp.on_hover_text(
                    "GPU compute utilisation, 0–100 %. Reflects the 3D / compute \
                     engine; idle desktops sit close to 0 even with the GPU clocked up.",
                );
            }

            // Decide once which rows have data — kept stable across the two
            // columns so the visual layout is predictable. Discrete GPUs
            // always show TEMP/CLOCK/POWER (the headline trio) even when the
            // current snapshot is missing one; iGPUs hide them when the
            // hardware does not expose the reading.
            let show_temp = !integrated || gpu.temp.is_some();
            let show_clock = finite(gpu.clock_mhz).is_some();
            // HOTSPOT, JUNCTION, PCIE, V are detail readings — kept in Full
            // mode, dropped in Compact so the card fits a narrow window.
            let show_hot_spot = full && finite(gpu.hot_spot_temp).is_some();
            let show_junction = full && finite(gpu.memory_junction_temp_c).is_some();
            let show_mem_use = !integrated || finite(gpu.memory_load).is_some();
            let show_vram = true; // dedicated for discrete, shared for iGPU
            let show_pcie = full && sum_optional(gpu.pcie_rx_bps, gpu.pcie_tx_bps).is_some();
            let show_power = !integrated || finite(gpu.power_w).map(|p| p > 0.05).unwrap_or(false);
            let show_voltage = full && finite(gpu.voltage_v).is_some();

            ui.columns(2, |cols| {
                // -------- Left column: TEMP, HOTSPOT, JUNCTION, CLOCK, V --------
                let left = &mut cols[0];
                if show_temp {
                    let temp_value = format_temp(gpu.temp, unit);
                    let temp_col = gpu.temp.map(|t| temp_color(t, TempKind::Processor, theme));
                    tip(stat_row(left, theme, "TEMP", &temp_value, temp_col), "TEMP");
                }
                if show_hot_spot {
                    let hot = gpu.hot_spot_temp.unwrap();
                    let hot_value = format_temp(Some(hot), unit);
                    let hot_col = Some(temp_color(hot, TempKind::Processor, theme));
                    tip(
                        stat_row(left, theme, "HOTSPOT", &hot_value, hot_col),
                        "HOTSPOT",
                    );
                }
                if show_junction {
                    let junction = gpu.memory_junction_temp_c.unwrap();
                    let val = format_temp(Some(junction), unit);
                    let col = Some(temp_color(junction, TempKind::Processor, theme));
                    tip(stat_row(left, theme, "JUNCTION", &val, col), "JUNCTION");
                }
                if show_clock {
                    tip(
                        stat_row(left, theme, "CLOCK", &format_clock(gpu.clock_mhz), None),
                        "CLOCK",
                    );
                }
                if show_voltage {
                    let v = gpu.voltage_v.unwrap();
                    tip(stat_row(left, theme, "V", &format!("{v:.3} V"), None), "V");
                }

                // -------- Right column: MEM USE, VRAM, PCIE, POWER --------
                let right = &mut cols[1];
                if show_mem_use {
                    tip(
                        stat_row(
                            right,
                            theme,
                            "MEM USE",
                            &format_percent_unit(gpu.memory_load),
                            None,
                        ),
                        "MEM USE",
                    );
                }
                if show_vram {
                    let vram_value = if integrated {
                        format_shared_vram(gpu.shared_vram_used_mb)
                    } else {
                        format_gb_pair(gpu.vram_used_mb, gpu.vram_total_mb)
                    };
                    let vram_resp = stat_row(right, theme, "VRAM", &vram_value, None);
                    let vram_resp = tip(vram_resp, "VRAM");
                    if let Some(text) = vram_breakdown_tooltip(gpu, integrated) {
                        vram_resp.on_hover_text(text);
                    }
                }
                if show_pcie {
                    let total = sum_optional(gpu.pcie_rx_bps, gpu.pcie_tx_bps).unwrap();
                    let val = format_bytes_per_sec(total);
                    let pcie_resp = stat_row(right, theme, "PCIE", &val, None);
                    let pcie_resp = tip(pcie_resp, "PCIE");
                    if let Some(detail) = pcie_breakdown_tooltip(gpu) {
                        pcie_resp.on_hover_text(detail);
                    }
                }
                if show_power {
                    let power_resp =
                        stat_row(right, theme, "POWER", &format_power(gpu.power_w), None);
                    tip(power_resp, "POWER");
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

/// Hover text for the LOAD donut summarising which DXGI engines are busy.
/// Reads `gpu.d3d_engines` (sensord's pre-sorted descending breakdown of
/// "D3D 3D", "D3D Copy", "D3D Video Encode" etc.) and formats every entry
/// with a meaningful (>0.5 %) load. Returns `None` when sensord did not
/// publish a breakdown — older builds, hardware without D3D counters.
fn d3d_breakdown_tooltip(gpu: &GpuInfo) -> Option<String> {
    let engines = gpu.d3d_engines.as_ref()?;
    if engines.is_empty() {
        return None;
    }
    let mut out = String::from("GPU compute utilisation, 0–100 %.\n");
    if let Some(total) = finite(gpu.load) {
        out.push_str(&format!("Overall: {:.0} %\n", total));
    }
    out.push_str("\nPer DXGI engine (idle engines hidden):\n");
    let mut shown = 0;
    for e in engines {
        if e.load < 0.5 {
            continue;
        }
        out.push_str(&format!("  {:<18} {:>5.1} %\n", e.name, e.load));
        shown += 1;
    }
    if shown == 0 {
        out.push_str("  (all engines idle)\n");
    }
    Some(out.trim_end().to_string())
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

/// Render an iGPU's shared-memory usage as `"332 MB shared"`, or the literal
/// `"shared"` when the driver does not report a megabyte figure.
fn format_shared_vram(shared_mb: Option<f64>) -> String {
    match finite(shared_mb) {
        Some(mb) => format!("{} MB shared", mb.round() as i64),
        None => "shared".to_string(),
    }
}

/// `"73 %"`, or `"—"` when absent.
fn format_percent_unit(load: Option<f64>) -> String {
    match load {
        Some(v) if v.is_finite() => format!("{} %", v.round() as i64),
        _ => "—".to_string(),
    }
}

/// MHz rendered as `"4.40 GHz"` once we cross 1 GHz, otherwise `"312 MHz"`.
/// Returns `"—"` when the reading is absent. Used by both CPU and GPU stat
/// rows so the format stays consistent across panels.
fn format_clock(clock_mhz: Option<f64>) -> String {
    match clock_mhz {
        Some(mhz) if mhz >= 1000.0 => format!("{:.2} GHz", mhz / 1000.0),
        Some(mhz) if mhz > 0.0 => format!("{} MHz", mhz.round() as i64),
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
