use crate::theme::{FontFamily, Theme};
use egui::{Color32, FontId, Pos2, Response, Sense, Stroke, Vec2};

/// Row height for the stat widget (label + value on one line, plus a 4 px gap
/// at the bottom for the border rule).
const ROW_H: f32 = 22.0;
/// Natural value size, and the smallest a long value shrinks to.
const VALUE_SIZE: f32 = 14.0;
const MIN_VALUE_SIZE: f32 = 9.0;
/// Space kept between the label and a value that had to shrink.
const LABEL_GAP: f32 = 6.0;

/// The value font size that fits `room` px, given the value's width at
/// `VALUE_SIZE`. The data fonts are monospaced, so width scales linearly with
/// size; a value that would run over its label ("1.0 / 8.0 GB" in a narrow
/// column) shrinks instead, down to `MIN_VALUE_SIZE`.
fn fitted_value_size(natural_w: f32, room: f32) -> f32 {
    if natural_w <= room || natural_w <= 0.0 {
        return VALUE_SIZE;
    }
    (VALUE_SIZE * room / natural_w).floor().max(MIN_VALUE_SIZE)
}

/// Draw a single labelled stat row.
///
/// Left side: `label` in ~9 px dim data-font, uppercase. Right side: `value`
/// in ~14 px bold data-font (smaller when it would run into the label),
/// coloured `value_color` if `Some`, else `theme.ink`. A 1 px rule in
/// `theme.border` is drawn across the bottom.
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
    // Reject NaN / non-positive width before it reaches the tessellator
    // (see widgets/sparkline.rs for the failure mode — wgpu epaint can abort
    // on a degenerate Rect in release-LTO builds).
    if !available_w.is_finite() || available_w <= 0.0 {
        let (_, response) = ui.allocate_exact_size(Vec2::new(0.0, ROW_H), Sense::hover());
        return response;
    }
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
    let label_w = label_galley.size().x;
    painter.galley(Pos2::new(rect.min.x, label_y), label_galley, theme.dim);

    // --- value (right-aligned, 14 px, bold data font) ---
    let value_family = match theme.font_data {
        FontFamily::PlexMono => FontFamily::PlexMonoBold,
        other => other,
    };
    let color = value_color.unwrap_or(theme.ink);
    let layout_value = |size: f32| {
        painter.layout_no_wrap(
            value.to_owned(),
            FontId::new(size, value_family.egui()),
            color,
        )
    };
    let mut value_galley = layout_value(VALUE_SIZE);
    let room = available_w - label_w - LABEL_GAP;
    let size = fitted_value_size(value_galley.size().x, room);
    if size < VALUE_SIZE {
        value_galley = layout_value(size);
    }

    let value_x = rect.max.x - value_galley.size().x;
    let value_y = rect.min.y + (text_area_h - value_galley.size().y) / 2.0;
    painter.galley(Pos2::new(value_x, value_y), value_galley, color);

    // --- bottom rule (1 px, border colour) ---
    let rule_y = rect.max.y - 1.0;
    painter.add(egui::Shape::line_segment(
        [Pos2::new(rect.min.x, rule_y), Pos2::new(rect.max.x, rule_y)],
        Stroke::new(1.0_f32, theme.border),
    ));

    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_that_fits_keeps_its_size() {
        assert_eq!(fitted_value_size(60.0, 80.0), VALUE_SIZE);
        assert_eq!(fitted_value_size(80.0, 80.0), VALUE_SIZE);
    }

    #[test]
    fn a_long_value_shrinks_to_its_room() {
        // 100 px at 14 px into 84 px of room: 14 * 0.84 = 11.76, floored.
        assert_eq!(fitted_value_size(100.0, 84.0), 11.0);
    }

    #[test]
    fn shrinking_stops_at_the_floor() {
        assert_eq!(fitted_value_size(200.0, 20.0), MIN_VALUE_SIZE);
        assert_eq!(fitted_value_size(200.0, -5.0), MIN_VALUE_SIZE);
    }
}
