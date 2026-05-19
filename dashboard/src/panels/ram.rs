//! The RAM component panel.

use super::{card, panel_title};
use crate::format::{format_gb_from_mb, format_gb_pair};
use crate::history::RingBuffer;
use crate::ipc::RamInfo;
use crate::theme::Theme;
use crate::widgets::bars::bar_meter;
use crate::widgets::gauge::donut;
use crate::widgets::sparkline::sparkline;
use crate::widgets::stat::stat_row;
use egui::{FontId, Sense, Vec2};

/// Height of the SWAP row (matches the mockup's `.pw-swap { height: 24px }`).
const SWAP_ROW_H: f32 = 24.0;

/// Render the RAM card: title, a usage donut beside used/free/cached stats, a
/// pagefile (SWAP) bar and a usage-history sparkline.
///
/// When no pagefile is configured (`pagefile_total_mb` absent or zero) the
/// whole SWAP row is rendered in a dimmed, inactive state. Every absent
/// (`None`) reading degrades gracefully; nothing here can panic.
pub fn ram_panel(ui: &mut egui::Ui, theme: &Theme, ram: &RamInfo, history: Option<&RingBuffer>) {
    card(ui, theme, |ui| {
        panel_title(ui, theme, "RAM", None);

        // Usage donut on the left, three stat rows on the right.
        ui.horizontal(|ui| {
            donut(ui, theme, ram.load.unwrap_or(0.0) as f32, "%", "USED");
            ui.vertical(|ui| {
                stat_row(ui, theme, "USED", &gb(ram.used_mb), None);
                stat_row(ui, theme, "FREE", &gb(ram.available_mb), None);
                stat_row(ui, theme, "CACHED", &gb(ram.cached_mb), None);
            });
        });

        // SWAP / pagefile row.
        swap_row(ui, theme, ram.pagefile_used_mb, ram.pagefile_total_mb);

        // Usage-history sparkline.
        let samples: Vec<f32> = history
            .map(|h| h.iter_oldest_first().collect())
            .unwrap_or_default();
        sparkline(ui, theme, &samples, 100.0);
    });
}

/// `format_gb_from_mb` with the `" GB"` suffix appended; a bare `"—"` when the
/// value is absent.
fn gb(mb: Option<f64>) -> String {
    match mb {
        Some(_) => format!("{} GB", format_gb_from_mb(mb)),
        None => "—".to_string(),
    }
}

/// Draw the SWAP row: a `SWAP` label, a pagefile fill bar taking the middle and
/// a `"<used> / <total> GB"` value on the right.
///
/// With no pagefile configured the label, bar and value are all dimmed
/// (`theme.dim` / `theme.track`) to signal the inactive state.
fn swap_row(ui: &mut egui::Ui, theme: &Theme, used_mb: Option<f64>, total_mb: Option<f64>) {
    let total = total_mb.unwrap_or(0.0);
    let active = total > 0.0;
    let used = used_mb.unwrap_or(0.0);

    // Active label is `dim` per the mockup; inactive drops it to `faint`.
    let label_color = if active { theme.dim } else { theme.faint };
    let value_color = if active { theme.ink } else { theme.dim };
    let label_font = FontId::new(9.0, theme.font_data.egui());
    let value_font = FontId::new(10.0, theme.font_data.egui());

    let value_text = if active {
        format_gb_pair(used_mb, total_mb)
    } else {
        "—".to_string()
    };

    ui.horizontal(|ui| {
        // Pin the whole row to a consistent height so the bar centres cleanly.
        ui.set_min_height(SWAP_ROW_H);

        // Left: SWAP label.
        ui.label(
            egui::RichText::new("SWAP")
                .font(label_font)
                .color(label_color),
        );

        // Right: the value, then (laid out leftward) the bar fills the gap.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(value_text)
                    .font(value_font)
                    .color(value_color),
            );

            if active {
                let fraction = (used / total).clamp(0.0, 1.0) as f32;
                let warn = fraction >= 0.9;
                bar_meter(ui, theme, fraction, warn);
            } else {
                // Inactive: an empty track-coloured bar, no fill.
                let w = ui.available_width();
                let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 6.0), Sense::hover());
                if ui.is_rect_visible(rect) {
                    ui.painter().rect_filled(rect, 0.0, theme.track);
                }
            }
        });
    });
}
