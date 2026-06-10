//! The RAM component panel.

use super::{card, panel_title};
use crate::format::{finite, format_gb_from_mb, format_gb_pair, format_temp, TempUnit};
use crate::history::{samples_or_empty, RingBuffer};
use crate::ipc::snapshot::{DimmTemp, RamModule};
use crate::ipc::RamInfo;
use crate::theme::Theme;
use crate::ui::capacity::Capacity;
use crate::ui::stat_priority::{render, StatCandidate};
use crate::widgets::bars::bar_meter;
use crate::widgets::gauge::donut;
use crate::widgets::sparkline::sparkline;
use crate::widgets::{temp_color, TempKind};
use egui::{FontId, Sense, Vec2};

/// Height of the SWAP row.
const SWAP_ROW_H: f32 = 24.0;

/// Render the RAM card: title, a usage donut beside used/free/cached stats, a
/// pagefile (SWAP) bar and a usage-history sparkline.
///
/// When no pagefile is configured (`pagefile_total_mb` absent or zero) the
/// whole SWAP row is rendered in a dimmed, inactive state. Every absent
/// (`None`) reading degrades gracefully; nothing here can panic.
pub fn ram_panel(
    ui: &mut egui::Ui,
    theme: &Theme,
    ram: &RamInfo,
    history: Option<&RingBuffer>,
    unit: TempUnit,
    capacity: Capacity,
    min_h: f32,
) {
    card(ui, theme, min_h, |ui| {
        panel_title(ui, theme, "RAM", None);

        // Usage donut on the left, priority-ranked stat rows on the right.
        ui.horizontal(|ui| {
            donut(
                ui,
                theme,
                finite(ram.load).unwrap_or(0.0) as f32,
                "%",
                "USED",
            )
            .on_hover_text(
                "Physical RAM in use, 0–100 %. Cache is counted as free — Windows \
                 reclaims it on demand.",
            );
            ui.vertical(|ui| {
                let mut cands: Vec<StatCandidate> = Vec::new();

                cands.push(StatCandidate {
                    priority: 0,
                    label: "USED",
                    value: gb(ram.used_mb),
                    color: None,
                    tooltip_key: "USED",
                    hover_extra: None,
                });

                cands.push(StatCandidate {
                    priority: 1,
                    label: "FREE",
                    value: gb(ram.available_mb),
                    color: None,
                    tooltip_key: "FREE",
                    hover_extra: None,
                });

                cands.push(StatCandidate {
                    priority: 2,
                    label: "CACHED",
                    value: gb(ram.cached_mb),
                    color: None,
                    tooltip_key: "CACHED",
                    hover_extra: None,
                });

                // DIMM temperature — surface the hottest module. Only
                // present when the SPD hub exposes a thermal sensor (DDR5;
                // most DDR4 desktop kits). Older sensord builds and older
                // hardware skip it. With no finite reading at all the row is
                // hidden rather than folding garbage into a fake maximum.
                // Hovering reveals per-module identity (label, capacity,
                // temperature, timings) when sensord publishes `modules`.
                if let Some((hottest, finite_count)) =
                    hottest_dimm(ram.dimm_temps.as_deref().unwrap_or(&[]))
                {
                    let label = if finite_count > 1 { "DIMM MAX" } else { "DIMM" };
                    cands.push(StatCandidate {
                        priority: 3,
                        label,
                        value: format_temp(Some(hottest), unit),
                        color: Some(temp_color(hottest, TempKind::Processor, theme)),
                        tooltip_key: "DIMM",
                        hover_extra: dimm_modules_hover(
                            ram.modules.as_deref().unwrap_or(&[]),
                            unit,
                        ),
                    });
                }

                render(ui, theme, cands, capacity);
            });
        });

        // SWAP / pagefile row.
        swap_row(ui, theme, ram.pagefile_used_mb, ram.pagefile_total_mb);

        // Usage-history sparkline.
        sparkline(ui, theme, samples_or_empty(history), 100.0);
    });
}

/// `format_gb_from_mb` with the `" GB"` suffix appended; a bare `"—"` when the
/// value is absent or non-finite.
fn gb(mb: Option<f64>) -> String {
    match finite(mb) {
        Some(_) => format!("{} GB", format_gb_from_mb(mb)),
        None => "—".to_string(),
    }
}

/// Hottest finite DIMM reading plus the count of finite readings. `None` when
/// every reading is absent or non-finite, so an all-NaN snapshot hides the
/// row instead of rendering a `f64::MIN` fold as `"-9223372036854775808°"`.
fn hottest_dimm(dimms: &[DimmTemp]) -> Option<(f64, usize)> {
    let mut hottest: Option<f64> = None;
    let mut finite_count = 0usize;
    for t in dimms.iter().map(|d| d.temp_c).filter(|t| t.is_finite()) {
        finite_count += 1;
        hottest = Some(hottest.map_or(t, |m| m.max(t)));
    }
    hottest.map(|m| (m, finite_count))
}

/// Hover detail for the DIMM row: one `label · capacity · temp · timings`
/// line per installed module, absent parts skipped. Returns `None` when no
/// module yields a single displayable part — the row then falls back to the
/// static DIMM tooltip.
fn dimm_modules_hover(modules: &[RamModule], unit: TempUnit) -> Option<String> {
    let mut out = String::from("Installed memory modules:");
    let mut any = false;
    for m in modules {
        let mut parts: Vec<String> = Vec::new();
        let label = m.label.trim();
        if !label.is_empty() {
            parts.push(label.to_string());
        }
        if let Some(gb) = finite(m.capacity_gb) {
            parts.push(format!("{gb:.0} GB"));
        }
        if let Some(t) = finite(m.temp_c) {
            parts.push(format_temp(Some(t), unit));
        }
        if let Some(timings) = m
            .timings
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
        {
            parts.push(timings.to_string());
        }
        if !parts.is_empty() {
            out.push_str("\n  ");
            out.push_str(&parts.join(" \u{00b7} "));
            any = true;
        }
    }
    any.then_some(out)
}

/// Draw the SWAP row: a `SWAP` label, a pagefile fill bar taking the middle and
/// a `"<used> / <total> GB"` value on the right.
///
/// With no pagefile configured the label, bar and value are all dimmed
/// (`theme.dim` / `theme.track`) to signal the inactive state.
fn swap_row(ui: &mut egui::Ui, theme: &Theme, used_mb: Option<f64>, total_mb: Option<f64>) {
    let used_mb = finite(used_mb);
    let total_mb = finite(total_mb);
    let total = total_mb.unwrap_or(0.0);
    let active = total > 0.0;
    let used = used_mb.unwrap_or(0.0);

    // Active label is `dim`; inactive drops it to `faint`.
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
                // Inactive: an empty track-coloured bar, no fill. Same
                // degenerate-width guard as `widgets::bars::bar_meter`.
                let w = ui.available_width();
                if w.is_finite() && w > 0.0 {
                    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 6.0), Sense::hover());
                    if ui.is_rect_visible(rect) {
                        ui.painter().rect_filled(rect, 0.0, theme.track);
                    }
                }
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gb_renders_em_dash_for_absent_or_non_finite_values() {
        assert_eq!(gb(Some(15462.0)), "15.1 GB");
        assert_eq!(gb(None), "—");
        assert_eq!(gb(Some(f64::NAN)), "—");
        assert_eq!(gb(Some(f64::INFINITY)), "—");
    }

    fn dimm(temp_c: f64) -> DimmTemp {
        DimmTemp {
            label: "DIMM".into(),
            temp_c,
        }
    }

    #[test]
    fn hottest_dimm_picks_the_max_finite_reading() {
        let dimms = [dimm(41.0), dimm(44.5), dimm(39.0)];
        assert_eq!(hottest_dimm(&dimms), Some((44.5, 3)));
    }

    #[test]
    fn hottest_dimm_skips_non_finite_readings() {
        let dimms = [dimm(f64::NAN), dimm(42.0), dimm(f64::INFINITY)];
        assert_eq!(hottest_dimm(&dimms), Some((42.0, 1)));
    }

    #[test]
    fn hottest_dimm_is_none_when_all_readings_are_garbage() {
        let dimms = [dimm(f64::NAN), dimm(f64::NEG_INFINITY)];
        assert_eq!(hottest_dimm(&dimms), None);
        assert_eq!(hottest_dimm(&[]), None);
    }

    fn module(
        label: &str,
        capacity_gb: Option<f64>,
        temp_c: Option<f64>,
        timings: Option<&str>,
    ) -> RamModule {
        RamModule {
            label: label.into(),
            capacity_gb,
            temp_c,
            timings: timings.map(str::to_string),
        }
    }

    #[test]
    fn dimm_modules_hover_lists_every_part() {
        let modules = [module(
            "Kingston KF556S40 #0",
            Some(16.0),
            Some(44.5),
            Some("CL40-39-39 @ 5600 MT/s"),
        )];
        let hover = dimm_modules_hover(&modules, TempUnit::Celsius).unwrap();
        assert!(hover.contains(
            "Kingston KF556S40 #0 \u{00b7} 16 GB \u{00b7} 45° \u{00b7} CL40-39-39 @ 5600 MT/s"
        ));
    }

    #[test]
    fn dimm_modules_hover_skips_absent_and_non_finite_parts() {
        let modules = [module("DIMM #1", None, Some(f64::NAN), None)];
        let hover = dimm_modules_hover(&modules, TempUnit::Celsius).unwrap();
        assert!(hover.contains("\n  DIMM #1"));
        assert!(!hover.contains('\u{00b7}'), "no separator for a lone label");
    }

    #[test]
    fn dimm_modules_hover_is_none_without_displayable_modules() {
        assert_eq!(dimm_modules_hover(&[], TempUnit::Celsius), None);
        let blank = [module("   ", None, None, Some("  "))];
        assert_eq!(dimm_modules_hover(&blank, TempUnit::Celsius), None);
    }
}
