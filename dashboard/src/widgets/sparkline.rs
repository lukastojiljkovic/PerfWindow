use crate::theme::Theme;
use egui::{Color32, Pos2, Sense, Shape, Stroke, Vec2};

/// Height of the sparkline widget in pixels.
const HEIGHT: f32 = 30.0;

/// Draw a 30 px-tall area-chart sparkline spanning the available width.
///
/// `samples` are arbitrary f32 values; `max` is the scale ceiling (maps to the
/// top edge). Values are linearly interpolated across the available width.
/// If fewer than 2 samples are provided the area is still allocated but nothing
/// is drawn.  Matches the `.pw-spark` element in the mockup.
pub fn sparkline(ui: &mut egui::Ui, theme: &Theme, samples: &[f32], max: f32) {
    let available_w = ui.available_width();
    let (rect, _response) = ui.allocate_exact_size(Vec2::new(available_w, HEIGHT), Sense::hover());

    if samples.len() < 2 || !ui.is_rect_visible(rect) {
        return;
    }

    let painter = ui.painter_at(rect);
    let n = samples.len();

    // Clamp max to avoid division by zero.
    let scale = if max > 0.0 { max } else { 1.0 };

    // Map sample index → (x, y) pixel position.
    let points: Vec<Pos2> = samples
        .iter()
        .enumerate()
        .map(|(i, &v)| {
            let x = rect.min.x + (i as f32 / (n - 1) as f32) * rect.width();
            let norm = (v / scale).clamp(0.0, 1.0);
            // y=0 (top) → max value; y=HEIGHT (bottom) → 0 value
            let y = rect.max.y - norm * HEIGHT;
            Pos2::new(x, y)
        })
        .collect();

    // --- filled polygon: line points + bottom-right + bottom-left ---
    let [r, g, b, _] = theme.accent.to_array();
    let fill_color = Color32::from_rgba_unmultiplied(r, g, b, (255.0 * 0.12) as u8);

    let mut poly = points.clone();
    poly.push(Pos2::new(rect.max.x, rect.max.y)); // bottom-right
    poly.push(Pos2::new(rect.min.x, rect.max.y)); // bottom-left
    painter.add(Shape::convex_polygon(poly, fill_color, Stroke::NONE));

    // --- polyline on top ---
    painter.add(Shape::line(points, Stroke::new(1.8, theme.accent)));
}
