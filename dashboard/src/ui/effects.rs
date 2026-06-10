//! The retro CRT atmosphere: a faint background grid, scanlines and a vignette.
//!
//! [`paint_grid`] draws the 25 px grid. It is called inside the central panel,
//! after the body fill but before the cards, so the opaque card frames paint
//! over it and it shows only through the inter-card gaps. [`paint_effects`]
//! draws the scanlines and vignette on a foreground layer as the last thing
//! the render method does each frame, so they sit on top of everything.
//!
//! Every effect is theme-parameterised and guarded: the Light theme sets
//! `scanline_opacity` and `vignette` to (near) zero, so it shows no CRT
//! atmosphere — only the faint base grid.

use crate::theme::Theme;
use egui::epaint::Mesh;
use egui::{Color32, Id, LayerId, Order, Pos2, Rect, Shape, Stroke, StrokeKind};

/// Grid cell size, in pixels.
const GRID_STEP: f32 = 25.0;
/// Grid line alpha as a fraction of `theme.border`.
const GRID_ALPHA: f32 = 0.05;
/// Scanline period, in pixels.
const SCANLINE_STEP: f32 = 4.0;
/// Number of concentric vignette strokes drawn inward from the window edge.
const VIGNETTE_RINGS: usize = 14;
/// Peak alpha, in 0..255, of the outermost vignette ring at
/// `theme.vignette == 1.0`. A short stack of fading 1 px strokes approximates a
/// soft inset shadow without any blur work.
const VIGNETTE_MAX_ALPHA: f32 = 80.0;

/// Draw the faint 25 px background grid into `ui`.
///
/// Called inside the central panel — after its body fill, before the cards —
/// so the opaque card frames paint over it and it shows only through the
/// inter-card gaps and body padding. The lines are `theme.border` faded to
/// ~5 % alpha: just enough texture to read as a grid without competing with
/// panel content.
pub fn paint_grid(ui: &egui::Ui, theme: &Theme) {
    let color = theme.border.gamma_multiply(GRID_ALPHA);
    // `gamma_multiply` can land on a fully transparent colour; nothing to draw.
    if color == Color32::TRANSPARENT {
        return;
    }
    let rect = ui.max_rect();
    if !rect.is_finite() || rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }

    // One mesh of 1 px quads instead of one line shape per grid line — the
    // tessellator otherwise rebuilds hundreds of stroked paths per frame.
    let mut mesh = Mesh::default();
    let mut x = rect.left();
    while x <= rect.right() {
        mesh.add_colored_rect(
            Rect::from_min_max(
                Pos2::new(x - 0.5, rect.top()),
                Pos2::new(x + 0.5, rect.bottom()),
            ),
            color,
        );
        x += GRID_STEP;
    }
    let mut y = rect.top();
    while y <= rect.bottom() {
        mesh.add_colored_rect(
            Rect::from_min_max(
                Pos2::new(rect.left(), y - 0.5),
                Pos2::new(rect.right(), y + 0.5),
            ),
            color,
        );
        y += GRID_STEP;
    }
    ui.painter().add(Shape::mesh(mesh));
}

/// Paint the scanline + vignette overlay for `theme` over the whole window.
///
/// Both effects are skipped entirely when their driving parameter is zero, so
/// a theme that disables an effect pays no painter cost for it at all. They
/// share one foreground layer, so their draw order is fixed: scanlines first,
/// then the vignette over them.
pub fn paint_effects(ctx: &egui::Context, theme: &Theme) {
    // `content_rect` is the whole window area safe for rendering — egui 0.34
    // split the former `screen_rect` into this and `viewport_rect`, and this is
    // the documented replacement for a full-window content overlay.
    let screen = ctx.content_rect();
    if !screen.is_finite() || screen.width() <= 0.0 || screen.height() <= 0.0 {
        return;
    }

    let painter = ctx.layer_painter(LayerId::new(Order::Foreground, Id::new("pw_overlay")));
    scanlines(&painter, theme, screen);
    vignette(&painter, theme, screen);
}

/// Draw horizontal scanlines every 4 px across the whole window.
///
/// Skipped unless `theme.scanline_opacity > 0` (the Light theme sets it to 0).
/// Each line is solid black at `scanline_opacity` alpha; all lines are batched
/// into one mesh so the per-frame overlay costs a single shape.
fn scanlines(painter: &egui::Painter, theme: &Theme, screen: Rect) {
    if theme.scanline_opacity <= 0.0 {
        return;
    }
    let alpha = (theme.scanline_opacity * 255.0).round().clamp(0.0, 255.0) as u8;
    if alpha == 0 {
        return;
    }
    let color = Color32::from_black_alpha(alpha);

    let mut mesh = Mesh::default();
    let mut y = screen.top();
    while y <= screen.bottom() {
        mesh.add_colored_rect(
            Rect::from_min_max(
                Pos2::new(screen.left(), y - 0.5),
                Pos2::new(screen.right(), y + 0.5),
            ),
            color,
        );
        y += SCANLINE_STEP;
    }
    painter.add(Shape::mesh(mesh));
}

/// Draw a soft dark vignette: a short stack of concentric 1 px rectangle
/// strokes just inside the window edge, fading out toward the centre.
///
/// Skipped unless `theme.vignette > 0`. The strokes step inward 1 px at a time;
/// alpha is highest at the outermost ring and decays linearly to zero, scaled
/// overall by `theme.vignette`. A dark band hugging the frame without a blur
/// pass.
fn vignette(painter: &egui::Painter, theme: &Theme, screen: Rect) {
    if theme.vignette <= 0.0 {
        return;
    }
    for ring in 0..VIGNETTE_RINGS {
        // `ring` 0 is the outermost, full-strength ring; strength fades to 0
        // at the innermost ring so the band dissolves into the centre.
        let strength = 1.0 - ring as f32 / VIGNETTE_RINGS as f32;
        let alpha = (theme.vignette * strength * VIGNETTE_MAX_ALPHA)
            .round()
            .clamp(0.0, 255.0) as u8;
        if alpha == 0 {
            continue;
        }
        let rect = screen.shrink(ring as f32);
        painter.rect_stroke(
            rect,
            0.0,
            Stroke::new(1.0, Color32::from_black_alpha(alpha)),
            StrokeKind::Inside,
        );
    }
}
