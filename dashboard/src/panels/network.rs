//! The network component panel.
//!
//! Shows download/upload utilisation in the same shape as CPU/GPU/RAM: a load
//! donut on the left, stat rows on the right, and a dual-line throughput
//! sparkline at the bottom (download primary, upload secondary).

use super::{card, empty_note, panel_title};
use crate::format::{finite, format_bytes_per_sec, format_link};
use crate::history::NetThroughputHistory;
use crate::ipc::NetInfo;
use crate::theme::Theme;
use crate::widgets::gauge::donut;
use crate::widgets::sparkline::dual_sparkline;
use crate::widgets::stat::stat_row;

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
) {
    card(ui, theme, |ui| {
        panel_title(ui, theme, "NETWORK", net.map(|n| n.adapter.as_str()));

        let Some(net) = net else {
            empty_note(ui, theme, "No active network adapter");
            return;
        };

        // Donut on the left, three stat rows on the right.
        let donut_pct = finite(net.down_pct)
            .zip(finite(net.up_pct))
            .map(|(d, u)| d.max(u))
            .or_else(|| finite(net.down_pct))
            .or_else(|| finite(net.up_pct))
            .unwrap_or(0.0);

        ui.horizontal(|ui| {
            donut(ui, theme, donut_pct as f32, "%", "USE");
            ui.vertical(|ui| {
                stat_row(
                    ui,
                    theme,
                    "DOWN",
                    &format_bytes_per_sec(finite(net.down_bps).unwrap_or(0.0)),
                    None,
                );
                stat_row(
                    ui,
                    theme,
                    "UP",
                    &format_bytes_per_sec(finite(net.up_bps).unwrap_or(0.0)),
                    None,
                );
                stat_row(ui, theme, "LINK", &format_link(net.link_bps), None);
            });
        });

        // Dual sparkline: download primary, upload secondary, both bytes/sec.
        // Y-ceiling is the rolling max of the two buffers, floored at 1.0 to
        // avoid division by zero.
        let (down_samples, up_samples) = match history {
            Some(h) => (
                h.down.iter_oldest_first().collect::<Vec<f32>>(),
                h.up.iter_oldest_first().collect::<Vec<f32>>(),
            ),
            None => (Vec::new(), Vec::new()),
        };
        let y_max = down_samples
            .iter()
            .chain(up_samples.iter())
            .copied()
            .fold(0.0_f32, f32::max)
            .max(1.0);
        dual_sparkline(ui, theme, &down_samples, &up_samples, y_max);
    });
}
