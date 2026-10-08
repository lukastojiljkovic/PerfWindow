use crate::theme::Theme;
use egui::{Color32, FontId, Painter, Pos2, Rect, Response, Sense, Shape, Stroke, Vec2};
use std::f32::consts::TAU;

/// Size of the gauge widget in pixels.
const SIZE: f32 = 86.0;
/// Outer radius of the ring. Kept one pixel inside `SIZE / 2` so the stroke's
/// anti-aliasing feather (one physical pixel wide) still lands inside the
/// 86×86 allocation instead of being clipped flat against its edge.
const OUTER_R: f32 = SIZE / 2.0 - 1.0;
/// Ring thickness. The track and the accent arc both cover the
/// `OUTER_R - RING_THICK` .. `OUTER_R` band.
const RING_THICK: f32 = 8.0;
/// Number of points used to approximate a circle: both the full-circle track
/// and the partial accent arc (smooth enough at this size).
const SEGMENTS: usize = 120;

/// Draw a donut-style gauge centred in an 86×86 allocation.
///
/// `value` is 0–100 (clamped). The arc starts at 12 o'clock and advances
/// clockwise, filled with `theme.accent`. The track is `theme.track`.
/// Both cover the same `OUTER_R - RING_THICK` .. `OUTER_R` band. The centre is
/// left unpainted so the card shows through exactly as it does around the ring.
/// Centre text shows the integer value and `unit`; `label` appears below.
///
/// Returns the gauge's egui [`Response`] so callers can attach a hover
/// tooltip (e.g. `donut(...).on_hover_text("CPU load.")`).
pub fn donut(ui: &mut egui::Ui, theme: &Theme, value: f32, unit: &str, label: &str) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(SIZE), Sense::hover());

    if !ui.is_rect_visible(rect) {
        return response;
    }

    let painter = ui.painter_at(rect);

    // 0 % draws no arc; a non-finite reading makes `fraction` NaN, which fails
    // this comparison and leaves only the empty track behind the "--" text.
    let fraction = (value / 100.0).clamp(0.0, 1.0);
    track_and_arc(
        &painter,
        rect.center(),
        OUTER_R,
        RING_THICK,
        theme.track,
        theme.accent,
        fraction,
    );

    // --- centre text ---
    draw_center_text(&painter, rect, theme, value, unit, label);

    response
}

/// Draw a ring's full-circle `track` plus the `accent` arc over it. Both are
/// centred-stroked paths on the `radius - thickness / 2` midline, so they cover
/// exactly the `radius - thickness` .. `radius` band and the arc lies flush on
/// the track. `fraction` is a 0.0–1.0 share of the circle, starting at 12
/// o'clock and running clockwise; 0.0 draws no accent.
///
/// Shared by [`donut`] and the settings modal's theme-preview ring so the two
/// cannot drift apart.
///
/// The track is a [`Shape::closed_line`], not a `Painter::circle_stroke`: in
/// epaint 0.34 a circle's stroke is tessellated *outside* its radius
/// (`tessellate_circle` uses `PathStroke::from(stroke).outside()`), while a path
/// stroke is centred. A `circle_stroke` at `radius` would therefore cover
/// `radius` .. `radius + thickness` and leave a `thickness / 2` gap against the
/// arc. A closed line is centred like the arc and, being closed, has no seam
/// where its ends meet.
pub(crate) fn track_and_arc(
    painter: &Painter,
    center: Pos2,
    radius: f32,
    thickness: f32,
    track: Color32,
    accent: Color32,
    fraction: f32,
) {
    let mid_r = radius - thickness / 2.0;
    painter.add(Shape::closed_line(
        circle_points(center, mid_r),
        Stroke::new(thickness, track),
    ));
    if fraction > 0.0 {
        draw_arc(painter, center, mid_r, 0.0, fraction, thickness, accent);
    }
}

/// A full circle of `radius` around `center` as the point list
/// [`Shape::closed_line`] expects. Angle 0 = 12 o'clock; direction = clockwise.
fn circle_points(center: Pos2, radius: f32) -> Vec<Pos2> {
    (0..SEGMENTS)
        .map(|i| {
            let angle = i as f32 / SEGMENTS as f32 * TAU - std::f32::consts::FRAC_PI_2;
            center + Vec2::new(angle.cos() * radius, angle.sin() * radius)
        })
        .collect()
}

/// Draw an arc as a polyline of short segments with the given stroke width.
/// `start_frac` and `end_frac` are 0.0–1.0 fractions of the full circle.
/// Angle 0 = 12 o'clock; direction = clockwise.
fn draw_arc(
    painter: &Painter,
    center: Pos2,
    radius: f32,
    start_frac: f32,
    end_frac: f32,
    thickness: f32,
    color: Color32,
) {
    let n = ((end_frac - start_frac).abs() * SEGMENTS as f32).ceil() as usize + 1;
    let n = n.max(2);

    let points: Vec<Pos2> = (0..n)
        .map(|i| {
            let t = start_frac + (end_frac - start_frac) * (i as f32 / (n - 1) as f32);
            // 12 o'clock = -π/2, clockwise → subtract for standard math coords
            let angle = t * TAU - std::f32::consts::FRAC_PI_2;
            center + Vec2::new(angle.cos() * radius, angle.sin() * radius)
        })
        .collect();

    painter.add(Shape::line(points, Stroke::new(thickness, color)));
}

/// Render the centred value + unit + label text inside the gauge disc.
fn draw_center_text(
    painter: &Painter,
    rect: Rect,
    theme: &Theme,
    value: f32,
    unit: &str,
    label: &str,
) {
    let center = rect.center();

    // Value integer text (~22 px, ink colour). A non-finite reading (failed
    // sensor) must not display as a confident "0" — show "--" instead.
    let value_str = if value.is_finite() {
        format!("{}", value.round() as i32)
    } else {
        "--".to_string()
    };
    let value_font = FontId::new(22.0, theme.font_data.egui());

    // Unit suffix (~11 px, dim colour) — positioned to the right of the value
    let unit_font = FontId::new(11.0, theme.font_data.egui());

    // Measure value text width so we can position value+unit together, centred
    let value_galley = painter.layout_no_wrap(value_str.clone(), value_font.clone(), theme.ink);
    let unit_galley = painter.layout_no_wrap(unit.to_owned(), unit_font.clone(), theme.dim);

    let total_w = value_galley.size().x + unit_galley.size().x;
    // Align baselines: unit sits at same top as value (slightly baseline offset is fine at this size)
    let block_top = center.y - value_galley.size().y / 2.0 - 4.0;

    let value_x = center.x - total_w / 2.0;
    let unit_x = value_x + value_galley.size().x;

    painter.galley(Pos2::new(value_x, block_top), value_galley, theme.ink);
    // Align unit to the bottom of the value text (superscript-style offset)
    painter.galley(
        Pos2::new(unit_x, block_top + value_font.size - unit_font.size),
        unit_galley,
        theme.dim,
    );

    // Label below (~8 px, dim, uppercase)
    let label_font = FontId::new(8.0, theme.font_data.egui());
    let label_upper = label.to_uppercase();
    let label_galley = painter.layout_no_wrap(label_upper, label_font, theme.dim);
    let label_pos = Pos2::new(
        center.x - label_galley.size().x / 2.0,
        block_top + value_font.size + 3.0,
    );
    painter.galley(label_pos, label_galley, theme.dim);
}
