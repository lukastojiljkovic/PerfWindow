//! The motherboard / fans / voltages component panel.

use super::{card, empty_note, panel_title};
use crate::format::{finite, format_temp, TempUnit};
use crate::ipc::{BoardInfo, FanInfo, VoltageInfo};
use crate::theme::{FontFamily, Theme};
use crate::widgets::{temp_color, TempKind};
use egui::{Color32, FontId, Pos2, Sense, Stroke, Vec2};

/// Height of one readout row (mockup `.pw-ro` line over a 5 px rule margin).
const READOUT_ROW_H: f32 = 20.0;
/// Horizontal gap between the two readout columns (mockup `.pw-readout { gap: 14px }`).
const COL_GAP: f32 = 14.0;
/// Vertical gap between readout rows (mockup `.pw-readout { gap: 8px }`).
const ROW_GAP: f32 = 8.0;

/// One `label : value` readout row, with the value optionally tinted.
struct Readout {
    label: String,
    value: String,
    /// `None` falls back to `theme.ink`; `Some` is used for temperature tints.
    color: Option<Color32>,
}

/// Render the BOARD & SENSORS card: a two-column grid of motherboard
/// temperatures, fan speeds and voltage readouts.
///
/// Only readings that are actually present are shown. When the machine exposes
/// nothing at all — the common case on laptops with no readable Super-I/O chip
/// — a single dimmed line is drawn instead. Nothing here fabricates values or
/// panics.
pub fn sensors_panel(
    ui: &mut egui::Ui,
    theme: &Theme,
    board: Option<&BoardInfo>,
    fans: &[FanInfo],
    voltages: &[VoltageInfo],
    unit: TempUnit,
    min_h: f32,
) {
    card(ui, theme, min_h, |ui| {
        panel_title(ui, theme, "BOARD & SENSORS", None);

        let rows = collect_readouts(theme, board, fans, voltages, unit);

        if rows.is_empty() {
            empty_note(ui, theme, "No motherboard or fan sensors on this machine");
        } else {
            readout_grid(ui, theme, &rows);
        }
    });
}

/// Returns `true` iff at least one displayable readout would be produced by
/// [`collect_readouts`]. Used by the card grid to decide whether to include
/// the Sensors card at all on machines whose Super-I/O chip is unreadable.
pub fn has_content(board: Option<&BoardInfo>, fans: &[FanInfo], voltages: &[VoltageInfo]) -> bool {
    if let Some(board) = board {
        if finite(board.temp).is_some() || finite(board.vrm_temp).is_some() {
            return true;
        }
    }
    if fans.iter().any(|f| finite(f.rpm).is_some()) {
        return true;
    }
    if voltages.iter().any(|v| finite(v.volts).is_some()) {
        return true;
    }
    false
}

/// Gather every present sensor reading into an ordered list of readout rows:
/// board temperature, VRM temperature, fans, then voltages.
fn collect_readouts(
    theme: &Theme,
    board: Option<&BoardInfo>,
    fans: &[FanInfo],
    voltages: &[VoltageInfo],
    unit: TempUnit,
) -> Vec<Readout> {
    let mut rows: Vec<Readout> = Vec::new();

    if let Some(board) = board {
        if let Some(temp) = finite(board.temp) {
            rows.push(Readout {
                label: "TEMP".to_string(),
                value: format_temp(Some(temp), unit),
                color: Some(temp_color(temp, TempKind::Board, theme)),
            });
        }
        if let Some(vrm) = finite(board.vrm_temp) {
            rows.push(Readout {
                label: "VRM".to_string(),
                value: format_temp(Some(vrm), unit),
                color: Some(temp_color(vrm, TempKind::Board, theme)),
            });
        }
    }

    for fan in fans {
        if let Some(rpm) = finite(fan.rpm) {
            rows.push(Readout {
                label: fan.name.clone(),
                value: format!("{}", rpm.round() as i64),
                color: None,
            });
        }
    }

    for voltage in voltages {
        if let Some(volts) = finite(voltage.volts) {
            rows.push(Readout {
                label: voltage.name.clone(),
                value: format!("{:.2}", volts),
                color: None,
            });
        }
    }

    rows
}

/// Draw the two-column readout grid: dim label left, bold value right, with a
/// 1 px rule under each row (mockup `.pw-readout` / `.pw-ro`).
fn readout_grid(ui: &mut egui::Ui, theme: &Theme, rows: &[Readout]) {
    let total_w = ui.available_width();
    let col_w = ((total_w - COL_GAP) / 2.0).max(0.0);

    let row_count = rows.len().div_ceil(2);
    let grid_h = row_count as f32 * READOUT_ROW_H + (row_count.saturating_sub(1)) as f32 * ROW_GAP;

    let (rect, _) = ui.allocate_exact_size(Vec2::new(total_w, grid_h), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter_at(rect);

    let label_font = FontId::new(11.0, theme.font_data.egui());
    // The value is the data font's bold sibling where one exists (mockup
    // `.pw-ro b { font-weight: 600 }`).
    let value_family = match theme.font_data {
        FontFamily::PlexMono => FontFamily::PlexMonoBold,
        other => other,
    };
    let value_font = FontId::new(11.0, value_family.egui());

    for (i, row) in rows.iter().enumerate() {
        let col = i % 2;
        let line = i / 2;

        let cell_x0 = rect.min.x + col as f32 * (col_w + COL_GAP);
        let cell_y0 = rect.min.y + line as f32 * (READOUT_ROW_H + ROW_GAP);
        // Text baseline area sits above the 5 px rule margin.
        let text_h = READOUT_ROW_H - 5.0;
        let center_y = cell_y0 + text_h / 2.0;

        // Label, left-aligned, dim.
        let label_galley = painter.layout_no_wrap(row.label.clone(), label_font.clone(), theme.dim);
        painter.galley(
            Pos2::new(cell_x0, center_y - label_galley.size().y / 2.0),
            label_galley,
            theme.dim,
        );

        // Value, right-aligned within the cell.
        let value_color = row.color.unwrap_or(theme.ink);
        let value_galley =
            painter.layout_no_wrap(row.value.clone(), value_font.clone(), value_color);
        painter.galley(
            Pos2::new(
                cell_x0 + col_w - value_galley.size().x,
                center_y - value_galley.size().y / 2.0,
            ),
            value_galley,
            value_color,
        );

        // 1 px bottom rule across the cell.
        let rule_y = cell_y0 + READOUT_ROW_H - 1.0;
        painter.add(egui::Shape::line_segment(
            [
                Pos2::new(cell_x0, rule_y),
                Pos2::new(cell_x0 + col_w, rule_y),
            ],
            Stroke::new(1.0, theme.border),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::{BoardInfo, FanInfo, VoltageInfo};

    #[test]
    fn has_content_returns_false_when_everything_is_absent_or_none() {
        assert!(!has_content(None, &[], &[]));
        let board = BoardInfo {
            temp: None,
            vrm_temp: None,
        };
        assert!(!has_content(Some(&board), &[], &[]));
    }

    #[test]
    fn has_content_returns_true_for_a_board_temperature() {
        let board = BoardInfo {
            temp: Some(38.0),
            vrm_temp: None,
        };
        assert!(has_content(Some(&board), &[], &[]));
    }

    #[test]
    fn has_content_returns_true_for_a_single_fan() {
        let fan = FanInfo {
            name: "CPU".into(),
            rpm: Some(920.0),
        };
        assert!(has_content(None, &[fan], &[]));
    }

    #[test]
    fn has_content_returns_true_for_a_single_voltage() {
        let v = VoltageInfo {
            name: "+12V".into(),
            volts: Some(12.0),
        };
        assert!(has_content(None, &[], &[v]));
    }

    #[test]
    fn has_content_ignores_non_finite_values() {
        let board = BoardInfo {
            temp: Some(f64::NAN),
            vrm_temp: None,
        };
        assert!(!has_content(Some(&board), &[], &[]));
        let fan = FanInfo {
            name: "CPU".into(),
            rpm: Some(f64::NAN),
        };
        assert!(!has_content(None, &[fan], &[]));
    }
}
