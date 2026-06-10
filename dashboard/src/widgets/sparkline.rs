use crate::theme::Theme;
use egui::epaint::Mesh;
use egui::{Color32, Pos2, Rect, Sense, Shape, Stroke, Vec2};

/// Minimum vertical footprint of a sparkline, in pixels. Anything below this
/// is too small to read. Public so panels can reserve sparkline space when
/// sizing the widgets above it (e.g. the CPU heat-map clamp).
pub const MIN_HEIGHT: f32 = 30.0;
/// Maximum vertical footprint. Generous so the sparkline absorbs the space
/// left over after the stat rows in a fixed-height card.
const MAX_HEIGHT: f32 = 200.0;

/// Draw a 30 px-tall area-chart sparkline spanning the available width.
///
/// `samples` are arbitrary f32 values; `max` is the scale ceiling (maps to the
/// top edge). Values are linearly interpolated across the available width.
/// If fewer than 2 samples are provided the area is still allocated but nothing
/// is drawn.
pub fn sparkline(
    ui: &mut egui::Ui,
    theme: &Theme,
    samples: impl ExactSizeIterator<Item = f32>,
    max: f32,
) {
    let Some(rect) = allocate_chart(ui) else {
        return;
    };
    if samples.len() < 2 || !ui.is_rect_visible(rect) {
        return;
    }

    let painter = ui.painter_at(rect);
    let points = map_points(samples, rect, scale_of(max));

    painter.add(area_fill_mesh(&points, rect.max.y, fill_color(theme)));
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
    primary: impl ExactSizeIterator<Item = f32>,
    secondary: impl ExactSizeIterator<Item = f32>,
    max: f32,
) {
    let Some(rect) = allocate_chart(ui) else {
        return;
    };
    if primary.len() < 2 || !ui.is_rect_visible(rect) {
        return;
    }

    let painter = ui.painter_at(rect);
    let scale = scale_of(max);

    // Primary: fill + polyline, identical to the single-line `sparkline`.
    let primary_points = map_points(primary, rect, scale);
    painter.add(area_fill_mesh(
        &primary_points,
        rect.max.y,
        fill_color(theme),
    ));
    painter.add(Shape::line(primary_points, Stroke::new(1.8, theme.accent)));

    // Secondary: stroke only, lower opacity, no fill. Drawn after primary so
    // it remains visible when the two lines cross.
    if secondary.len() >= 2 {
        let secondary_points = map_points(secondary, rect, scale);
        let secondary_color = theme.accent.gamma_multiply(0.45);
        painter.add(Shape::line(
            secondary_points,
            Stroke::new(1.4, secondary_color),
        ));
    }
}

/// Allocate the chart rect spanning the available width. Returns `None` for
/// degenerate dimensions: a NaN / non-positive Vec2 builds a Rect that wgpu's
/// epaint pipeline rejects via `__fastfail` (STATUS_STACK_BUFFER_OVERRUN) in
/// release-LTO builds rather than a recoverable Rust panic.
fn allocate_chart(ui: &mut egui::Ui) -> Option<Rect> {
    let available_w = ui.available_width();
    let height = ui.available_height().clamp(MIN_HEIGHT, MAX_HEIGHT);
    if !available_w.is_finite() || available_w <= 0.0 || !height.is_finite() || height <= 0.0 {
        return None;
    }
    let (rect, _response) = ui.allocate_exact_size(Vec2::new(available_w, height), Sense::hover());
    Some(rect)
}

/// Clamp `max` away from zero so the normalisation below cannot divide by it.
fn scale_of(max: f32) -> f32 {
    if max > 0.0 {
        max
    } else {
        1.0
    }
}

fn fill_color(theme: &Theme) -> Color32 {
    theme.accent.gamma_multiply(0.12)
}

/// Map sample index → (x, y) pixel position across `rect`.
fn map_points(samples: impl ExactSizeIterator<Item = f32>, rect: Rect, scale: f32) -> Vec<Pos2> {
    let n = samples.len();
    samples
        .enumerate()
        .map(|(i, v)| {
            let x = rect.min.x + (i as f32 / (n - 1) as f32) * rect.width();
            // Live sensor data: a non-finite sample (NaN/inf) would propagate
            // through clamp into a garbage vertex — treat it as 0.0.
            let v = if v.is_finite() { v } else { 0.0 };
            let norm = (v / scale).clamp(0.0, 1.0);
            // y=top → max value; y=bottom → 0 value
            let y = rect.max.y - norm * rect.height();
            Pos2::new(x, y)
        })
        .collect()
}

/// Build the area fill under the data polyline as ONE mesh: two vertices per
/// sample (the data point and its projection onto the baseline) and two
/// triangles per segment. Replaces the previous one-boxed-`PathShape`-per-
/// segment trapezoid tiling (~240 shape allocations per chart per frame).
fn area_fill_mesh(points: &[Pos2], baseline_y: f32, color: Color32) -> Mesh {
    let mut mesh = Mesh::default();
    if points.len() < 2 {
        return mesh;
    }
    mesh.reserve_vertices(points.len() * 2);
    mesh.reserve_triangles((points.len() - 1) * 2);
    for p in points {
        mesh.colored_vertex(*p, color);
        mesh.colored_vertex(Pos2::new(p.x, baseline_y), color);
    }
    for i in 0..points.len() - 1 {
        let a = (2 * i) as u32;
        // Quad [point_i, baseline_i, point_i+1, baseline_i+1] as two
        // triangles; the per-segment tiling stays correct for a zig-zag
        // polyline where one big polygon would be non-convex.
        mesh.add_triangle(a, a + 1, a + 2);
        mesh.add_triangle(a + 1, a + 3, a + 2);
    }
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn area_fill_mesh_emits_two_triangles_per_segment() {
        let points = vec![
            Pos2::new(0.0, 10.0),
            Pos2::new(10.0, 5.0),
            Pos2::new(20.0, 8.0),
        ];
        let mesh = area_fill_mesh(&points, 30.0, Color32::RED);
        assert_eq!(mesh.vertices.len(), 6);
        assert_eq!(mesh.indices.len(), 4 * 3);
    }

    #[test]
    fn area_fill_mesh_is_empty_below_two_points() {
        assert!(area_fill_mesh(&[], 30.0, Color32::RED).is_empty());
        assert!(area_fill_mesh(&[Pos2::new(0.0, 0.0)], 30.0, Color32::RED).is_empty());
    }

    #[test]
    fn map_points_sanitises_non_finite_samples() {
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(100.0, 30.0));
        let samples = [f32::NAN, f32::INFINITY, 50.0];
        let points = map_points(samples.iter().copied(), rect, 100.0);
        assert!(points.iter().all(|p| p.x.is_finite() && p.y.is_finite()));
        // Non-finite samples map to the baseline (value 0).
        assert_eq!(points[0].y, rect.max.y);
        assert_eq!(points[1].y, rect.max.y);
    }
}
