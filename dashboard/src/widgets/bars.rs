use crate::theme::Theme;
use egui::{Color32, Rect, Response, Sense, Vec2};

/// A thin 6 px-tall horizontal progress bar spanning the available width.
///
/// `fraction` is 0.0–1.0 (clamped). The background uses `theme.track`; the
/// fill uses `theme.accent`, or `theme.warn` when `warn` is `true`.
/// Matches the `.pw-bar` element in the mockup.
pub fn bar_meter(ui: &mut egui::Ui, theme: &Theme, fraction: f32, warn: bool) -> Response {
    let available_w = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(available_w, 6.0), Sense::hover());

    if ui.is_rect_visible(rect) {
        let painter = ui.painter_at(rect);

        // Background track
        painter.rect_filled(rect, 0.0, theme.track);

        // Filled portion
        let fill_w = (fraction.clamp(0.0, 1.0) * rect.width()).max(0.0);
        if fill_w > 0.0 {
            let fill_rect = Rect::from_min_size(rect.min, Vec2::new(fill_w, rect.height()));
            let fill_color = if warn { theme.warn } else { theme.accent };
            painter.rect_filled(fill_rect, 0.0, fill_color);
        }
    }

    response
}

/// A 24 px-tall row of equal-width vertical bars, one per element of `loads`.
///
/// Each bar's height is proportional to its load (0–100), minimum 2 px.
/// Bars are separated by 3 px gaps and sit on the baseline (grow upward).
/// Fill is `theme.accent` at ~0.72 opacity.
/// Matches the `.pw-cores` element in the mockup.
pub fn core_strip(ui: &mut egui::Ui, theme: &Theme, loads: &[f32]) {
    if loads.is_empty() {
        return;
    }

    let strip_h = 24.0;
    let gap = 3.0;
    let available_w = ui.available_width();
    let (rect, _response) = ui.allocate_exact_size(Vec2::new(available_w, strip_h), Sense::hover());

    if !ui.is_rect_visible(rect) {
        return;
    }

    let painter = ui.painter_at(rect);
    let n = loads.len() as f32;
    let total_gap = gap * (n - 1.0);
    let bar_w = ((available_w - total_gap) / n).max(1.0);

    // accent at 0.72 opacity
    let [r, g, b, _] = theme.accent.to_array();
    let bar_color = Color32::from_rgba_unmultiplied(r, g, b, (255.0 * 0.72) as u8);

    for (i, &load) in loads.iter().enumerate() {
        let fraction = (load / 100.0).clamp(0.0, 1.0);
        let bar_h = (fraction * strip_h).max(2.0);

        let x = rect.min.x + i as f32 * (bar_w + gap);
        let bar_rect = Rect::from_min_size(
            egui::Pos2::new(x, rect.max.y - bar_h),
            Vec2::new(bar_w, bar_h),
        );
        painter.rect_filled(bar_rect, 0.0, bar_color);
    }
}
