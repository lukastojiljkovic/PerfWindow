//! The motherboard / fans / voltages component panel.

use super::{card, empty_note, panel_title};
use crate::format::{finite, format_temp, TempUnit};
use crate::ipc::{BoardInfo, FanInfo, VoltageInfo};
use crate::theme::Theme;
use crate::ui::capacity::Capacity;
use crate::ui::tooltips::tip;
use crate::widgets::stat::stat_row;
use crate::widgets::{temp_color, TempKind};
use egui::{Color32, RichText};

/// The fixed-height Sensors card seats at most nine readouts in two columns.
/// The Full capacity tier grew to 11 rows for the GPU card (v0.10.0), so cap
/// locally instead of letting the longer list overflow the card frame.
const MAX_READOUTS: usize = 9;

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

        // Hardware-identity caption (v0.10.0): board model + BIOS version
        // and date in dim small text. Identity alone never summons the card
        // (`has_content` ignores it) — this only decorates a card that is
        // already there for its readouts.
        if let Some(line) = board.and_then(identity_line) {
            ui.label(
                RichText::new(line)
                    .family(theme.font_data.egui())
                    .size(9.0)
                    .color(theme.dim),
            );
        }

        // Build a flat candidate list carrying the raw reading next to its
        // category tag — 0 = temperature, 1 = fan, 2 = voltage — so the
        // rank-and-truncate step below preserves at least one of each kind
        // before falling back to magnitude order within a category. Without
        // it, fans (500–3000) and voltages (1–12) compete on raw value and
        // voltages drop out. Sorting on the raw value avoids the previous
        // per-frame format→parse round-trip in the comparator.
        let mut items: Vec<(u8, f64, String, String, Option<Color32>)> = Vec::new();

        if let Some(b) = board {
            if let Some(t) = finite(b.temp) {
                items.push((
                    0,
                    t,
                    "BOARD".to_string(),
                    format_temp(Some(t), unit),
                    Some(temp_color(t, TempKind::Board, theme)),
                ));
            }
            if let Some(t) = finite(b.vrm_temp) {
                items.push((
                    0,
                    t,
                    "VRM".to_string(),
                    format_temp(Some(t), unit),
                    Some(temp_color(t, TempKind::Board, theme)),
                ));
            }
        }
        for f in fans {
            if let Some(rpm) = finite(f.rpm) {
                items.push((
                    1,
                    rpm,
                    f.name.to_uppercase(),
                    format!("{} RPM", rpm.round() as i64),
                    None,
                ));
            }
        }
        for v in voltages {
            if let Some(volts) = finite(v.volts) {
                items.push((
                    2,
                    volts,
                    v.name.to_uppercase(),
                    format!("{:.2} V", volts),
                    None,
                ));
            }
        }

        if items.is_empty() {
            empty_note(ui, theme, "No motherboard or fan sensors on this machine");
            return;
        }

        // Sort by category ascending then by raw value descending within
        // each category. Truncation drops the smallest voltages last, not
        // all of them — the hottest temp, loudest fan and highest voltage
        // all survive at any capacity tier that holds three rows. Every
        // value passed `finite()` above, so `partial_cmp` cannot fail.
        items.sort_by(|a, b| {
            a.0.cmp(&b.0)
                .then_with(|| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal))
        });
        items.truncate(capacity.rows.min(MAX_READOUTS));

        if capacity.columns >= 2 {
            // First half goes in the left column, second half in the right.
            let split = items.len().div_ceil(2);
            ui.columns(2, |cols| {
                for (_, _, label, val, col) in &items[..split] {
                    tip(stat_row(&mut cols[0], theme, label, val, *col), label);
                }
                for (_, _, label, val, col) in &items[split..] {
                    tip(stat_row(&mut cols[1], theme, label, val, *col), label);
                }
            });
        } else {
            for (_, _, label, val, col) in &items {
                tip(stat_row(ui, theme, label, val, *col), label);
            }
        }
    });
}

/// Compose the hardware-identity caption: `"<board> · BIOS <ver> (<date>)"`
/// with absent parts skipped. The date only renders next to a version — a
/// bare date carries no context. `None` when neither part is present.
fn identity_line(board: &BoardInfo) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    if let Some(name) = board
        .name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        parts.push(name.to_string());
    }
    if let Some(version) = board
        .bios_version
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let mut bios = format!("BIOS {version}");
        if let Some(date) = board
            .bios_date
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            bios.push_str(&format!(" ({date})"));
        }
        parts.push(bios);
    }
    (!parts.is_empty()).then(|| parts.join(" \u{00b7} "))
}

/// Returns `true` iff at least one displayable readout would survive the
/// candidate-collection step in [`sensors_panel`]. Used by the card grid to
/// decide whether to include the Sensors card at all on machines whose
/// Super-I/O chip is unreadable. Identity fields (board name, BIOS) are
/// deliberately ignored: identity alone must not summon the card.
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
            name: None,
            bios_version: None,
            bios_date: None,
        };
        assert!(!has_content(Some(&board), &[], &[]));
    }

    #[test]
    fn has_content_returns_true_for_a_board_temperature() {
        let board = BoardInfo {
            temp: Some(38.0),
            vrm_temp: None,
            name: None,
            bios_version: None,
            bios_date: None,
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
            name: None,
            bios_version: None,
            bios_date: None,
        };
        assert!(!has_content(Some(&board), &[], &[]));
        let fan = FanInfo {
            name: "CPU".into(),
            rpm: Some(f64::NAN),
        };
        assert!(!has_content(None, &[fan], &[]));
    }

    fn identity_board(
        name: Option<&str>,
        bios_version: Option<&str>,
        bios_date: Option<&str>,
    ) -> BoardInfo {
        BoardInfo {
            temp: None,
            vrm_temp: None,
            name: name.map(str::to_string),
            bios_version: bios_version.map(str::to_string),
            bios_date: bios_date.map(str::to_string),
        }
    }

    #[test]
    fn identity_alone_does_not_summon_the_card() {
        let board = identity_board(Some("ASUS FX507VI"), Some("16.0302"), Some("2023-11-15"));
        assert!(!has_content(Some(&board), &[], &[]));
    }

    #[test]
    fn identity_line_combines_board_and_bios() {
        let board = identity_board(Some("ASUS FX507VI"), Some("16.0302"), Some("2023-11-15"));
        assert_eq!(
            identity_line(&board).as_deref(),
            Some("ASUS FX507VI \u{00b7} BIOS 16.0302 (2023-11-15)")
        );
    }

    #[test]
    fn identity_line_skips_absent_parts() {
        assert_eq!(
            identity_line(&identity_board(Some("ASUS FX507VI"), None, None)).as_deref(),
            Some("ASUS FX507VI")
        );
        assert_eq!(
            identity_line(&identity_board(None, Some("16.0302"), None)).as_deref(),
            Some("BIOS 16.0302")
        );
        // A bare date has no context without a version — dropped.
        assert_eq!(
            identity_line(&identity_board(Some("X"), None, Some("2023-11-15"))).as_deref(),
            Some("X")
        );
    }

    #[test]
    fn identity_line_is_none_when_empty_or_blank() {
        assert!(identity_line(&identity_board(None, None, None)).is_none());
        assert!(identity_line(&identity_board(Some("  "), Some(""), None)).is_none());
    }
}
