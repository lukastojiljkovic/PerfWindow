//! The retro CRT atmosphere overlay: a faint background grid, scanlines and a
//! vignette.
//!
//! [`paint_effects`] is the last thing the render method does each frame. All
//! three effects are pure [`egui::Painter`] work — no real bloom, no textures —
//! and every one is theme-parameterised and individually guarded, so the Light
//! theme (which sets all three parameters at or near zero) renders genuinely
//! effect-free with no wasted painter calls.
//!
//! The grid is drawn on a [`egui::Order::Background`] layer so it sits behind
//! the panels; the scanlines and vignette are drawn on a
//! [`egui::Order::Foreground`] layer so they sit on top of everything. This
//! mirrors the mockup's `docs/mockups/dashboard-layout.html`, where the grid is
//! a `background-image` on `.pw-mock` and the scanline / vignette pseudo-
//! elements carry high `z-index` values.

use crate::theme::Theme;
use egui::{Color32, Id, LayerId, Order, Rect, Stroke, StrokeKind};

/// Grid cell size, in pixels (mockup `.pw-mock { background-size: 25px 25px }`).
const GRID_STEP: f32 = 25.0;
/// Grid line alpha as a fraction of `theme.border`
/// (mockup grid `linear-gradient(rgba(127,127,127,.05) 1px, ...)`).
const GRID_ALPHA: f32 = 0.05;
/// Scanline period, in pixels (mockup scanline `repeating-linear-gradient`
/// stop at `4px`).
const SCANLINE_STEP: f32 = 4.0;
/// Number of concentric vignette strokes drawn inward from the window edge.
const VIGNETTE_RINGS: usize = 14;
/// Peak alpha, in 0..255, of the innermost-decaying vignette ring at
/// `theme.vignette == 1.0`. The mockup's vignette is a soft 110 px inset
/// shadow; a short stack of fading 1 px strokes approximates that band without
/// any blur work.
const VIGNETTE_MAX_ALPHA: f32 = 80.0;

/// Paint the grid / scanline / vignette overlay for `theme` over the whole
/// window.
///
/// Each effect is skipped entirely when its driving parameter is zero, so a
/// theme that disables an effect pays no painter cost for it at all.
pub fn paint_effects(ctx: &egui::Context, theme: &Theme) {
    // `content_rect` is the whole window area safe for rendering — egui 0.34
    // split the former `screen_rect` into this and `viewport_rect`, and this is
    // the documented replacement for a full-window content overlay.
    let screen = ctx.content_rect();
    if screen.width() <= 0.0 || screen.height() <= 0.0 {
        return;
    }

    background_grid(ctx, theme, screen);
    scanlines(ctx, theme, screen);
    vignette(ctx, theme, screen);
}

/// Draw the faint 25 px background grid on a background layer, behind the
/// panels.
///
/// The lines are `theme.border` faded to ~5 % alpha — just enough texture to
/// read as a grid without competing with panel content. It is drawn on an
/// [`egui::Order::Background`] layer (rather than the foreground) precisely so
/// the opaque panel cards paint over it: a foreground grid would lay hairlines
/// across every gauge and readout and hurt legibility, whereas the mockup's
/// grid is a `background-image` showing only through the inter-card gaps.
fn background_grid(ctx: &egui::Context, theme: &Theme, screen: Rect) {
    let color = theme.border.gamma_multiply(GRID_ALPHA);
    // `gamma_multiply` can land on a fully transparent colour; nothing to draw.
    if color == Color32::TRANSPARENT {
        return;
    }
    let painter = ctx.layer_painter(LayerId::new(Order::Background, Id::new("pw_grid")));
    let stroke = Stroke::new(1.0, color);

    // Vertical lines, then horizontal lines, stepping across the full window.
    let mut x = screen.left();
    while x <= screen.right() {
        painter.vline(x, screen.y_range(), stroke);
        x += GRID_STEP;
    }
    let mut y = screen.top();
    while y <= screen.bottom() {
        painter.hline(screen.x_range(), y, stroke);
        y += GRID_STEP;
    }
}

/// Draw horizontal scanlines every 4 px across the whole window.
///
/// Skipped unless `theme.scanline_opacity > 0` (the Light theme sets it to 0).
/// Each line is solid black at `scanline_opacity` alpha, matching the mockup's
/// `repeating-linear-gradient(..., rgba(0,0,0,var(--scanA)) 2px 4px)`. Drawn on
/// a foreground layer so the lines fall over the panels, as the mockup's
/// high-`z-index` `::after` pseudo-element does.
fn scanlines(ctx: &egui::Context, theme: &Theme, screen: Rect) {
    if theme.scanline_opacity <= 0.0 {
        return;
    }
    let alpha = (theme.scanline_opacity * 255.0).round().clamp(0.0, 255.0) as u8;
    if alpha == 0 {
        return;
    }
    let painter = ctx.layer_painter(LayerId::new(Order::Foreground, Id::new("pw_scanlines")));
    let stroke = Stroke::new(1.0, Color32::from_black_alpha(alpha));

    let mut y = screen.top();
    while y <= screen.bottom() {
        painter.hline(screen.x_range(), y, stroke);
        y += SCANLINE_STEP;
    }
}

/// Draw a soft dark vignette: a short stack of concentric 1 px rectangle
/// strokes just inside the window edge, fading out toward the centre.
///
/// Skipped unless `theme.vignette > 0`. The strokes step inward 1 px at a time;
/// alpha is highest at the outermost ring and decays linearly to zero, scaled
/// overall by `theme.vignette`. This is the cheap painter equivalent of the
/// mockup's `box-shadow: inset 0 0 110px 16px rgba(0,0,0,var(--vig))` — a dark
/// band hugging the frame, no blur pass required. Drawn on a foreground layer
/// so it darkens the panel edges too.
fn vignette(ctx: &egui::Context, theme: &Theme, screen: Rect) {
    if theme.vignette <= 0.0 {
        return;
    }
    let painter = ctx.layer_painter(LayerId::new(Order::Foreground, Id::new("pw_vignette")));

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
