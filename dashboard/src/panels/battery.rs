//! The battery component panel — laptop-only, hidden on desktops.

use super::{card, panel_title};
use crate::format::{finite, format_uptime};
use crate::ipc::BatteryInfo;
use crate::theme::Theme;
use crate::ui::capacity::Capacity;
use crate::ui::stat_priority::{render, StatCandidate};
use crate::widgets::gauge::donut;
use crate::widgets::health_color;

/// Render the BATTERY card: charge donut on the left, four stat rows on the
/// right (STATE, RATE, TIME, HEALTH). Missing readings render as `"—"`;
/// nothing here can panic.
///
/// The caller is expected to gate the card on `snap.battery.is_some()` — if
/// no battery hardware is present, the card should not be added to the grid.
pub fn battery_panel(
    ui: &mut egui::Ui,
    theme: &Theme,
    batt: &BatteryInfo,
    capacity: Capacity,
    min_h: f32,
) {
    card(ui, theme, min_h, |ui| {
        panel_title(ui, theme, "BATTERY", None);

        ui.horizontal(|ui| {
            let charge = batt.charge_pct.unwrap_or(0.0) as f32;
            donut(ui, theme, charge, "%", "CHARGE").on_hover_text(
                "Battery charge level, 0–100 %. Reading comes from the embedded \
                 controller via the Windows power API.",
            );
            ui.vertical(|ui| {
                let mut cands: Vec<StatCandidate> = Vec::new();

                if let Some(pct) = batt.charge_pct {
                    cands.push(StatCandidate {
                        priority: 0,
                        label: "REMAINING",
                        value: format!("{} %", pct.round() as i64),
                        color: None,
                        tooltip_key: "REMAINING",
                    });
                }

                // TIME: shown unconditionally so the row keeps its slot
                // across AC/charging/idle states; em-dash when the OS has
                // no estimate (charging or just-plugged-in).
                let time_value = match batt.time_remaining_sec.filter(|s| *s > 0) {
                    Some(secs) => format_uptime(secs),
                    None => "—".to_string(),
                };
                cands.push(StatCandidate {
                    priority: 1,
                    label: "TIME",
                    value: time_value,
                    color: None,
                    tooltip_key: "TIME",
                });

                // RATE: dedicated row with directional arrow restored from
                // v0.8.x. Folded into STATE during v0.9.0 transition; the
                // arrow + decimals carry more information than the rounded
                // integer in the STATE row.
                if let Some(rate) = batt.rate_w {
                    cands.push(StatCandidate {
                        priority: 2,
                        label: "RATE",
                        value: format_rate(Some(rate)),
                        color: None,
                        tooltip_key: "RATE",
                    });
                }

                if let Some(rate) = batt.rate_w {
                    let (state, col) = if rate > 0.05 {
                        ("CHARGING", Some(theme.ok))
                    } else if rate < -0.05 {
                        ("DISCHARGING", Some(theme.warn))
                    } else {
                        ("IDLE", Some(theme.ok))
                    };
                    cands.push(StatCandidate {
                        priority: 3,
                        label: "STATE",
                        value: state.to_string(),
                        color: col,
                        tooltip_key: "STATE",
                    });
                }

                // HEALTH: restored from v0.8.x as remaining life percentage.
                // Wear is computed internally for the health colour band but
                // the visible value matches user expectation ("100 % = new").
                if let (Some(design), Some(full)) =
                    (batt.design_capacity_mwh, batt.full_capacity_mwh)
                {
                    if design > 0.0 {
                        let health = (full / design * 100.0).clamp(0.0, 100.0);
                        cands.push(StatCandidate {
                            priority: 4,
                            label: "HEALTH",
                            value: format!("{health:.1} %"),
                            color: Some(health_color(health, theme)),
                            tooltip_key: "HEALTH",
                        });
                    }
                }

                render(ui, theme, cands, capacity);
            });
        });
    });
}

/// Power-flow state shown in the STATE stat row, with a colour cue that
/// matches the situation: green-ish ok while charging or on AC, warn while
/// running on battery, dim when no reading is available.
///
/// Currently unused: the v0.9.0 priority-ranked renderer inlines the
/// state+rate composition in the panel body. Kept for potential reuse.
#[allow(dead_code)]
fn format_state(rate_w: Option<f64>, theme: &Theme) -> (String, Option<egui::Color32>) {
    match finite(rate_w) {
        Some(w) if w > 0.05 => ("CHARGING".to_string(), Some(theme.ok)),
        Some(w) if w < -0.05 => ("ON BATTERY".to_string(), Some(theme.warn)),
        Some(_) => ("ON AC".to_string(), Some(theme.ok)),
        None => ("—".to_string(), None),
    }
}

/// Positive rate → `"\u{2191}12.4 W"`, negative → `"\u{2193}12.4 W"`,
/// idle (on AC, no charge/discharge) → `"0 W"`, absent → `"—"`.
fn format_rate(rate_w: Option<f64>) -> String {
    match finite(rate_w) {
        Some(w) if w > 0.05 => format!("\u{2191}{:.1} W", w),
        Some(w) if w < -0.05 => format!("\u{2193}{:.1} W", -w),
        Some(_) => "0 W".to_string(),
        None => "—".to_string(),
    }
}

/// Show `"3h 14m"` while discharging; suppress (`"—"`) while charging,
/// stopped, or with no reading — Windows' charging-side estimate is
/// notoriously unreliable.
///
/// Currently unused: the v0.9.0 priority-ranked renderer surfaces TIME
/// whenever the OS reports a positive estimate, regardless of direction.
/// Kept for potential reuse.
#[allow(dead_code)]
fn format_time_remaining(secs: Option<i64>, rate_w: Option<f64>) -> String {
    let discharging = matches!(finite(rate_w), Some(w) if w < -0.05);
    match (discharging, secs) {
        (true, Some(s)) if s > 0 => format_uptime(s),
        _ => "—".to_string(),
    }
}

/// `"<full / design × 100>%"` with health-banded colour, or `"—"` when
/// either capacity reading is missing.
///
/// Currently unused: the v0.9.0 priority-ranked renderer exposes WEAR
/// (1 − full/design) instead of remaining HEALTH. Kept for potential reuse.
#[allow(dead_code)]
fn format_health(
    full_mwh: Option<f64>,
    design_mwh: Option<f64>,
    theme: &Theme,
) -> (String, Option<egui::Color32>) {
    match (finite(full_mwh), finite(design_mwh)) {
        (Some(f), Some(d)) if d > 0.0 => {
            let pct = (f / d * 100.0).min(100.0);
            (format!("{pct:.1}%"), Some(health_color(pct, theme)))
        }
        _ => ("—".to_string(), None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn batt(rate_w: Option<f64>, secs: Option<i64>) -> BatteryInfo {
        BatteryInfo {
            charge_pct: Some(64.0),
            rate_w,
            voltage_v: None,
            design_capacity_mwh: None,
            full_capacity_mwh: None,
            time_remaining_sec: secs,
        }
    }

    #[test]
    fn rate_formats_with_direction_arrow() {
        assert_eq!(format_rate(Some(12.4)), "\u{2191}12.4 W");
        assert_eq!(format_rate(Some(-8.7)), "\u{2193}8.7 W");
        assert_eq!(format_rate(Some(0.0)), "0 W");
        assert_eq!(format_rate(None), "—");
    }

    #[test]
    fn time_remaining_shown_only_while_discharging() {
        // Discharging with a positive remaining estimate -> formatted uptime.
        let b = batt(Some(-10.0), Some(3 * 3600 + 25 * 60));
        assert_eq!(
            format_time_remaining(b.time_remaining_sec, b.rate_w),
            "3h 25m"
        );
        // Charging -> suppressed.
        let b = batt(Some(20.0), Some(900));
        assert_eq!(format_time_remaining(b.time_remaining_sec, b.rate_w), "—");
        // Idle -> suppressed.
        let b = batt(Some(0.0), Some(900));
        assert_eq!(format_time_remaining(b.time_remaining_sec, b.rate_w), "—");
        // No reading -> suppressed.
        let b = batt(None, None);
        assert_eq!(format_time_remaining(b.time_remaining_sec, b.rate_w), "—");
    }

    #[test]
    fn state_label_picks_correct_tag() {
        use crate::theme::Theme;
        let theme = Theme::for_id(crate::config::ThemeId::Amber);
        assert_eq!(format_state(Some(15.0), &theme).0, "CHARGING");
        assert_eq!(format_state(Some(-12.0), &theme).0, "ON BATTERY");
        assert_eq!(format_state(Some(0.0), &theme).0, "ON AC");
        assert_eq!(format_state(None, &theme).0, "—");
    }
}
