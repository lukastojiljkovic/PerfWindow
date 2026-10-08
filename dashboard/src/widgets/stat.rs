use crate::theme::{FontFamily, Theme};
use crate::ui::recorder;
use egui::{Color32, FontId, Pos2, Rect, Response, Sense, Stroke, Vec2};

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

/// Lay a value out on one line inside `max_width`, ending it with an ellipsis
/// when it cannot fit. Used as the last-resort guard in [`stat_row`]: below
/// `MIN_VALUE_SIZE` the text would be unreadable, so the row truncates instead
/// of letting the value run into its label.
fn elided_value(
    painter: &egui::Painter,
    value: &str,
    size: f32,
    family: FontFamily,
    color: Color32,
    max_width: f32,
) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::single_section(
        value.to_owned(),
        egui::TextFormat {
            font_id: FontId::new(size, family.egui()),
            color,
            ..Default::default()
        },
    );
    // A hair of slack keeps a galley that only just fits from flipping to the
    // overflow character on a rounding error.
    job.wrap.max_width = (max_width - 0.5).max(1.0);
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    job.wrap.overflow_character = Some('\u{2026}');
    painter.layout_job(job)
}

/// A drawn stat row: the row's [`Response`] plus what the caller needs to
/// build a tooltip — whether the value had to be elided, and the full value so
/// a truncated reading is never lost.
pub struct StatRow {
    pub response: Response,
    pub elided: bool,
    value: String,
}

impl StatRow {
    /// Attach `base` (the panel's own explanation for the row) as the hover
    /// text, appending the full value when the row had to elide it.
    pub fn with_hover(self, base: Option<String>) -> Response {
        let full = self.elided.then(|| format!("Full value: {}", self.value));
        match (base, full) {
            (Some(base), Some(full)) => self.response.on_hover_text(format!("{base}\n\n{full}")),
            (Some(base), None) => self.response.on_hover_text(base),
            (None, Some(full)) => self.response.on_hover_text(full),
            (None, None) => self.response,
        }
    }
}

/// Draw a single labelled stat row.
///
/// Left side: `label` in ~9 px dim data-font, uppercase. Right side: `value`
/// in ~14 px bold data-font (smaller when it would run into the label),
/// coloured `value_color` if `Some`, else `theme.ink`. A 1 px rule in
/// `theme.border` is drawn across the bottom.
///
/// A value that does not fit next to its label at 9 px is elided with `…`;
/// callers chain [`StatRow::with_hover`] to attach an explanation tooltip that
/// then also carries the full value.
pub fn stat_row(
    ui: &mut egui::Ui,
    theme: &Theme,
    label: &str,
    value: &str,
    value_color: Option<Color32>,
) -> StatRow {
    let available_w = ui.available_width();
    // Reject NaN / non-positive width before it reaches the tessellator
    // (see widgets/sparkline.rs for the failure mode — wgpu epaint can abort
    // on a degenerate Rect in release-LTO builds).
    if !available_w.is_finite() || available_w <= 0.0 {
        let (_, response) = ui.allocate_exact_size(Vec2::new(0.0, ROW_H), Sense::hover());
        return StatRow {
            response,
            elided: false,
            value: value.to_owned(),
        };
    }
    let (rect, response) = ui.allocate_exact_size(Vec2::new(available_w, ROW_H), Sense::hover());

    if !ui.is_rect_visible(rect) {
        return StatRow {
            response,
            elided: false,
            value: value.to_owned(),
        };
    }

    let painter = ui.painter_at(rect);

    // --- label (left, 9 px, dim, uppercase) ---
    let label_font = FontId::new(9.0, theme.font_data.egui());
    let label_upper = label.to_uppercase();
    let label_galley = painter.layout_no_wrap(label_upper, label_font, theme.dim);

    // Vertically centre in the usable row area (above the 4 px rule margin).
    let text_area_h = ROW_H - 4.0;
    let label_y = rect.min.y + (text_area_h - label_galley.size().y) / 2.0;
    let label_size = label_galley.size();
    let label_w = label_size.x;
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
    let room = (available_w - label_w - LABEL_GAP).max(0.0);
    let size = fitted_value_size(value_galley.size().x, room);
    if size < VALUE_SIZE {
        value_galley = layout_value(size);
    }
    // Safety net: if even the 9 px floor runs into the label, elide instead of
    // overlapping. The full value stays available in the hover text below.
    let mut elided = false;
    if value_galley.size().x > room + 0.5 {
        elided = true;
        value_galley = elided_value(&painter, value, size, value_family, color, room);
    }

    let value_x = rect.max.x - value_galley.size().x;
    let value_y = rect.min.y + (text_area_h - value_galley.size().y) / 2.0;
    let value_size = value_galley.size();
    painter.galley(Pos2::new(value_x, value_y), value_galley, color);

    // --- bottom rule (1 px, border colour) ---
    let rule_y = rect.max.y - 1.0;
    painter.add(egui::Shape::line_segment(
        [Pos2::new(rect.min.x, rule_y), Pos2::new(rect.max.x, rule_y)],
        Stroke::new(1.0_f32, theme.border),
    ));

    let label_rect = Rect::from_min_size(Pos2::new(rect.min.x, label_y), label_size);
    let value_rect = Rect::from_min_size(Pos2::new(value_x, value_y), value_size);
    recorder::stat_row(label_rect, value_rect, elided, value);

    StatRow {
        response,
        elided,
        value: value.to_owned(),
    }
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
