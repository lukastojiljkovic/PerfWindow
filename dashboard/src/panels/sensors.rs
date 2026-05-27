//! The motherboard / fans / voltages component panel.

use super::{card, empty_note, panel_title};
use crate::format::{finite, format_temp, TempUnit};
use crate::ipc::{BoardInfo, FanInfo, VoltageInfo};
use crate::theme::Theme;
use crate::ui::capacity::Capacity;
use crate::ui::tooltips::tip;
use crate::widgets::stat::stat_row;
use crate::widgets::{temp_color, TempKind};
use egui::Color32;

/// Render the BOARD & SENSORS card: a flat list of motherboard temperatures,
/// fan speeds and voltage readouts, ranked by raw magnitude (hottest /
/// loudest / highest first) and trimmed to the per-card `Capacity` budget.
///
/// In wide capacity tiers the list is split into two columns; in the
/// narrowest tier it stacks. Only readings that are actually present are
/// shown. When the machine exposes nothing at all — the common case on
/// laptops with no readable Super-I/O chip — a single dimmed line is drawn
/// instead. Nothing here fabricates values or panics.
#[allow(clippy::too_many_arguments)]
pub fn sensors_panel(
    ui: &mut egui::Ui,
    theme: &Theme,
    board: Option<&BoardInfo>,
    fans: &[FanInfo],
    voltages: &[VoltageInfo],
    unit: TempUnit,
    capacity: Capacity,
    min_h: f32,
) {
    card(ui, theme, min_h, |ui| {
        panel_title(ui, theme, "BOARD & SENSORS", None);

        // Build a flat candidate list of (label, value, optional value tint).
        let mut items: Vec<(String, String, Option<Color32>)> = Vec::new();

        if let Some(b) = board {
            if let Some(t) = finite(b.temp) {
                items.push((
                    "BOARD".to_string(),
                    format_temp(Some(t), unit),
                    Some(temp_color(t, TempKind::Board, theme)),
                ));
            }
            if let Some(t) = finite(b.vrm_temp) {
                items.push((
                    "VRM".to_string(),
                    format_temp(Some(t), unit),
                    Some(temp_color(t, TempKind::Board, theme)),
                ));
            }
        }
        for f in fans {
            if let Some(rpm) = finite(f.rpm) {
                items.push((
                    f.name.to_uppercase(),
                    format!("{} RPM", rpm.round() as i64),
                    None,
                ));
            }
        }
        for v in voltages {
            if let Some(volts) = finite(v.volts) {
                items.push((v.name.to_uppercase(), format!("{:.2} V", volts), None));
            }
        }

        if items.is_empty() {
            empty_note(ui, theme, "No motherboard or fan sensors on this machine");
            return;
        }

        // Sort by the leading numeric value descending so the hottest temp,
        // the loudest fan and the highest voltage bubble to the top of their
        // category; once truncated to the row budget the most informative
        // readings survive at every capacity tier.
        items.sort_by(|a, b| {
            let av = parse_leading_number(&a.1);
            let bv = parse_leading_number(&b.1);
            bv.partial_cmp(&av).unwrap_or(std::cmp::Ordering::Equal)
        });
        items.truncate(capacity.rows);

        if capacity.columns >= 2 {
            // First half goes in the left column, second half in the right —
            // mirrors the v0.8.x two-up layout but with a ranked-and-trimmed
            // candidate set instead of every reading.
            let split = items.len().div_ceil(2);
            ui.columns(2, |cols| {
                for (label, val, col) in &items[..split] {
                    tip(stat_row(&mut cols[0], theme, label, val, *col), label);
                }
                for (label, val, col) in &items[split..] {
                    tip(stat_row(&mut cols[1], theme, label, val, *col), label);
                }
            });
        } else {
            for (label, val, col) in &items {
                tip(stat_row(ui, theme, label, val, *col), label);
            }
        }
    });
}

/// Returns `true` iff at least one displayable readout would survive the
/// candidate-collection step in [`sensors_panel`]. Used by the card grid to
/// decide whether to include the Sensors card at all on machines whose
/// Super-I/O chip is unreadable.
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

/// Parse the leading whitespace-separated token of `s` as `f64`, falling back
/// to `0.0` when no number is present. Used to rank readouts that carry
/// trailing unit strings (`"55 °C"`, `"1200 RPM"`, `"12.00 V"`) by their raw
/// magnitude.
fn parse_leading_number(s: &str) -> f64 {
    s.split_whitespace()
        .next()
        .and_then(|t| t.parse::<f64>().ok())
        .unwrap_or(0.0)
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
