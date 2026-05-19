//! The network component panel: two link-utilisation arrow meters.

use super::{card, panel_title};
use crate::format::format_bytes_per_sec;
use crate::ipc::NetInfo;
use crate::theme::Theme;
use egui::{Color32, FontId, Pos2, Rect, Sense, Shape, Stroke, Vec2};

/// Width of one arrow meter box (mockup `.pw-arrow { width: 72px }`).
const ARROW_W: f32 = 72.0;
/// Height of one arrow meter box (mockup `.pw-arrow { height: 86px }`).
const ARROW_H: f32 = 86.0;
/// Horizontal gap between the download and upload columns
/// (mockup `.pw-arrows { gap: 18px }`).
const COLUMN_GAP: f32 = 18.0;

/// Direction an arrow points; selects which rect+triangle decomposition to use.
#[derive(Clone, Copy)]
enum ArrowDir {
    Down,
    Up,
}

/// Render the NETWORK card: two arrow meters (download / upload) that fill by
/// height to the link-utilisation percentage.
///
/// With no active adapter (`net` is `None`) a single dimmed line is shown
/// instead. Absent throughput / utilisation fields are treated as zero; nothing
/// here panics.
pub fn network_panel(ui: &mut egui::Ui, theme: &Theme, net: Option<&NetInfo>) {
    card(ui, theme, |ui| {
        panel_title(ui, theme, "NETWORK", net.map(|n| n.adapter.as_str()));

        let Some(net) = net else {
            empty_state(ui, theme);
            return;
        };

        // Two arrow columns side by side, centred in the card body.
        let pair_w = ARROW_W * 2.0 + COLUMN_GAP;
        ui.horizontal(|ui| {
            let pad = ((ui.available_width() - pair_w) / 2.0).max(0.0);
            ui.add_space(pad);

            ui.spacing_mut().item_spacing.x = COLUMN_GAP;

            arrow_column(
                ui,
                theme,
                ArrowDir::Down,
                theme.accent,
                "DOWNLOAD",
                &format_bytes_per_sec(net.down_bps.unwrap_or(0.0)),
                net.down_pct.unwrap_or(0.0),
            );
            arrow_column(
                ui,
                theme,
                ArrowDir::Up,
                theme.accent_soft,
                "UPLOAD",
                &format_bytes_per_sec(net.up_bps.unwrap_or(0.0)),
                net.up_pct.unwrap_or(0.0),
            );
        });
    });
}

/// Draw one arrow column: the arrow meter on top, a letter-spaced label, then
/// the throughput value (mockup `.pw-arrow-col`).
fn arrow_column(
    ui: &mut egui::Ui,
    theme: &Theme,
    dir: ArrowDir,
    fill_color: Color32,
    label: &str,
    value: &str,
    util_pct: f64,
) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 9.0;

        // --- arrow meter (72 × 86 box) ---
        let (rect, _) = ui.allocate_exact_size(Vec2::new(ARROW_W, ARROW_H), Sense::hover());
        if ui.is_rect_visible(rect) {
            let fill = (util_pct / 100.0).clamp(0.0, 1.0) as f32;
            draw_arrow(ui, rect, dir, fill_color, fill, theme.track);
        }

        // --- label, centred, dim, letter-spaced ---
        centered_label(
            ui,
            &letter_spaced(label),
            FontId::new(9.0, theme.font_data.egui()),
            theme.dim,
        );

        // --- throughput value, centred, ink ---
        centered_label(
            ui,
            value,
            FontId::new(13.0, theme.font_data.egui()),
            theme.ink,
        );
    });
}

/// Draw an arrow meter inside `rect`.
///
/// An arrow outline is a non-convex 7-vertex polygon; `Shape::convex_polygon`
/// fan-triangulates and would render it wrong. So each arrow is decomposed into
/// one rectangle (the shaft) plus one triangle (the head) — both convex — and
/// the meter is drawn in two passes:
///   1. the whole arrow filled with `track_color` (the empty meter), then
///   2. the whole arrow filled with `fill_color` but clipped to only the bottom
///      `fill`-fraction of the box, so the meter fills upward from the bottom.
fn draw_arrow(
    ui: &egui::Ui,
    rect: Rect,
    dir: ArrowDir,
    fill_color: Color32,
    fill: f32,
    track_color: Color32,
) {
    // Pass 1: empty track-coloured arrow.
    let painter = ui.painter_at(rect);
    paint_arrow_shapes(&painter, rect, dir, track_color);

    // Pass 2: fill-coloured arrow, clipped to the bottom `fill`-fraction.
    let fill = fill.clamp(0.0, 1.0);
    if fill > 0.0 {
        let fill_h = fill * rect.height();
        let clip = Rect::from_min_max(Pos2::new(rect.min.x, rect.max.y - fill_h), rect.max);
        let clipped = ui.painter_at(rect).with_clip_rect(clip);
        paint_arrow_shapes(&clipped, rect, dir, fill_color);
    }
}

/// Shaft-rectangle vertices, as (x, y) fractions of the box (x right, y down),
/// for each arrow direction.
const DOWN_SHAFT: [(f32, f32); 4] = [(0.30, 0.0), (0.70, 0.0), (0.70, 0.52), (0.30, 0.52)];
const UP_SHAFT: [(f32, f32); 4] = [(0.30, 0.48), (0.70, 0.48), (0.70, 1.0), (0.30, 1.0)];
/// Head-triangle vertices, as (x, y) fractions of the box, for each direction.
const DOWN_HEAD: [(f32, f32); 3] = [(1.0, 0.52), (0.50, 1.0), (0.0, 0.52)];
const UP_HEAD: [(f32, f32); 3] = [(0.50, 0.0), (1.0, 0.48), (0.0, 0.48)];

/// Paint an arrow's rectangle + triangle (each a convex polygon) into `painter`
/// using `color`. The two pieces together tile the full arrow outline.
fn paint_arrow_shapes(painter: &egui::Painter, rect: Rect, dir: ArrowDir, color: Color32) {
    let shaft_frac: &[(f32, f32)] = match dir {
        ArrowDir::Down => &DOWN_SHAFT,
        ArrowDir::Up => &UP_SHAFT,
    };
    let head_frac: &[(f32, f32)] = match dir {
        ArrowDir::Down => &DOWN_HEAD,
        ArrowDir::Up => &UP_HEAD,
    };

    let to_pos = |&(fx, fy): &(f32, f32)| {
        Pos2::new(
            rect.min.x + fx * rect.width(),
            rect.min.y + fy * rect.height(),
        )
    };

    let shaft: Vec<Pos2> = shaft_frac.iter().map(to_pos).collect();
    let head: Vec<Pos2> = head_frac.iter().map(to_pos).collect();

    painter.add(Shape::convex_polygon(shaft, color, Stroke::NONE));
    painter.add(Shape::convex_polygon(head, color, Stroke::NONE));
}

/// Draw `text` horizontally centred on its own row.
fn centered_label(ui: &mut egui::Ui, text: &str, font: FontId, color: Color32) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, font.size + 2.0), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter_at(rect);
    let galley = painter.layout_no_wrap(text.to_owned(), font, color);
    painter.galley(
        Pos2::new(
            rect.center().x - galley.size().x / 2.0,
            rect.center().y - galley.size().y / 2.0,
        ),
        galley,
        color,
    );
}

/// Insert thin spaces between characters to approximate the mockup's
/// `letter-spacing` on the arrow labels.
fn letter_spaced(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 2);
    for (i, ch) in text.chars().enumerate() {
        if i > 0 {
            out.push('\u{2009}'); // thin space
        }
        out.push(ch);
    }
    out
}

/// Draw the centred dimmed line shown when there is no active network adapter.
fn empty_state(ui: &mut egui::Ui, theme: &Theme) {
    let total_w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(total_w, 22.0), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter_at(rect);
    let font = FontId::new(11.0, theme.font_data.egui());
    let galley = painter.layout_no_wrap("No active network adapter".to_string(), font, theme.dim);
    painter.galley(
        Pos2::new(
            rect.center().x - galley.size().x / 2.0,
            rect.center().y - galley.size().y / 2.0,
        ),
        galley,
        theme.dim,
    );
}
