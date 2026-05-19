//! The network component panel: two link-utilisation arrow meters.

use super::{card, empty_note, panel_title};
use crate::format::{finite, format_bytes_per_sec};
use crate::ipc::NetInfo;
use crate::theme::{FontFamily, Theme};
use egui::{Color32, FontId, Pos2, Rect, Sense, Shape, Stroke, Vec2};
use std::sync::Arc;

/// Width of one arrow meter box (mockup `.pw-arrow { width: 72px }`).
const ARROW_W: f32 = 72.0;
/// Height of one arrow meter box (mockup `.pw-arrow { height: 86px }`).
const ARROW_H: f32 = 86.0;
/// Horizontal gap between the download and upload columns
/// (mockup `.pw-arrows { gap: 18px }`).
const COLUMN_GAP: f32 = 18.0;
/// Vertical gap between an arrow, its label and its value
/// (mockup `.pw-arrow-col { gap: 9px }`).
const STACK_GAP: f32 = 9.0;
/// Font size of the letter-spaced direction label.
const LABEL_SIZE: f32 = 9.0;
/// Font size of the throughput value (mockup `.pw-arrow-val { font: 600 17px }`).
const VALUE_SIZE: f32 = 17.0;

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
/// instead. Absent or non-finite throughput / utilisation fields are treated as
/// zero; nothing here panics.
pub fn network_panel(ui: &mut egui::Ui, theme: &Theme, net: Option<&NetInfo>) {
    card(ui, theme, |ui| {
        panel_title(ui, theme, "NETWORK", net.map(|n| n.adapter.as_str()));

        let Some(net) = net else {
            empty_note(ui, theme, "No active network adapter");
            return;
        };

        arrows_row(ui, theme, net);
    });
}

/// Lay out the download and upload columns, centred as a pair in the card body.
fn arrows_row(ui: &mut egui::Ui, theme: &Theme, net: &NetInfo) {
    let down = ArrowColumn::measure(
        ui,
        theme,
        ArrowDir::Down,
        theme.accent,
        "DOWNLOAD",
        &format_bytes_per_sec(finite(net.down_bps).unwrap_or(0.0)),
        finite(net.down_pct),
    );
    let up = ArrowColumn::measure(
        ui,
        theme,
        ArrowDir::Up,
        theme.accent_soft,
        "UPLOAD",
        &format_bytes_per_sec(finite(net.up_bps).unwrap_or(0.0)),
        finite(net.up_pct),
    );

    let pair_w = down.width + COLUMN_GAP + up.width;
    let row_h = down.height.max(up.height);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), row_h), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter_at(rect);

    // Centre the pair in the row; paint each column around its own centre x.
    let mut cx = rect.center().x - pair_w / 2.0 + down.width / 2.0;
    down.paint(&painter, theme, Pos2::new(cx, rect.min.y));
    cx += down.width / 2.0 + COLUMN_GAP + up.width / 2.0;
    up.paint(&painter, theme, Pos2::new(cx, rect.min.y));
}

/// One measured arrow column: the meter's direction and fill plus the
/// pre-laid-out label and value galleys, ready to paint around a centre point.
struct ArrowColumn {
    dir: ArrowDir,
    fill_color: Color32,
    /// Bottom-up fill fraction, in `0.0..=1.0`.
    fill: f32,
    label: Arc<egui::Galley>,
    value: Arc<egui::Galley>,
    /// Widest of the arrow box and the two galleys — spaces the pair.
    width: f32,
    /// Stacked height of arrow + label + value.
    height: f32,
}

impl ArrowColumn {
    /// Lay out a column's text and compute its footprint. `util_pct` is the
    /// link-utilisation percentage (0–100), or `None` when the link speed is
    /// unknown — then the meter shows empty but the value still shows.
    fn measure(
        ui: &egui::Ui,
        theme: &Theme,
        dir: ArrowDir,
        fill_color: Color32,
        label: &str,
        value: &str,
        util_pct: Option<f64>,
    ) -> Self {
        let painter = ui.painter();
        let label_galley = painter.layout_no_wrap(
            letter_spaced(label),
            FontId::new(LABEL_SIZE, theme.font_data.egui()),
            theme.dim,
        );
        // The value uses the data font's bold sibling where one exists
        // (mockup `.pw-arrow-val { font-weight: 600 }`).
        let value_family = match theme.font_data {
            FontFamily::PlexMono => FontFamily::PlexMonoBold,
            other => other,
        };
        let value_galley = painter.layout_no_wrap(
            value.to_owned(),
            FontId::new(VALUE_SIZE, value_family.egui()),
            theme.ink,
        );

        let width = ARROW_W
            .max(label_galley.size().x)
            .max(value_galley.size().x);
        let height =
            ARROW_H + STACK_GAP + label_galley.size().y + STACK_GAP + value_galley.size().y;
        let fill = util_pct
            .map(|p| (p / 100.0).clamp(0.0, 1.0) as f32)
            .unwrap_or(0.0);

        Self {
            dir,
            fill_color,
            fill,
            label: label_galley,
            value: value_galley,
            width,
            height,
        }
    }

    /// Paint the arrow, label and value, all horizontally centred on
    /// `top_center.x` and stacked downward from `top_center.y`.
    fn paint(&self, painter: &egui::Painter, theme: &Theme, top_center: Pos2) {
        let arrow_rect = Rect::from_min_size(
            Pos2::new(top_center.x - ARROW_W / 2.0, top_center.y),
            Vec2::new(ARROW_W, ARROW_H),
        );
        draw_arrow(
            painter,
            arrow_rect,
            self.dir,
            self.fill_color,
            self.fill,
            theme.track,
        );

        let label_y = arrow_rect.max.y + STACK_GAP;
        painter.galley(
            Pos2::new(top_center.x - self.label.size().x / 2.0, label_y),
            self.label.clone(),
            theme.dim,
        );

        let value_y = label_y + self.label.size().y + STACK_GAP;
        painter.galley(
            Pos2::new(top_center.x - self.value.size().x / 2.0, value_y),
            self.value.clone(),
            theme.ink,
        );
    }
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
    painter: &egui::Painter,
    rect: Rect,
    dir: ArrowDir,
    fill_color: Color32,
    fill: f32,
    track_color: Color32,
) {
    // Pass 1: empty track-coloured arrow.
    paint_arrow_shapes(painter, rect, dir, track_color);

    // Pass 2: fill-coloured arrow, clipped to the bottom `fill`-fraction.
    let fill = fill.clamp(0.0, 1.0);
    if fill > 0.0 {
        let fill_h = fill * rect.height();
        let clip = Rect::from_min_max(Pos2::new(rect.min.x, rect.max.y - fill_h), rect.max);
        paint_arrow_shapes(&painter.with_clip_rect(clip), rect, dir, fill_color);
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

/// Insert thin spaces between characters to approximate the mockup's
/// `letter-spacing` on the arrow labels.
fn letter_spaced(text: &str) -> String {
    // Each inter-character gap is a 3-byte thin space; reserve generously.
    let mut out = String::with_capacity(text.len() * 4);
    for (i, ch) in text.chars().enumerate() {
        if i > 0 {
            out.push('\u{2009}'); // thin space
        }
        out.push(ch);
    }
    out
}
