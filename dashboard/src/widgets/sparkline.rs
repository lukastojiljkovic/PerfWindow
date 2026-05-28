use crate::theme::Theme;
use egui::{Pos2, Sense, Shape, Stroke, Vec2};

/// Minimum vertical footprint of a sparkline, in pixels. Anything below this
/// is too small to read.
const MIN_HEIGHT: f32 = 30.0;
/// Maximum vertical footprint. Generous so the sparkline absorbs the space
/// left over after the stat rows in a fixed-height card.
const MAX_HEIGHT: f32 = 200.0;

/// Draw a 30 px-tall area-chart sparkline spanning the available width.
///
/// `samples` are arbitrary f32 values; `max` is the scale ceiling (maps to the
/// top edge). Values are linearly interpolated across the available width.
/// If fewer than 2 samples are provided the area is still allocated but nothing
/// is drawn.
pub fn sparkline(ui: &mut egui::Ui, theme: &Theme, samples: &[f32], max: f32) {
    let available_w = ui.available_width();
    let height = ui.available_height().clamp(MIN_HEIGHT, MAX_HEIGHT);
    let (rect, _response) = ui.allocate_exact_size(Vec2::new(available_w, height), Sense::hover());

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
            // Live sensor data: a non-finite sample (NaN/inf) would propagate
            // through clamp into a garbage Pos2 vertex — treat it as 0.0.
            let v = if v.is_finite() { v } else { 0.0 };
            let norm = (v / scale).clamp(0.0, 1.0);
            // y=top → max value; y=bottom → 0 value
            let y = rect.max.y - norm * rect.height();
            Pos2::new(x, y)
        })
        .collect();

    // --- area fill: one trapezoid per line segment ---
    // The data polyline zig-zags, so a single area polygon would be non-convex
    // and `convex_polygon` (fan triangulation) would render it wrong. Instead,
    // tile the area with per-segment trapezoids — each is genuinely convex.
    let fill_color = theme.accent.gamma_multiply(0.12);

    let baseline_y = rect.max.y;
    let trapezoids: Vec<Shape> = points
        .windows(2)
        .map(|seg| {
            let quad = vec![
                seg[0],
                seg[1],
                Pos2::new(seg[1].x, baseline_y),
                Pos2::new(seg[0].x, baseline_y),
            ];
            Shape::convex_polygon(quad, fill_color, Stroke::NONE)
        })
        .collect();
    painter.add(Shape::Vec(trapezoids));

    // --- polyline on top ---
    painter.add(Shape::line(points, Stroke::new(1.8, theme.accent)));
}

/// Like [`sparkline`], but renders a second `secondary` series on top of the
/// primary one in the same `theme.accent` at reduced opacity. Both series
/// share the same `max` (Y-axis ceiling), so they must be on the same scale
/// (both percentages, both MB/s, etc.).
///
/// If `secondary` has fewer than two samples, the function falls back to the
/// single-line behaviour of [`sparkline`].
pub fn dual_sparkline(
    ui: &mut egui::Ui,
    theme: &Theme,
    primary: &[f32],
    secondary: &[f32],
    max: f32,
) {
    let available_w = ui.available_width();
    let height = ui.available_height().clamp(MIN_HEIGHT, MAX_HEIGHT);
    let (rect, _response) = ui.allocate_exact_size(Vec2::new(available_w, height), Sense::hover());

    if primary.len() < 2 || !ui.is_rect_visible(rect) {
        return;
    }

    let painter = ui.painter_at(rect);
    let scale = if max > 0.0 { max } else { 1.0 };
    let baseline_y = rect.max.y;

    let map_points = |samples: &[f32]| -> Vec<Pos2> {
        let n = samples.len();
        samples
            .iter()
            .enumerate()
            .map(|(i, &v)| {
                let x = rect.min.x + (i as f32 / (n - 1) as f32) * rect.width();
                let v = if v.is_finite() { v } else { 0.0 };
                let norm = (v / scale).clamp(0.0, 1.0);
                let y = rect.max.y - norm * rect.height();
                Pos2::new(x, y)
            })
            .collect()
    };

    // Primary: fill + polyline, identical to the single-line `sparkline`.
    let primary_points = map_points(primary);
    let fill_color = theme.accent.gamma_multiply(0.12);
    let trapezoids: Vec<Shape> = primary_points
        .windows(2)
        .map(|seg| {
            let quad = vec![
                seg[0],
                seg[1],
                Pos2::new(seg[1].x, baseline_y),
                Pos2::new(seg[0].x, baseline_y),
            ];
            Shape::convex_polygon(quad, fill_color, Stroke::NONE)
        })
        .collect();
    painter.add(Shape::Vec(trapezoids));
    painter.add(Shape::line(primary_points, Stroke::new(1.8, theme.accent)));

    // Secondary: stroke only, lower opacity, no fill. Drawn after primary so
    // it remains visible when the two lines cross.
    if secondary.len() >= 2 {
        let secondary_points = map_points(secondary);
        let secondary_color = theme.accent.gamma_multiply(0.45);
        painter.add(Shape::line(
            secondary_points,
            Stroke::new(1.4, secondary_color),
        ));
    }
}
