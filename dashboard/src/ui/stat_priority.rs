//! Priority-ranked stat row rendering.
//!
//! Panels build a candidate list (each entry carries its own priority, label,
//! value, optional color and tooltip key). [`render`] sorts by priority,
//! truncates to `capacity.rows`, then lays the survivors out in either a
//! single column (`capacity.columns == 1`) or a 2-column split (otherwise).
//!
//! The model replaces v0.8.x's `LayoutDensity` enum: instead of "drop this
//! specific row in Compact mode", each panel publishes a ranked list and the
//! renderer keeps as many top entries as the card's allocated width affords.
//! Primaries (TEMP, VRAM used/total, USED/FREE) survive at every capacity
//! tier; secondaries (HOTSPOT, JUNCTION, PCIE, V, etc.) drop first.

use crate::theme::Theme;
use crate::ui::capacity::Capacity;
use crate::ui::tooltips::tip;
use crate::widgets::stat::stat_row;
use egui::Color32;

/// One stat-row candidate. `priority == 0` is the highest priority (kept
/// first); higher numbers drop earlier as capacity shrinks. `tooltip_key`
/// must be a label recognised by [`crate::ui::tooltips::describe`].
#[derive(Debug, Clone)]
pub struct StatCandidate {
    pub priority: u8,
    pub label: &'static str,
    pub value: String,
    pub color: Option<Color32>,
    pub tooltip_key: &'static str,
}

/// Select the subset of `candidates` that fits the capacity, sorted by
/// priority ascending (lowest number = highest priority = kept first).
///
/// Pure function; does not touch egui. Exposed for unit testing.
pub fn select<'a>(candidates: &'a [StatCandidate], capacity: Capacity) -> Vec<&'a StatCandidate> {
    let mut sorted: Vec<&StatCandidate> = candidates.iter().collect();
    sorted.sort_by_key(|c| c.priority);
    sorted.truncate(capacity.rows);
    sorted
}

/// Paint the selected candidates into `ui` according to `capacity.columns`.
pub fn render(
    ui: &mut egui::Ui,
    theme: &Theme,
    candidates: Vec<StatCandidate>,
    capacity: Capacity,
) {
    let selected = select(&candidates, capacity);
    if selected.is_empty() {
        return;
    }
    match capacity.columns {
        1 => {
            for c in selected {
                tip(stat_row(ui, theme, c.label, &c.value, c.color), c.tooltip_key);
            }
        }
        _ => {
            // 2 columns: split with ceil/floor so the left column gets the
            // extra row when the count is odd.
            let split = selected.len().div_ceil(2);
            ui.columns(2, |cols| {
                let left = &mut cols[0];
                for c in &selected[..split] {
                    tip(stat_row(left, theme, c.label, &c.value, c.color), c.tooltip_key);
                }
                let right = &mut cols[1];
                for c in &selected[split..] {
                    tip(stat_row(right, theme, c.label, &c.value, c.color), c.tooltip_key);
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cand(priority: u8, label: &'static str) -> StatCandidate {
        StatCandidate {
            priority,
            label,
            value: "x".into(),
            color: None,
            tooltip_key: label,
        }
    }

    #[test]
    fn select_returns_top_n_by_priority() {
        let cands = vec![cand(2, "B"), cand(0, "A"), cand(3, "D"), cand(1, "C")];
        let out = select(&cands, Capacity { rows: 2, columns: 2 });
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].label, "A");
        assert_eq!(out[1].label, "C");
    }

    #[test]
    fn select_caps_at_candidate_count() {
        let cands = vec![cand(0, "A")];
        let out = select(&cands, Capacity { rows: 6, columns: 2 });
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn select_stable_among_equal_priorities() {
        let cands = vec![cand(0, "A"), cand(0, "B"), cand(0, "C")];
        let out = select(&cands, Capacity { rows: 3, columns: 2 });
        assert_eq!(
            out.iter().map(|c| c.label).collect::<Vec<_>>(),
            vec!["A", "B", "C"]
        );
    }

    #[test]
    fn select_returns_empty_when_capacity_zero() {
        let cands = vec![cand(0, "A"), cand(1, "B")];
        let out = select(&cands, Capacity { rows: 0, columns: 1 });
        assert!(out.is_empty());
    }
}
