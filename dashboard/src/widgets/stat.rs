use crate::theme::{FontFamily, Theme};
use egui::{Color32, FontId, Pos2, Response, Sense, Stroke, Vec2};

/// Row height for the stat widget (label + value on one line, plus a 4 px gap
/// at the bottom for the border rule).
const ROW_H: f32 = 22.0;

/// Draw a single labelled stat row.
///
/// Left side: `label` in ~9 px dim data-font, uppercase (matches `.pw-stat
/// label`). Right side: `value` in ~14 px bold data-font (matches `.pw-stat
/// b`), coloured `value_color` if `Some`, else `theme.ink`. A 1 px rule in
/// `theme.border` is drawn across the bottom. Matches `.pw-stat` in the mockup.
///
/// Returns the row's egui [`Response`] so callers can chain
/// [`Response::on_hover_text`] (or [`crate::ui::tooltips::tip`]) to attach an
/// explanation tooltip.
pub fn stat_row(
    ui: &mut egui::Ui,
    theme: &Theme,
    label: &str,
    value: &str,
    value_color: Option<Color32>,
) -> Response {
    let available_w = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(available_w, ROW_H), Sense::hover());

    if !ui.is_rect_visible(rect) {
        return response;
    }

    let painter = ui.painter_at(rect);

    // --- label (left, 9 px, dim, uppercase) ---
    let label_font = FontId::new(9.0, theme.font_data.egui());
    let label_upper = label.to_uppercase();
    let label_galley = painter.layout_no_wrap(label_upper, label_font, theme.dim);

    // Vertically centre in the usable row area (above the 4 px rule margin).
    let text_area_h = ROW_H - 4.0;
    let label_y = rect.min.y + (text_area_h - label_galley.size().y) / 2.0;
    painter.galley(Pos2::new(rect.min.x, label_y), label_galley, theme.dim);

    // --- value (right-aligned, 14 px, bold data font) ---
    let value_family = match theme.font_data {
        FontFamily::PlexMono => FontFamily::PlexMonoBold,
        other => other,
    };
    let value_font = FontId::new(14.0, value_family.egui());
    let color = value_color.unwrap_or(theme.ink);
    let value_galley = painter.layout_no_wrap(value.to_owned(), value_font, color);

    let value_x = rect.max.x - value_galley.size().x;
    let value_y = rect.min.y + (text_area_h - value_galley.size().y) / 2.0;
    painter.galley(Pos2::new(value_x, value_y), value_galley, color);

    // --- bottom rule (1 px, border colour) ---
    let rule_y = rect.max.y - 1.0;
    painter.add(egui::Shape::line_segment(
        [Pos2::new(rect.min.x, rule_y), Pos2::new(rect.max.x, rule_y)],
        Stroke::new(1.0, theme.border),
    ));

    response
}
