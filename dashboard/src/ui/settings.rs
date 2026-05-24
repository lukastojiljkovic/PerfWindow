//! The settings modal: a centred floating window for theme, units and refresh.
//!
//! [`settings_modal`] is shown by the render method whenever `app.settings_open`
//! is set (toggled by the title-bar gear chip). It is a free-floating
//! [`egui::Window`] — not a panel — so it takes the [`egui::Context`] directly.
//! Every control applies live: there is no OK/Cancel. A change writes the new
//! value into `app.config`, then re-applies and persists it via
//! [`PerfApp::apply_config_change`] and [`crate::config::Config::save`].
//!
//! The window paints its own chrome (a `chrome` title bar with a `✕` button and
//! a `chrome` footer) in the active theme, so egui's default window visuals are
//! never seen. Matches `docs/mockups/settings-light.html`.

use crate::app::PerfApp;
use crate::config::{RefreshRate, ThemeId};
use crate::format::{letter_spaced, TempUnit};
use crate::theme::Theme;
use egui::{
    Align2, Color32, FontId, Margin, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Vec2,
};

/// Fixed outer width of the settings window (mockup `.cfg { width: 600px }`).
const WINDOW_WIDTH: f32 = 600.0;
/// Combined height of the title bar + footer + outer breathing room. The body's
/// `ScrollArea` is capped at `viewport_height - CHROME_RESERVE` so the chrome
/// stays on-screen even on a short app window.
const CHROME_RESERVE: f32 = 90.0;
/// Floor for the body's `ScrollArea` max height. Below this the modal becomes
/// useless even with scroll — pick something that still shows one section.
const MIN_BODY_HEIGHT: f32 = 180.0;
/// Title-bar / footer strip padding (mockup `.cfg-tb { padding: 12px 15px }`).
const TB_PADDING_X: i8 = 15;
const TB_PADDING_Y: i8 = 12;
/// Footer strip padding (mockup `.cfg-foot { padding: 10px 15px }`).
const FOOT_PADDING_X: i8 = 15;
const FOOT_PADDING_Y: i8 = 10;
/// Body padding (mockup `.cfg-body { padding: 19px 16px }`).
const BODY_PADDING_X: i8 = 16;
const BODY_PADDING_Y: i8 = 19;
/// Vertical gap between body sections (mockup `.cfg-body { gap: 21px }`).
const SECTION_GAP: f32 = 21.0;
/// Vertical gap between a section's label and its control (mockup `.cfg-sec { gap: 10px }`).
const SECTION_INNER_GAP: f32 = 10.0;
/// Gap between the four theme cards (mockup `.theme-grid { gap: 8px }`).
const THEME_CARD_GAP: f32 = 8.0;
/// Inner padding of one theme card (mockup `.tcard { padding: 8px }`).
const THEME_CARD_PADDING: i8 = 8;
/// Height of a theme card's preview strip (mockup `.tcard-prev { height: 52px }`).
const THEME_PREVIEW_H: f32 = 52.0;
/// Diameter of the preview ring (mockup `.pv-ring { width: 28px }`).
const PREVIEW_RING_D: f32 = 28.0;
/// Ring thickness — the mockup masks the centre with `inset: 6px`.
const PREVIEW_RING_THICK: f32 = 6.0;
/// Width of the three preview bars (mockup `.pv-bars { width: 36px }`).
const PREVIEW_BARS_W: f32 = 36.0;
/// Height of one preview bar (mockup `.pv-bars i { height: 4px }`).
const PREVIEW_BAR_H: f32 = 4.0;
/// Gap between the stacked preview bars (mockup `.pv-bars { gap: 4px }`).
const PREVIEW_BAR_GAP: f32 = 4.0;
/// Toggle track size (mockup `.tg { width: 34px; height: 18px }`).
const TOGGLE_W: f32 = 34.0;
const TOGGLE_H: f32 = 18.0;
/// Toggle knob size (mockup `.tg-dot { width: 12px; height: 12px }`).
const TOGGLE_DOT: f32 = 12.0;
/// Segmented-control segment padding (mockup `.seg-i { padding: 8px 15px }`).
const SEG_PADDING_X: f32 = 15.0;
const SEG_PADDING_Y: f32 = 8.0;
/// Section-label and hint font size (mockup `.cfg-label` / `.cfg-hint`).
const LABEL_FONT_SIZE: f32 = 10.0;

/// A pending change to `app.config`, queued by a control and applied after the
/// window closure returns so `app` is not borrowed twice.
enum Change {
    Theme(ThemeId),
    Follow(bool),
    Unit(TempUnit),
    Refresh(RefreshRate),
    CheckUpdates(bool),
    ManualCheck,
}

/// Draw the settings modal when `app.settings_open` is set.
///
/// Renders a fixed-width, non-resizable, centred [`egui::Window`] with custom
/// chrome in the active theme. The window's own title bar is disabled; a
/// `chrome` strip with a `✕` button is painted instead. Closing it — via the
/// `✕` button — clears `app.settings_open`. Any control change is applied live
/// through [`PerfApp::apply_config_change`] and persisted with
/// [`crate::config::Config::save`].
pub fn settings_modal(ctx: &egui::Context, app: &mut PerfApp) {
    if !app.settings_open {
        return;
    }
    let theme = app.theme.clone();

    // The window's `Frame` carries the theme background, a 1 px border and a
    // soft drop shadow (mockup `.cfg { box-shadow: 0 26px 64px ... }`). No
    // inner margin: the title bar, body and footer each set their own padding.
    let frame = egui::Frame::NONE
        .fill(theme.bg)
        .stroke(Stroke::new(1.0, theme.border))
        .shadow(egui::epaint::Shadow {
            offset: [0, 18],
            blur: 48,
            spread: 0,
            color: Color32::from_black_alpha(140),
        });

    // Collected inside the closure, applied once afterwards to keep `app`'s
    // borrow single. `close` is set by the `✕` button.
    let mut change: Option<Change> = None;
    let mut close = false;

    egui::Window::new("settings")
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        // Width fixed to 600; height unconstrained (`f32::INFINITY`) so the
        // window auto-sizes to its content.
        .fixed_size(Vec2::new(WINDOW_WIDTH, f32::INFINITY))
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .frame(frame)
        .show(ctx, |ui| {
            // The fixed-size resize area can still leave the content narrower
            // than requested; pin the width so every strip spans the full 600.
            ui.set_width(WINDOW_WIDTH);

            if title_bar(ui, &theme).clicked() {
                close = true;
            }

            // Cap the body to whatever vertical room is left in the app window
            // after reserving the title bar + footer. When the body fits, the
            // `ScrollArea` shrinks to its content; when it does not, the
            // scrollbar appears so the title bar's ✕ and the footer remain
            // reachable on a short window. Without this the modal grew to its
            // content's natural height and clipped past the viewport.
            let body_max_h =
                (ctx.content_rect().height() - CHROME_RESERVE).max(MIN_BODY_HEIGHT);
            egui::ScrollArea::vertical()
                .max_height(body_max_h)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    egui::Frame::NONE
                        .inner_margin(Margin::symmetric(BODY_PADDING_X, BODY_PADDING_Y))
                        .show(ui, |ui| {
                            ui.spacing_mut().item_spacing.y = SECTION_GAP;

                            theme_section(ui, &theme, app, &mut change);
                            unit_section(ui, &theme, app.config.unit, &mut change);
                            refresh_section(ui, &theme, app.config.refresh, &mut change);
                            updates_section(ui, &theme, app, &mut change);
                        });
                });

            footer(ui, &theme);
        });

    // Apply the queued change after the closure: writing `config`, re-applying
    // the theme and persisting to disk all happen exactly once per frame.
    if let Some(change) = change {
        let needs_save = !matches!(change, Change::ManualCheck);
        match change {
            Change::Theme(id) => app.config.theme = id,
            Change::Follow(on) => app.config.follow_windows = on,
            Change::Unit(unit) => app.config.unit = unit,
            Change::Refresh(rate) => app.config.refresh = rate,
            Change::CheckUpdates(on) => app.config.check_updates_on_startup = on,
            Change::ManualCheck => app.manual_update_check(ctx),
        }
        if needs_save {
            app.apply_config_change(ctx);
            app.config.save();
        }
    }
    if close {
        app.settings_open = false;
    }
}

/// Draw the modal title bar: a `chrome` strip with the `⚙ SETTINGS` wordmark on
/// the left and a bordered `✕` close button on the right, closed by a 1 px
/// `border` bottom rule. Returns the close button's click response. Matches
/// `.cfg-tb` / `.cfg-x` in the mockup.
fn title_bar(ui: &mut egui::Ui, theme: &Theme) -> Response {
    let inner = egui::Frame::NONE
        .fill(theme.chrome)
        .inner_margin(Margin::symmetric(TB_PADDING_X, TB_PADDING_Y))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(letter_spaced("\u{2699} SETTINGS"))
                        .family(theme.font_display.egui())
                        .size(13.0)
                        .color(theme.accent),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    close_button(ui, theme)
                })
                .inner
            })
            .inner
        });

    // 1 px `border` rule along the strip's bottom edge (mockup `.cfg-tb`).
    let rect = inner.response.rect;
    ui.painter().add(egui::Shape::line_segment(
        [rect.left_bottom(), rect.right_bottom()],
        Stroke::new(1.0, theme.border),
    ));
    inner.inner
}

/// Draw the `✕` close button: a `dim` glyph inside a 1 px `border` box that
/// fills `accent` on hover. Returns its click-sensing response. Matches
/// `.cfg-x` in the mockup.
fn close_button(ui: &mut egui::Ui, theme: &Theme) -> Response {
    let font = FontId::new(12.0, theme.font_data.egui());
    // Padding mirrors the mockup's `.cfg-x { padding: 4px 9px }`.
    let pad = Vec2::new(9.0, 4.0);
    // Laid out with `PLACEHOLDER` so the hover-dependent `text_color` below
    // tints the glyph via `painter.galley`'s fallback colour.
    let galley = ui
        .painter()
        .layout_no_wrap("\u{2715}".to_owned(), font, Color32::PLACEHOLDER);
    let size = galley.size() + pad * 2.0;
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());

    if ui.is_rect_visible(rect) {
        let painter = ui.painter_at(rect);
        let hovered = response.hovered();
        let (fill, stroke_color, text_color) = if hovered {
            (theme.accent, theme.accent, theme.bg)
        } else {
            (Color32::TRANSPARENT, theme.border, theme.dim)
        };
        if fill != Color32::TRANSPARENT {
            painter.rect_filled(rect, 0.0, fill);
        }
        painter.rect_stroke(
            rect,
            0.0,
            Stroke::new(1.0, stroke_color),
            StrokeKind::Inside,
        );
        painter.galley(rect.min + pad, galley, text_color);
    }
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
}

/// Draw a section header: a letter-spaced, ~10 px `accent` label in the display
/// font (mockup `.cfg-label`).
fn section_label(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    ui.label(
        egui::RichText::new(letter_spaced(text))
            .family(theme.font_display.egui())
            .size(LABEL_FONT_SIZE)
            .color(theme.accent),
    );
}

/// Draw a section's hint line: ~10 px `dim` data-font text that wraps inside
/// the window width (mockup `.cfg-hint`).
fn section_hint(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .family(theme.font_data.egui())
            .size(LABEL_FONT_SIZE)
            .color(theme.dim),
    );
}

/// Draw the THEME section: the four-card theme picker followed by the
/// "Follow Windows" toggle.
fn theme_section(ui: &mut egui::Ui, theme: &Theme, app: &PerfApp, change: &mut Option<Change>) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = SECTION_INNER_GAP;
        section_label(ui, theme, "THEME");
        theme_grid(ui, theme, app.config.theme, change);
        follow_toggle(ui, theme, app.config.follow_windows, change);
    });
}

/// The six themes offered, in picker order: Light, Amber, Slate, Phosphor,
/// Synthwave, Crimson — each with the short tag shown under its name.
const THEME_CARDS: [(ThemeId, &str); 6] = [
    (ThemeId::Light,     "LIGHT"),
    (ThemeId::Amber,     "DARK"),
    (ThemeId::Slate,     "DARK"),
    (ThemeId::Phosphor,  "DARK"),
    (ThemeId::Synthwave, "DARK"),
    (ThemeId::Crimson,   "DARK"),
];

/// Draw the theme-card grid as 3 columns x 2 rows (6 cards total). `selected`
/// is the currently configured theme; clicking a different card queues a
/// [`Change::Theme`]. Matches `.theme-grid` in the mockup.
fn theme_grid(ui: &mut egui::Ui, theme: &Theme, selected: ThemeId, change: &mut Option<Change>) {
    const COLS: usize = 3;
    let card_w = (ui.available_width() - THEME_CARD_GAP * (COLS as f32 - 1.0)) / COLS as f32;
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = THEME_CARD_GAP;
        for row in THEME_CARDS.chunks(COLS) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = THEME_CARD_GAP;
                for &(id, tag) in row {
                    ui.allocate_ui_with_layout(
                        Vec2::new(card_w, 0.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.set_width(card_w);
                            if theme_card(ui, theme, id, tag, id == selected).clicked()
                                && id != selected
                            {
                                *change = Some(Change::Theme(id));
                            }
                        },
                    );
                }
            });
        }
    });
}

/// Draw one theme card: a tiny preview (a ring + three bars in `id`'s palette),
/// the theme name and a short tag. The selected card gets an `accent` border
/// and a 1 px inset `accent` outline. Returns the click response. Matches
/// `.tcard` / `.tcard.sel` in the mockup.
fn theme_card(
    ui: &mut egui::Ui,
    theme: &Theme,
    id: ThemeId,
    tag: &str,
    selected: bool,
) -> Response {
    let preview = Theme::for_id(id);

    let frame = egui::Frame::NONE
        .fill(theme.panel)
        .stroke(Stroke::new(
            1.0,
            if selected { theme.accent } else { theme.border },
        ))
        .inner_margin(Margin::same(THEME_CARD_PADDING));

    let inner = frame.show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 7.0;
        theme_preview(ui, &preview);
        // Card name — ~11 px data font in `ink` (mockup `.tcard-name`).
        ui.label(
            egui::RichText::new(preview.name)
                .family(theme.font_data.egui())
                .size(11.0)
                .color(theme.ink),
        );
        // Short tag — ~8 px letter-spaced `dim` (mockup `.tcard-tag`).
        ui.label(
            egui::RichText::new(letter_spaced(tag))
                .family(theme.font_data.egui())
                .size(8.0)
                .color(theme.dim),
        );
    });

    let rect = inner.response.rect;
    // The selected card carries a second 1 px `accent` rule just inside its
    // border (mockup `.tcard.sel { box-shadow: inset 0 0 0 1px var(--accent) }`).
    if selected {
        ui.painter().rect_stroke(
            rect.shrink(1.0),
            0.0,
            Stroke::new(1.0, theme.accent),
            StrokeKind::Inside,
        );
    }
    let response = ui.interact(rect, inner.response.id, Sense::click());
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
}

/// Paint the tiny theme preview inside a card: a bordered 52 px strip holding a
/// donut ring and three stacked bars, all in `preview`'s own palette. Matches
/// `.tcard-prev` plus `.pv-ring` / `.pv-bars` in the mockup.
fn theme_preview(ui: &mut egui::Ui, preview: &Theme) {
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), THEME_PREVIEW_H),
        Sense::hover(),
    );
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter_at(rect);

    // The preview strip paints in the previewed theme's background, with a
    // 1 px border in the *outer* theme so card edges stay consistent.
    painter.rect_filled(rect, 0.0, preview.bg);

    // Lay the ring and the bar stack out as one centred group, 8 px apart
    // (mockup `.tcard-prev { gap: 8px }`).
    let group_w = PREVIEW_RING_D + 8.0 + PREVIEW_BARS_W;
    let group_x0 = rect.center().x - group_w / 2.0;
    let ring_center = Pos2::new(group_x0 + PREVIEW_RING_D / 2.0, rect.center().y);
    preview_ring(&painter, preview, ring_center);

    let bars_x0 = group_x0 + PREVIEW_RING_D + 8.0;
    preview_bars(&painter, preview, bars_x0, rect.center().y);
}

/// Draw the preview ring: a 28 px donut filled ~64 % with the previewed
/// theme's `accent` over its `track`, centre masked with its `panel`.
fn preview_ring(painter: &egui::Painter, preview: &Theme, center: Pos2) {
    use std::f32::consts::TAU;
    let radius = PREVIEW_RING_D / 2.0;
    let mid_r = radius - PREVIEW_RING_THICK / 2.0;

    // Approximate each arc as a short polyline; 64 % filled like the mockup's
    // `conic-gradient(var(--pa) 0 64%, var(--pt) 64% 100%)`.
    let arc = |from: f32, to: f32, color: Color32| {
        let n = (((to - from).abs() * 64.0).ceil() as usize + 1).max(2);
        let pts: Vec<Pos2> = (0..n)
            .map(|i| {
                let t = from + (to - from) * (i as f32 / (n - 1) as f32);
                let a = t * TAU - std::f32::consts::FRAC_PI_2;
                center + Vec2::new(a.cos() * mid_r, a.sin() * mid_r)
            })
            .collect();
        painter.add(egui::Shape::line(
            pts,
            Stroke::new(PREVIEW_RING_THICK, color),
        ));
    };
    // The full track ring is a plain stroked circle — an open polyline would
    // leave a hairline seam where its ends meet; the accent is a partial arc.
    painter.circle_stroke(
        center,
        mid_r,
        Stroke::new(PREVIEW_RING_THICK, preview.track),
    );
    arc(0.0, 0.64, preview.accent);
    painter.circle_filled(center, radius - PREVIEW_RING_THICK, preview.panel);
}

/// Draw the three stacked preview bars: 36 px wide, 4 px tall, in the
/// previewed theme's `accent` at decreasing opacity (1.0 / 0.55 / 0.28),
/// matching the mockup's `.pv-bars i` opacity steps.
fn preview_bars(painter: &egui::Painter, preview: &Theme, x0: f32, center_y: f32) {
    const OPACITIES: [f32; 3] = [1.0, 0.55, 0.28];
    let stack_h = PREVIEW_BAR_H * 3.0 + PREVIEW_BAR_GAP * 2.0;
    let mut y = center_y - stack_h / 2.0;
    for opacity in OPACITIES {
        let bar = Rect::from_min_size(Pos2::new(x0, y), Vec2::new(PREVIEW_BARS_W, PREVIEW_BAR_H));
        painter.rect_filled(bar, 0.0, preview.accent.gamma_multiply(opacity));
        y += PREVIEW_BAR_H + PREVIEW_BAR_GAP;
    }
}

/// Draw the "Follow Windows" toggle: a 34×18 switch followed by a label and a
/// dimmed hint. Toggling it queues a [`Change::Follow`]. Matches `.cfg-toggle`
/// in the mockup.
fn follow_toggle(ui: &mut egui::Ui, theme: &Theme, on: bool, change: &mut Option<Change>) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 11.0;

        // The switch is one allocation; the whole row is clickable.
        let (track_rect, response) =
            ui.allocate_exact_size(Vec2::new(TOGGLE_W, TOGGLE_H), Sense::click());
        if ui.is_rect_visible(track_rect) {
            let painter = ui.painter_at(track_rect);
            let (track_fill, track_stroke) = if on {
                (theme.accent, theme.accent)
            } else {
                (theme.track, theme.border)
            };
            painter.rect_filled(track_rect, 0.0, track_fill);
            painter.rect_stroke(
                track_rect,
                0.0,
                Stroke::new(1.0, track_stroke),
                StrokeKind::Inside,
            );
            // Knob: 2 px inset, slid to the right edge when on (mockup
            // `.tg-dot { top: 2px; left: 2px }` / `.on .tg-dot { left: 18px }`).
            let knob_x = if on {
                track_rect.max.x - 2.0 - TOGGLE_DOT
            } else {
                track_rect.min.x + 2.0
            };
            let knob = Rect::from_min_size(
                Pos2::new(knob_x, track_rect.min.y + 2.0),
                Vec2::splat(TOGGLE_DOT),
            );
            let knob_color = if on { theme.bg } else { theme.dim };
            painter.rect_filled(knob, 0.0, knob_color);
        }
        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }

        // Label: the ink lead-in and the dimmed hint share one wrapped line
        // (mockup `.cfg-toggle .lbl` with the inner `<em>`).
        let mut job = egui::text::LayoutJob::default();
        let data_font = FontId::new(11.0, theme.font_data.egui());
        job.wrap.max_width = ui.available_width();
        job.append(
            "Follow Windows",
            0.0,
            egui::TextFormat {
                font_id: data_font.clone(),
                color: theme.ink,
                ..Default::default()
            },
        );
        job.append(
            "  \u{2014} a light OS uses Light; a dark OS uses the chosen dark theme",
            0.0,
            egui::TextFormat {
                font_id: data_font,
                color: theme.dim,
                ..Default::default()
            },
        );
        ui.label(job);

        if response.clicked() {
            *change = Some(Change::Follow(!on));
        }
    });
}

/// Draw the UNITS · TEMPERATURE section: a two-segment `°C` / `°F` control.
fn unit_section(ui: &mut egui::Ui, theme: &Theme, unit: TempUnit, change: &mut Option<Change>) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = SECTION_INNER_GAP;
        section_label(ui, theme, "UNITS \u{00b7} TEMPERATURE");
        let segments = [
            ("\u{00b0}C", unit == TempUnit::Celsius),
            ("\u{00b0}F", unit == TempUnit::Fahrenheit),
        ];
        if let Some(picked) = segmented(ui, theme, &segments) {
            let chosen = if picked == 0 {
                TempUnit::Celsius
            } else {
                TempUnit::Fahrenheit
            };
            if chosen != unit {
                *change = Some(Change::Unit(chosen));
            }
        }
    });
}

/// Draw the REFRESH section: a four-segment poll-interval control plus a hint.
fn refresh_section(
    ui: &mut egui::Ui,
    theme: &Theme,
    refresh: RefreshRate,
    change: &mut Option<Change>,
) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = SECTION_INNER_GAP;
        section_label(ui, theme, "REFRESH");
        let segments: Vec<(&str, bool)> = RefreshRate::ALL
            .iter()
            .map(|&rate| (rate.label(), rate == refresh))
            .collect();
        if let Some(picked) = segmented(ui, theme, &segments) {
            let chosen = RefreshRate::ALL[picked];
            if chosen != refresh {
                *change = Some(Change::Refresh(chosen));
            }
        }
        section_hint(
            ui,
            theme,
            "Slower refresh = less load while the app is open. 1 s is the default.",
        );
    });
}

/// Draw a segmented control: a single bordered strip of equal-height segments,
/// divided by 1 px `border` rules. The `on` segment fills `accent` with
/// `bg`-coloured text; the rest are `dim` labels. Returns `Some(index)` of a
/// clicked segment, or `None`. Matches `.seg` / `.seg-i` in the mockup.
fn segmented(ui: &mut egui::Ui, theme: &Theme, segments: &[(&str, bool)]) -> Option<usize> {
    let font = FontId::new(11.0, theme.font_data.egui());

    // Lay every segment's text out first so the row height (and each segment's
    // width) can be derived before any allocation. `layout_no_wrap` returns a
    // cache-shared `Arc<Galley>`; keep the `Arc` rather than deep-cloning it.
    // The glyphs are `PLACEHOLDER` so each segment's `painter.galley` fallback
    // colour can tint them (`bg` for the active segment, `dim` for the rest).
    let galleys: Vec<std::sync::Arc<egui::Galley>> = segments
        .iter()
        .map(|(label, _)| {
            ui.painter()
                .layout_no_wrap((*label).to_owned(), font.clone(), Color32::PLACEHOLDER)
        })
        .collect();
    let text_h = galleys.iter().map(|g| g.size().y).fold(0.0_f32, f32::max);
    let seg_h = text_h + SEG_PADDING_Y * 2.0;
    let seg_widths: Vec<f32> = galleys
        .iter()
        .map(|g| g.size().x + SEG_PADDING_X * 2.0)
        .collect();
    let total_w: f32 = seg_widths.iter().sum();

    let (rect, _) = ui.allocate_exact_size(Vec2::new(total_w, seg_h), Sense::hover());
    let mut clicked = None;

    if ui.is_rect_visible(rect) {
        let painter = ui.painter_at(rect);
        let mut x = rect.min.x;
        for (i, ((label, on), galley)) in segments.iter().zip(galleys).enumerate() {
            let seg_rect =
                Rect::from_min_size(Pos2::new(x, rect.min.y), Vec2::new(seg_widths[i], seg_h));

            // An active segment fills `accent`; the glyphs flip to `bg`.
            let text_color = if *on { theme.bg } else { theme.dim };
            if *on {
                painter.rect_filled(seg_rect, 0.0, theme.accent);
            }
            let text_pos = Pos2::new(
                seg_rect.center().x - galley.size().x / 2.0,
                seg_rect.center().y - galley.size().y / 2.0,
            );
            painter.galley(text_pos, galley, text_color);

            // Divider rule between this segment and the next (mockup
            // `.seg-i { border-right: 1px solid var(--border) }`).
            if i + 1 < segments.len() {
                painter.add(egui::Shape::line_segment(
                    [
                        Pos2::new(seg_rect.max.x, seg_rect.min.y),
                        Pos2::new(seg_rect.max.x, seg_rect.max.y),
                    ],
                    Stroke::new(1.0, theme.border),
                ));
            }

            // Per-segment hit-testing — the strip is one allocation, so each
            // segment is interacted with separately by its own id.
            let response = ui.interact(
                seg_rect,
                ui.id().with(("settings_seg", label, i)),
                Sense::click(),
            );
            if response.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if response.clicked() {
                clicked = Some(i);
            }

            x += seg_widths[i];
        }

        // The single 1 px outline around the whole strip (mockup `.seg`).
        painter.rect_stroke(
            rect,
            0.0,
            Stroke::new(1.0, theme.border),
            StrokeKind::Inside,
        );
    }

    clicked
}

/// Draw the modal footer: a `chrome` strip with the version / licence credit,
/// opened by a 1 px `border` top rule. Matches `.cfg-foot` in the mockup.
fn footer(ui: &mut egui::Ui, theme: &Theme) {
    egui::Frame::NONE
        .fill(theme.chrome)
        .inner_margin(Margin::symmetric(FOOT_PADDING_X, FOOT_PADDING_Y))
        .show(ui, |ui| {
            // The 1 px top rule — drawn explicitly because the strip is filled
            // and the body above carries no bottom border of its own.
            let rect = ui.max_rect();
            ui.painter().add(egui::Shape::line_segment(
                [rect.left_top(), rect.right_top()],
                Stroke::new(1.0, theme.border),
            ));
            ui.label(
                egui::RichText::new(format!(
                    "PerfWindow v{} \u{00b7} MIT license \u{00b7} sensors: \
                     LibreHardwareMonitor (MPL-2.0)",
                    env!("CARGO_PKG_VERSION"),
                ))
                .family(theme.font_data.egui())
                .size(9.0)
                .color(theme.dim),
            );
        });
}

/// The UPDATES section: current version, opt-out toggle, manual check,
/// last-checked stamp.
fn updates_section(
    ui: &mut egui::Ui,
    theme: &Theme,
    app: &PerfApp,
    change: &mut Option<Change>,
) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = SECTION_INNER_GAP;
        section_label(ui, theme, "UPDATES");

        ui.label(
            egui::RichText::new(format!("Current version: {}", env!("CARGO_PKG_VERSION")))
                .family(theme.font_data.egui())
                .size(11.0)
                .color(theme.ink),
        );

        let on = app.config.check_updates_on_startup;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 11.0;

            let (track_rect, response) =
                ui.allocate_exact_size(Vec2::new(TOGGLE_W, TOGGLE_H), Sense::click());
            if ui.is_rect_visible(track_rect) {
                let painter = ui.painter_at(track_rect);
                let (track_fill, track_stroke) = if on {
                    (theme.accent, theme.accent)
                } else {
                    (theme.track, theme.border)
                };
                painter.rect_filled(track_rect, 0.0, track_fill);
                painter.rect_stroke(
                    track_rect,
                    0.0,
                    Stroke::new(1.0, track_stroke),
                    StrokeKind::Inside,
                );
                let knob_x = if on {
                    track_rect.max.x - 2.0 - TOGGLE_DOT
                } else {
                    track_rect.min.x + 2.0
                };
                let knob = Rect::from_min_size(
                    Pos2::new(knob_x, track_rect.min.y + 2.0),
                    Vec2::splat(TOGGLE_DOT),
                );
                let knob_color = if on { theme.bg } else { theme.dim };
                painter.rect_filled(knob, 0.0, knob_color);
            }
            if response.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if response.clicked() {
                *change = Some(Change::CheckUpdates(!on));
            }
            ui.label(
                egui::RichText::new("Check for updates on startup")
                    .family(theme.font_data.egui())
                    .size(11.0)
                    .color(theme.ink),
            );
        });

        ui.horizontal(|ui| {
            let stamp = format_checked_at(&app.update_state.lock().unwrap());
            ui.label(
                egui::RichText::new(format!("Last checked: {stamp}"))
                    .family(theme.font_data.egui())
                    .size(10.0)
                    .color(theme.dim),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if check_now_button(ui, theme).clicked() {
                    *change = Some(Change::ManualCheck);
                }
            });
        });
    });
}

fn check_now_button(ui: &mut egui::Ui, theme: &Theme) -> egui::Response {
    let font = FontId::new(10.0, theme.font_data.egui());
    let galley = ui
        .painter()
        .layout_no_wrap("Check for updates now".to_owned(), font, Color32::PLACEHOLDER);
    let pad = Vec2::new(10.0, 5.0);
    let size = galley.size() + pad * 2.0;
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());

    if ui.is_rect_visible(rect) {
        let painter = ui.painter_at(rect);
        let (fill, stroke_color, text_color) = if response.hovered() {
            (theme.accent, theme.accent, theme.bg)
        } else {
            (Color32::TRANSPARENT, theme.border, theme.dim)
        };
        if fill != Color32::TRANSPARENT {
            painter.rect_filled(rect, 0.0, fill);
        }
        painter.rect_stroke(rect, 0.0, Stroke::new(1.0, stroke_color), StrokeKind::Inside);
        painter.galley(rect.min + pad, galley, text_color);
    }
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
}

fn format_checked_at(state: &crate::update::UpdateState) -> String {
    use crate::update::UpdateState;
    match state {
        UpdateState::Idle => "never".to_string(),
        UpdateState::Checking => "checking now\u{2026}".to_string(),
        UpdateState::NoUpdate { checked_at }
        | UpdateState::Available { checked_at, .. } => format_systemtime(*checked_at),
        UpdateState::Failed { checked_at, reason } => {
            format!("{} (failed: {reason})", format_systemtime(*checked_at))
        }
    }
}

fn format_systemtime(t: std::time::SystemTime) -> String {
    use std::time::UNIX_EPOCH;
    let secs = t.duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let (year, month, day, hour, minute) = ymdhm_local(secs);
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}")
}

/// Convert a Unix timestamp to a local Y/M/D H/M tuple via the Win32
/// `FileTimeToSystemTime` chain. Kept self-contained to avoid a date
/// library dependency for one timestamp.
fn ymdhm_local(unix_secs: u64) -> (i32, u32, u32, u32, u32) {
    use std::ptr::null_mut;

    type DWORD = u32;
    type WORD = u16;
    #[repr(C)]
    struct SYSTEMTIME {
        w_year: WORD,
        w_month: WORD,
        w_day_of_week: WORD,
        w_day: WORD,
        w_hour: WORD,
        w_minute: WORD,
        w_second: WORD,
        w_milliseconds: WORD,
    }
    #[repr(C)]
    struct FILETIME {
        dw_low: DWORD,
        dw_high: DWORD,
    }

    extern "system" {
        fn FileTimeToSystemTime(p_ft: *const FILETIME, p_st: *mut SYSTEMTIME) -> i32;
        fn SystemTimeToTzSpecificLocalTime(
            tz: *const u8,
            p_utc: *const SYSTEMTIME,
            p_local: *mut SYSTEMTIME,
        ) -> i32;
    }

    let ft_100ns: u64 = (unix_secs + 11_644_473_600) * 10_000_000;
    let ft = FILETIME {
        dw_low: (ft_100ns & 0xFFFF_FFFF) as u32,
        dw_high: (ft_100ns >> 32) as u32,
    };
    let mut utc = SYSTEMTIME {
        w_year: 0,
        w_month: 0,
        w_day_of_week: 0,
        w_day: 0,
        w_hour: 0,
        w_minute: 0,
        w_second: 0,
        w_milliseconds: 0,
    };
    let mut local = SYSTEMTIME {
        w_year: 0,
        w_month: 0,
        w_day_of_week: 0,
        w_day: 0,
        w_hour: 0,
        w_minute: 0,
        w_second: 0,
        w_milliseconds: 0,
    };
    unsafe {
        if FileTimeToSystemTime(&ft as *const _, &mut utc as *mut _) == 0 {
            return (1970, 1, 1, 0, 0);
        }
        if SystemTimeToTzSpecificLocalTime(null_mut(), &utc as *const _, &mut local as *mut _) == 0
        {
            local = utc;
        }
    }
    (
        local.w_year as i32,
        local.w_month as u32,
        local.w_day as u32,
        local.w_hour as u32,
        local.w_minute as u32,
    )
}
