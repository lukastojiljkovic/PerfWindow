//! The network component panel.
//!
//! Shows download/upload utilisation in the same shape as CPU/GPU/RAM: a load
//! donut on the left, stat rows on the right, and a dual-line throughput
//! sparkline at the bottom (download primary, upload secondary).

use super::{card, empty_note, panel_title};
use crate::format::{finite, format_bytes_per_sec, format_link};
use crate::history::{samples_or_empty, NetThroughputHistory};
use crate::ipc::snapshot::WifiInfo;
use crate::ipc::NetInfo;
use crate::theme::Theme;
use crate::ui::capacity::Capacity;
use crate::ui::stat_priority::{render, StatCandidate};
use crate::widgets::gauge::donut;
use crate::widgets::sparkline::dual_sparkline;

/// Render the NETWORK card.
///
/// With no active adapter (`net` is `None`) a single dimmed line is shown
/// instead. Absent or non-finite throughput / utilisation fields are treated
/// as zero; nothing here panics.
pub fn network_panel(
    ui: &mut egui::Ui,
    theme: &Theme,
    net: Option<&NetInfo>,
    history: Option<&NetThroughputHistory>,
    capacity: Capacity,
    min_h: f32,
) {
    card(ui, theme, min_h, |ui| {
        panel_title(ui, theme, "NETWORK", net.map(|n| n.adapter.as_str()));

        let Some(net) = net else {
            empty_note(ui, theme, "No active network adapter");
            return;
        };

        // Donut on the left, priority-ranked stat rows on the right.
        let donut_pct = finite(net.down_pct)
            .zip(finite(net.up_pct))
            .map(|(d, u)| d.max(u))
            .or_else(|| finite(net.down_pct))
            .or_else(|| finite(net.up_pct))
            .unwrap_or(0.0);

        ui.horizontal(|ui| {
            donut(ui, theme, donut_pct as f32, "%", "USE").on_hover_text(
                "Network link utilisation — the larger of download and upload as a \
                 fraction of the adapter's negotiated link speed.",
            );
            ui.vertical(|ui| {
                let mut cands: Vec<StatCandidate> = Vec::new();

                // DOWN and UP share priority 0 — they should never split.
                cands.push(StatCandidate {
                    priority: 0,
                    label: "DOWN",
                    value: format_bytes_per_sec(finite(net.down_bps).unwrap_or(0.0)),
                    color: None,
                    tooltip_key: "DOWN",
                    hover_extra: None,
                });
                cands.push(StatCandidate {
                    priority: 0,
                    label: "UP",
                    value: format_bytes_per_sec(finite(net.up_bps).unwrap_or(0.0)),
                    color: None,
                    tooltip_key: "UP",
                    hover_extra: None,
                });

                // On a connected 802.11 adapter the LINK slot carries the
                // Wi-Fi association instead — SSID/signal/PHY rate say more
                // than the nominal link speed. Wired (or wifi with nothing
                // displayable) keeps the plain LINK row.
                if let Some(value) = net.wifi.as_ref().and_then(wifi_value) {
                    cands.push(StatCandidate {
                        priority: 1,
                        label: "WI-FI",
                        value,
                        color: None,
                        tooltip_key: "WI-FI",
                        hover_extra: None,
                    });
                } else if net.link_bps.is_some() {
                    cands.push(StatCandidate {
                        priority: 1,
                        label: "LINK",
                        value: format_link(net.link_bps),
                        color: None,
                        tooltip_key: "LINK",
                        hover_extra: None,
                    });
                }

                if !net.adapter.is_empty() {
                    cands.push(StatCandidate {
                        priority: 2,
                        label: "IFACE",
                        value: net.adapter.clone(),
                        color: None,
                        tooltip_key: "IFACE",
                        hover_extra: None,
                    });
                }

                render(ui, theme, cands, capacity);
            });
        });

        // Dual sparkline: download primary, upload secondary, both bytes/sec.
        // Y-ceiling is the rolling max of the two buffers, floored at 1.0 to
        // avoid division by zero.
        let y_max = history
            .map(|h| {
                h.down
                    .iter_oldest_first()
                    .chain(h.up.iter_oldest_first())
                    .fold(0.0_f32, f32::max)
            })
            .unwrap_or(0.0)
            .max(1.0);
        dual_sparkline(
            ui,
            theme,
            samples_or_empty(history.map(|h| &h.down)),
            samples_or_empty(history.map(|h| &h.up)),
            y_max,
        );
    });
}

/// Compose the WI-FI row value: `"ssid · 87 % · 866 Mbps"`, skipping absent
/// or non-finite parts. `None` when nothing is displayable, so the caller
/// falls back to the plain LINK row.
fn wifi_value(wifi: &WifiInfo) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    if let Some(ssid) = wifi.ssid.as_deref().map(str::trim) {
        if !ssid.is_empty() {
            parts.push(ssid.to_string());
        }
    }
    if let Some(signal) = finite(wifi.signal_pct) {
        parts.push(format!("{} %", signal.round() as i64));
    }
    if let Some(rate) = finite(wifi.phy_mbps) {
        parts.push(format!("{} Mbps", rate.round() as i64));
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" \u{00b7} "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wifi(ssid: Option<&str>, signal: Option<f64>, rate: Option<f64>) -> WifiInfo {
        WifiInfo {
            ssid: ssid.map(str::to_string),
            signal_pct: signal,
            phy_mbps: rate,
            band: None,
        }
    }

    #[test]
    fn wifi_value_joins_present_parts() {
        assert_eq!(
            wifi_value(&wifi(Some("HomeNet"), Some(87.4), Some(866.7))).as_deref(),
            Some("HomeNet \u{00b7} 87 % \u{00b7} 867 Mbps")
        );
    }

    #[test]
    fn wifi_value_skips_absent_and_non_finite_parts() {
        assert_eq!(
            wifi_value(&wifi(Some("  "), Some(f64::NAN), Some(120.0))).as_deref(),
            Some("120 Mbps")
        );
    }

    #[test]
    fn wifi_value_is_none_when_nothing_is_displayable() {
        assert_eq!(wifi_value(&wifi(None, None, None)), None);
        assert_eq!(wifi_value(&wifi(Some(""), Some(f64::INFINITY), None)), None);
    }
}
