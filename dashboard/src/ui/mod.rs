//! The dashboard chrome: the title bar, the adaptive card grid and the footer.
//!
//! These are the three regions the render method assembles every frame —
//! [`title_bar`] in a top panel, [`footer`] in a bottom panel and [`card_grid`]
//! in the central scroll area. The grid simply orders the [`crate::panels`]
//! cards from the latest snapshot and places them; each panel paints its own
//! [`crate::panels::card`] frame, so the grid never wraps a cell itself.
//!
//! The [`settings`] submodule adds the floating settings modal, drawn on top of
//! everything else when the title-bar gear is toggled on. The [`effects`]
//! submodule paints the retro CRT overlay — grid, scanlines, vignette — last of
//! all each frame.

pub mod capacity;
pub mod changelog_modal;
pub mod effects;
pub mod fit;
pub mod health_banner;
pub mod loading_screen;
pub mod mini_strip;
pub mod modal;
// The geometry recorder is a test-only instrument: production builds link the
// empty stub so the shipping binary never pays for a rectangle it discards.
#[cfg(any(test, feature = "test-support"))]
pub mod recorder;
#[cfg(not(any(test, feature = "test-support")))]
#[path = "recorder_stub.rs"]
pub mod recorder;
pub mod settings;
pub mod shell;
pub mod stat_priority;
pub mod tooltips;
pub mod update_banner;
pub mod update_modal;

use crate::app::{PerfApp, Status};
use crate::config::ThemeId;
use crate::format::{
    finite, format_bytes_per_sec, format_percent, format_uptime, letter_spaced, TempUnit,
};
use crate::ipc::connect::ConnectPhase;
use crate::panels;
use crate::theme::Theme;
use egui::{Align, FontId, Layout, Margin, RichText, Sense, Stroke, Vec2};

/// Title-bar / footer strip padding.
const STRIP_PADDING_X: i8 = 13;
const STRIP_PADDING_Y_TB: i8 = 9;
const STRIP_PADDING_Y_FOOT: i8 = 7;
/// Gap between cards in the grid.
const GRID_GAP: f32 = 10.0;
/// Inner padding around the grid.
const GRID_BODY_PADDING: i8 = 13;
/// Period, in seconds, of the wordmark's blinking block cursor.
const CURSOR_BLINK_PERIOD: f64 = 1.1;

/// Draw the title bar: a `chrome`-filled strip with the wordmark on the left and
/// a row of control chips on the right, closed by a 1 px `border` bottom rule.
///
/// Clicking a chip mutates `app.config` and immediately re-applies and persists
/// it via [`PerfApp::apply_config_change`] and [`crate::config::Config::save`].
pub fn title_bar(ui: &mut egui::Ui, app: &mut PerfApp) {
    let theme = app.theme.clone();
    let frame = egui::Frame::NONE
        .fill(theme.chrome)
        .inner_margin(Margin::symmetric(STRIP_PADDING_X, STRIP_PADDING_Y_TB));

    frame.show(ui, |ui| {
        ui.horizontal(|ui| {
            wordmark(ui, &theme);
            // Chips are laid out from the right edge inward.
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                chip_row(ui, &theme, app);
            });
        });
    });
}

/// Paint the `▚ PERFWINDOW` wordmark with a blinking block cursor.
///
/// The wordmark is the display font in `theme.accent`, letter-spaced. The
/// trailing `▮` toggles on a ~1.1 s cycle driven by `ui.input(|i| i.time)`, so
/// it keeps blinking as long as the UI repaints.
fn wordmark(ui: &mut egui::Ui, theme: &Theme) {
    let (time, focused) = ui.input(|i| (i.time, i.focused));
    // On for the first half of each period, off for the second.
    let phase = time % CURSOR_BLINK_PERIOD;
    let cursor_visible = phase < CURSOR_BLINK_PERIOD / 2.0;
    // egui's `time` only advances when a frame is drawn, and the refresh-rate
    // watchdog repaints far too slowly for a 1.1 s blink. Schedule a repaint
    // exactly at the next on/off transition so the cursor keeps proper time —
    // but only while the window has focus: a background window must not wake
    // the compositor ~2x per second for a cursor nobody is looking at.
    if focused {
        let to_next_edge = if cursor_visible {
            CURSOR_BLINK_PERIOD / 2.0 - phase
        } else {
            CURSOR_BLINK_PERIOD - phase
        };
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_secs_f64(to_next_edge));
    }

    ui.spacing_mut().item_spacing.x = 0.0;
    ui.label(
        RichText::new(letter_spaced("\u{259a} PERFWINDOW"))
            .family(theme.font_display.egui())
            .size(13.0)
            .color(theme.accent),
    );
    // The cursor occupies a fixed slot whether visible or not, so the chips to
    // its right never jitter as it blinks.
    let cursor_color = if cursor_visible {
        theme.accent
    } else {
        egui::Color32::TRANSPARENT
    };
    ui.label(
        RichText::new("\u{25ae}")
            .family(theme.font_display.egui())
            .size(13.0)
            .color(cursor_color),
    );
}

/// Draw the right-hand chip row, newest (rightmost) first.
///
/// The `right_to_left` layout means chips are emitted in reverse visual order:
/// gear, theme, `°F`, `°C` — yielding `°C  °F  ◐ THEME  ⚙` on screen.
fn chip_row(ui: &mut egui::Ui, theme: &Theme, app: &mut PerfApp) {
    ui.spacing_mut().item_spacing.x = 6.0;

    // Settings gear.
    if chip(ui, theme, "\u{2699}", false).clicked() {
        app.settings_open = !app.settings_open;
    }

    // Theme cycler.
    if chip(ui, theme, "\u{25d0} THEME", false).clicked() {
        let ctx = ui.ctx().clone();
        app.config.theme = next_theme(app.config.theme);
        app.apply_config_change(&ctx);
        app.config.save();
    }

    // CPU heat-map toggle: `▦` (U+25A6 squared crosshatch).
    if chip(ui, theme, "\u{25A6}", app.config.cpu_heat_map).clicked() {
        let ctx = ui.ctx().clone();
        app.config.cpu_heat_map = !app.config.cpu_heat_map;
        app.apply_config_change(&ctx);
        app.config.save();
    }

    // Always-on-top toggle: `📌` (U+1F4CC pushpin). The viewport switch
    // happens in `PerfApp::sync_window_level` on the next frame, driven by
    // the new `config.always_on_top`.
    if chip(ui, theme, "\u{1F4CC}", app.config.always_on_top).clicked() {
        app.config.always_on_top = !app.config.always_on_top;
        app.config.save();
    }

    // Mini strip: the whole window collapses into a taskbar-style bar docked
    // to the top of the monitor. `PerfApp::enter_strip` owns the viewport and
    // appbar side of the switch, and persists it.
    let mini = chip(ui, theme, "MINI", false).on_hover_text("Mini strip \u{00b7} Ctrl+M");
    if mini.clicked() {
        let ctx = ui.ctx().clone();
        app.enter_strip(&ctx);
    }

    // Fahrenheit / Celsius — the active unit is filled.
    let unit = app.config.unit;
    if chip(ui, theme, "\u{00b0}F", unit == TempUnit::Fahrenheit).clicked()
        && unit != TempUnit::Fahrenheit
    {
        let ctx = ui.ctx().clone();
        app.config.unit = TempUnit::Fahrenheit;
        app.apply_config_change(&ctx);
        app.config.save();
    }
    if chip(ui, theme, "\u{00b0}C", unit == TempUnit::Celsius).clicked()
        && unit != TempUnit::Celsius
    {
        let ctx = ui.ctx().clone();
        app.config.unit = TempUnit::Celsius;
        app.apply_config_change(&ctx);
        app.config.save();
    }
}

/// Draw one title-bar chip: a small bordered box with letter-spaced ~10 px
/// `label`. When `on`, it is filled `accent` with `bg`-coloured text; otherwise
/// it is a `dim` label inside a 1 px `border` outline. Returns the click-sensing
/// response.
fn chip(ui: &mut egui::Ui, theme: &Theme, label: &str, on: bool) -> egui::Response {
    let font = FontId::new(10.0, theme.font_data.egui());
    let text_color = if on { theme.bg } else { theme.dim };
    let galley = ui
        .painter()
        .layout_no_wrap(letter_spaced(label), font, text_color);

    let pad = Vec2::new(8.0, 5.0);
    let size = galley.size() + pad * 2.0;
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());

    if ui.is_rect_visible(rect) {
        let painter = ui.painter_at(rect);
        // Active chip: `accent` fill with an `accent` outline. Inactive chip:
        // no fill, a 1 px `border` outline.
        let stroke_color = if on { theme.accent } else { theme.border };
        if on {
            painter.rect_filled(rect, 0.0, theme.accent);
        }
        painter.rect_stroke(
            rect,
            0.0,
            Stroke::new(1.0_f32, stroke_color),
            egui::StrokeKind::Inside,
        );
        let text_pos = rect.min + pad;
        painter.galley(text_pos, galley, text_color);
    }
    response
}

/// Draw the footer: a `chrome`-filled strip opened by a 1 px `border` top rule.
///
/// Left to right: a status dot — the running app version when sensord is
/// streaming, NO SIGNAL when it isn't — the configured refresh rate, system
/// uptime, and the latest snapshot's headline CPU / GPU / network figures.
/// While `sensord` is alive, the version label is a clickable widget that
/// opens the in-app changelog viewer; in `NO SIGNAL` state it stays a plain
/// label.
pub fn footer(ui: &mut egui::Ui, app: &mut PerfApp) {
    let theme = app.theme.clone();
    let frame = egui::Frame::NONE
        .fill(theme.chrome)
        .inner_margin(Margin::symmetric(STRIP_PADDING_X, STRIP_PADDING_Y_FOOT));

    frame.show(ui, |ui| {
        // Nothing in the row expands (the title bar's right-aligned chips do),
        // so without this the fill stops at the last figure and the window's
        // clear colour shows through on the right.
        ui.set_min_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 15.0;

            match app.status.clone() {
                Status::Running => {
                    // Clickable version label opens the changelog modal.
                    let dot_text = concat!("\u{25cf} v", env!("CARGO_PKG_VERSION"));
                    let resp = ui
                        .add(
                            egui::Label::new(
                                RichText::new(dot_text)
                                    .family(theme.font_data.egui())
                                    .size(10.0)
                                    .color(theme.ok),
                            )
                            .sense(Sense::click()),
                        )
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .on_hover_text("Show changelog");
                    if resp.clicked() {
                        app.show_changelog = true;
                        // The footer opens the full log, not the one-version
                        // "what's new" view the post-update path installs.
                        app.changelog_version = None;
                        app.changelog_show_all = false;
                    }
                }
                Status::Connecting(_) => {
                    ui.label(
                        RichText::new("\u{25cf} BOOTING")
                            .family(theme.font_data.egui())
                            .size(10.0)
                            .color(theme.dim),
                    );
                }
                Status::SensordDown => {
                    ui.label(
                        RichText::new("\u{25cf} NO SIGNAL")
                            .family(theme.font_data.egui())
                            .size(10.0)
                            .color(theme.hot),
                    );
                }
            }

            // Refresh rate.
            foot_item(
                ui,
                &theme,
                &format!("refresh {}", app.config.refresh.label()),
            );

            // Headline snapshot figures.
            if let Some(snap) = &app.latest {
                if let Some(secs) = snap.uptime_sec {
                    // Keyed "UPTIME" — "UP" belongs to the network panel.
                    tooltips::tip(
                        foot_item(ui, &theme, &format!("UP {}", format_uptime(secs))),
                        "UPTIME",
                    );
                }
                if let Some(cpu) = &snap.cpu {
                    foot_item(ui, &theme, &format!("CPU {}%", format_percent(cpu.load)));
                }
                if let Some(gpus) = &snap.gpu {
                    if let Some(gpu) = gpus.first() {
                        foot_item(ui, &theme, &format!("GPU {}%", format_percent(gpu.load)));
                    }
                }
                if let Some(net) = &snap.net {
                    let down = format_bytes_per_sec(finite(net.down_bps).unwrap_or(0.0));
                    let up = format_bytes_per_sec(finite(net.up_bps).unwrap_or(0.0));
                    foot_item(ui, &theme, &format!("NET \u{2193}{down} \u{2191}{up}"));
                }
                // ATK fans (ASUS-only). Each fan reads as one footer chip
                // labelled with its short name and current value. Skipped
                // silently on non-ASUS hardware.
                if let Some(fans) = &snap.atk_fans {
                    for fan in fans {
                        if let Some(rpm) = fan.rpm {
                            let short = fan
                                .name
                                .strip_suffix(" Fan")
                                .unwrap_or(&fan.name)
                                .to_uppercase();
                            foot_item(ui, &theme, &format!("{short} FAN {}", rpm.round() as i64));
                        }
                    }
                }
            }

            // Active displays. Enumerated in this process — the interactive
            // session — so the modes are the real ones; `sensord` runs in
            // session 0 and only sees a virtualised 1024x768 desktop. The
            // cache re-reads on a slow cadence in `PerfApp::ui`.
            for chip in crate::displays::footer_chips(app.display_cache.displays()) {
                foot_item(ui, &theme, &chip.text).on_hover_text(&chip.tooltip);
            }
        });
    });
}

/// One dimmed ~10 px footer figure. Returns the label's response so a caller
/// can attach a tooltip; most footer items ignore it.
fn foot_item(ui: &mut egui::Ui, theme: &Theme, text: &str) -> egui::Response {
    ui.label(
        RichText::new(text)
            .family(theme.font_data.egui())
            .size(10.0)
            .color(theme.dim),
    )
}

/// Render the adaptive card grid into `ui`.
///
/// The card list is built from `app.latest`: a CPU card, one card per GPU in
/// array order, a RAM card, a double-width Storage card, then the always-present
/// Sensors and Network cards. The column count adapts to the available width
/// (4 / 3 / 2). When no snapshot has arrived yet, a single centred dimmed line
/// is shown instead of the grid.
///
/// When the sensor feed has died ([`Status::SensordDown`]) the grid is replaced
/// wholesale by [`error_overlay`] — the stale panels are not drawn behind it.
pub fn card_grid(ui: &mut egui::Ui, app: &mut PerfApp) {
    // A fresh frame of geometry for the test-only recorder.
    recorder::begin();
    // Connecting: show the loading screen so the 5-15 s startup window reads
    // as deliberate progress rather than a "sensor feed stopped" failure.
    // `Status::Connecting(_)` is held by `poll_connect_events`; the guard in
    // `ingest()` prevents it from being flipped to SensordDown frame-by-frame.
    //
    // The Running+no-snapshot case (Ready event already consumed but the
    // first NDJSON snapshot hasn't arrived) reuses the LoadingSensors phase
    // of the same screen — without this fallback the user sees a brief
    // "waiting for sensord..." placeholder between Ready and the first
    // snapshot, which on a slow machine can last several seconds and reads
    // like an outright stall.
    let connecting_phase = match &app.status {
        Status::Connecting(p) => Some(p.clone()),
        Status::Running if app.latest.is_none() => Some(ConnectPhase::LoadingSensors),
        _ => None,
    };
    if let Some(phase) = connecting_phase {
        let theme = app.theme.clone();
        let ctx = ui.ctx().clone();
        // Staged-init progress drives the LoadingSensors checklist. Read
        // fresh from the shared sensor state: the reader thread updates it
        // between snapshots, while `ingest` only runs once per frame.
        let progress = app.sensor_progress();
        match loading_screen::loading_screen(ui, &theme, &phase, progress.as_ref()) {
            loading_screen::LoadingAction::Retry => app.restart_connect(&ctx),
            loading_screen::LoadingAction::Exit => app.want_quit = true,
            loading_screen::LoadingAction::None => {}
        }
        return;
    }

    // Feed down: replace the whole grid with the error overlay. Handled before
    // the immutable reborrow below, since `error_overlay` needs `&mut app` to
    // wire up its respawn button. `card_grid` takes no `Context`, so clone the
    // one egui threads through the `Ui`.
    if app.status == Status::SensordDown {
        let ctx = ui.ctx().clone();
        error_overlay(ui, app, &ctx);
        return;
    }

    // The `None` arm is a defensive safety net: every `Running`/no-snapshot
    // path is already absorbed by the loading-screen fallback above, so in
    // practice this branch is unreachable.
    if app.latest.is_none() {
        waiting_note(ui, &app.theme);
        return;
    }

    // Gather the card set and its geometry before painting: the plan is
    // cached on `app` (a mutable borrow), and painting then holds `app`
    // shared. The metric vectors own their data, so no snapshot borrow
    // survives the block.
    let (cards, metrics, plan, shed) = {
        let snap = app.latest.as_ref().expect("checked above");
        let cards = card_list(snap);
        let metrics: Vec<fit::CardMetrics> = cards.iter().map(|&c| c.metrics(snap)).collect();

        // The zoom decision is measured against the window's *physical* size
        // so that applying a zoom next frame cannot feed back into the
        // decision; see `zoom_one_window`. The chrome (title bar, banners,
        // footer) is part of the zoomed UI, so what remains for the grid is
        // that size minus the chrome height, all in zoom-1.0 points.
        let ctx = ui.ctx();
        let physical = window_points(ctx) * ctx.pixels_per_point();
        let native = ctx.native_pixels_per_point().unwrap_or(1.0);
        let zoom1 = zoom_one_window(ctx);
        // The chrome is measured in the points of the *current* zoom, so it is
        // scaled back up to zoom-1.0 points before it is subtracted from the
        // zoom-1.0 window height (mixing the two units would make the decision
        // depend on the zoom it produces).
        let chrome_h = (window_points(ctx).y - ui.available_height()).max(0.0) * ctx.zoom_factor();
        let key = fit::FitKey::new(
            physical.x,
            physical.y,
            native,
            chrome_h,
            card_set_signature(snap, &cards),
        );
        let plan = app.plan_for(key, &metrics, zoom1.x, (zoom1.y - chrome_h).max(1.0));
        apply_zoom(ctx, plan.zoom);
        // Fit the grid to the panel this frame actually got, not the planned
        // one: `set_zoom_factor` only takes effect next pass. The panel's
        // available height is already the grid's *outer* budget (padding
        // included), which is what `grid_height` reports.
        let panel_h = ui.available_height();
        let shed = fit::shed_for(&metrics, plan.cols, panel_h);
        (cards, metrics, plan, shed)
    };

    // Painting only reads the app.
    let app: &PerfApp = app;
    let snap = app.latest.as_ref().expect("checked above");
    let cols = plan.cols;
    let spans = fit::effective_spans(&metrics, cols);

    let frame = egui::Frame::NONE.inner_margin(Margin::same(GRID_BODY_PADDING));
    frame.show(ui, |ui| {
        // Widths are measured *inside* the grid frame, after its padding.
        let avail_w = ui.available_width();
        let col_width = ((avail_w - GRID_GAP * (cols as f32 - 1.0)) / cols as f32).max(1.0);
        layout_cards(
            ui, app, snap, &cards, &spans, &metrics, cols, col_width, shed,
        );
        crate::ui::recorder::grid(ui.min_rect());
    });
}

/// Build the ordered list of cards to draw from `snap`. Each variant carries
/// only an index; the panel data is read back out of the snapshot at paint
/// time.
fn card_list(snap: &crate::ipc::Snapshot) -> Vec<Card> {
    let mut cards: Vec<Card> = Vec::new();
    if snap.cpu.is_some() {
        cards.push(Card::Cpu);
    }
    if let Some(gpus) = &snap.gpu {
        for i in 0..gpus.len() {
            cards.push(Card::Gpu(i));
        }
    }
    if snap.igpu.is_some() {
        cards.push(Card::Igpu);
    }
    if snap.ram.is_some() {
        cards.push(Card::Ram);
    }
    cards.push(Card::Network);
    if snap.battery.is_some() {
        cards.push(Card::Battery);
    }
    if snap.storage.is_some() {
        cards.push(Card::Storage);
    }
    if panels::sensors::has_content(
        snap.board.as_ref(),
        snap.fans.as_deref().unwrap_or(&[]),
        snap.voltages.as_deref().unwrap_or(&[]),
    ) {
        cards.push(Card::Sensors);
    }
    cards
}

/// FNV-1a signature of the ordered card set plus the one card whose height
/// depends on its contents (storage, one rung per disk). Invalidates the
/// cached fit when a GPU appears, a disk disappears, and so on.
fn card_set_signature(snap: &crate::ipc::Snapshot, cards: &[Card]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    let mut mix = |value: u64| {
        hash ^= value;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    };
    for &card in cards {
        mix(match card {
            Card::Cpu => 1,
            Card::Gpu(i) => 10 + i as u64,
            Card::Igpu => 100,
            Card::Ram => 200,
            Card::Storage => 300,
            Card::Sensors => 400,
            Card::Network => 500,
            Card::Battery => 600,
        });
    }
    mix(snap.storage.as_ref().map_or(0, |s| s.len()) as u64);
    hash
}

/// Outer height of a row-1 card (CPU, GPU, iGPU, RAM, Network) at full
/// content: an eleven-candidate GPU block plus its legend, load donut and a
/// comfortable sparkline. Every other card in the row pads up to it.
const ROW_1_FULL_HEIGHT: f32 = 302.0;
/// Row-1 height with the stat block capped at eight candidates.
const ROW_1_MID_HEIGHT: f32 = 238.0;
/// Row-1 floor: title, load donut, legend, padding and a minimum-height
/// sparkline. Dropping further stat rows cannot shrink the card past the
/// donut.
const ROW_1_FLOOR_HEIGHT: f32 = 212.0;
/// Outer height of the Battery card (title + charge donut + stat rows).
const BATTERY_HEIGHT: f32 = 138.0;
/// Sensors card heights, paired with [`SENSORS_ROWS`], fullest first.
const SENSORS_HEIGHTS: [f32; 5] = [302.0, 238.0, 212.0, 140.0, 108.0];
/// Stat-row budgets matching [`SENSORS_HEIGHTS`].
const SENSORS_ROWS: [usize; 5] = [11, 8, 6, 4, 2];
/// Baseline for the Storage card: title row, column header and padding.
const STORAGE_BASE_HEIGHT: f32 = 72.0;
/// Vertical step per disk row inside the Storage card.
const STORAGE_DISK_ROW_HEIGHT: f32 = 46.0;

impl Card {
    /// Geometry the zoom-to-fit planner needs for this card: the ladder of
    /// content heights, fullest first. Each rung's height is the outer card
    /// height measured for the stat budget on the same rung, so the planner
    /// picks a height at which the panel provably fits.
    fn metrics(self, snap: &crate::ipc::Snapshot) -> fit::CardMetrics {
        let row1 = |height: f32, rows: usize| fit::CardTier { height, rows };
        let tiers = match self {
            Card::Cpu | Card::Gpu(_) | Card::Igpu | Card::Ram | Card::Network => vec![
                row1(ROW_1_FULL_HEIGHT, 11),
                row1(ROW_1_MID_HEIGHT, 8),
                row1(ROW_1_FLOOR_HEIGHT, 6),
            ],
            Card::Battery => vec![row1(BATTERY_HEIGHT, 2)],
            Card::Sensors => SENSORS_HEIGHTS
                .iter()
                .zip(SENSORS_ROWS)
                .map(|(&height, rows)| row1(height, rows))
                .collect(),
            Card::Storage => {
                // One rung per disk count, fullest first; the last rung keeps
                // a single disk so the card still reports something.
                let disks = snap.storage.as_ref().map(|s| s.len()).unwrap_or(0).max(1);
                (1..=disks)
                    .rev()
                    .map(|n| row1(STORAGE_BASE_HEIGHT + STORAGE_DISK_ROW_HEIGHT * n as f32, n))
                    .collect()
            }
        };
        fit::CardMetrics {
            span: self.base_span(),
            elastic: matches!(self, Card::Storage),
            reserves: matches!(self, Card::Network | Card::Battery | Card::Sensors),
            tiers,
        }
    }
}

/// One slot in the ordered card list. Indices refer into the snapshot's `gpu`
/// array; data-bearing cards re-read their section at paint time.
#[derive(Clone, Copy)]
enum Card {
    Cpu,
    Gpu(usize),
    /// Intel / integrated GPU. Rendered with `gpu_panel` (which already
    /// special-cases `kind == "integrated"`) but sourced from
    /// `snap.igpu` rather than `snap.gpu`.
    Igpu,
    Ram,
    Storage,
    Sensors,
    Network,
    Battery,
}

impl Card {
    /// Base span for this card before context-sensitive adjustments
    /// (see `layout_spans`).
    fn base_span(self) -> usize {
        match self {
            Card::Storage => 2,
            _ => 1,
        }
    }
}

/// The window's content size in *points at the current zoom*, falling back to
/// the input screen rect for backends (such as the test harness) that do not
/// report a viewport rectangle.
fn window_points(ctx: &egui::Context) -> Vec2 {
    ctx.input(|i| {
        i.viewport()
            .inner_rect
            .map(|r| r.size())
            .unwrap_or_else(|| i.content_rect().size())
    })
}

/// The window's content size in zoom-1.0 points.
///
/// Derived from the *physical* pixel size (current points times the current
/// pixels-per-point), so it is invariant under the zoom the renderer applies:
/// a decision taken from a size that the zoom itself had already divided would
/// drift frame after frame.
fn zoom_one_window(ctx: &egui::Context) -> Vec2 {
    let pp = ctx.pixels_per_point().max(f32::MIN_POSITIVE);
    let native = ctx
        .native_pixels_per_point()
        .unwrap_or(1.0)
        .max(f32::MIN_POSITIVE);
    window_points(ctx) * pp / native
}

/// Push the fitted zoom onto the context, but only when it differs from the
/// current one by more than [`fit::ZOOM_EPSILON`]. Keeping the current zoom for
/// sub-percent changes stops the scale factor from creeping every frame.
fn apply_zoom(ctx: &egui::Context, zoom: f32) {
    let current = ctx.zoom_factor();
    if (zoom / current - 1.0).abs() > fit::ZOOM_EPSILON {
        ctx.set_zoom_factor(zoom);
    }
}

/// Place `cards` left-to-right into `cols`-wide rows, wrapping as columns fill.
///
/// Each row is one `ui.horizontal`; a single-width card is allocated
/// `col_width`, the double-width Storage card `2*col_width + GRID_GAP`. A card
/// that would not fit in the columns left on the current row starts a new row.
///
/// Row heights come from [`fit::row_heights`] at the chosen shed rung. Any
/// height left over after the last row is shared equally, so every row grows
/// together and the window has no empty band.
#[allow(clippy::too_many_arguments)]
fn layout_cards(
    ui: &mut egui::Ui,
    app: &PerfApp,
    snap: &crate::ipc::Snapshot,
    cards: &[Card],
    spans: &[usize],
    metrics: &[fit::CardMetrics],
    cols: usize,
    col_width: f32,
    shed: fit::Shed,
) {
    ui.spacing_mut().item_spacing = Vec2::splat(GRID_GAP);

    let mut row_heights = fit::row_heights(metrics, cols, shed);
    let content_avail_h = ui.available_height();
    if !row_heights.is_empty() {
        let gaps = GRID_GAP * (row_heights.len() as f32 - 1.0);
        let natural = row_heights.iter().sum::<f32>() + gaps;
        let share = ((content_avail_h - natural) / row_heights.len() as f32).max(0.0);
        for h in &mut row_heights {
            *h += share;
        }
    }

    let mut row = 0usize;
    let mut idx = 0;
    while idx < cards.len() {
        // Greedily fill one row, respecting card spans and the column budget.
        let mut row_indices: Vec<usize> = Vec::new();
        let mut used = 0;
        while idx < cards.len() {
            let span = spans[idx];
            if used + span > cols {
                break;
            }
            used += span;
            row_indices.push(idx);
            idx += 1;
        }

        // Row height comes from the shed plan, pre-computed for every row
        // before any card is painted, so the layout is deterministic and has
        // no frame-to-frame feedback loop.
        let row_h = row_heights.get(row).copied().unwrap_or(ROW_1_FULL_HEIGHT);
        row += 1;

        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = GRID_GAP;
            for &i in &row_indices {
                let span = spans[i];
                let card_w = col_width * span as f32 + GRID_GAP * (span as f32 - 1.0);
                let capacity = crate::ui::capacity::Capacity::from_card_size(card_w, row_h);
                // Allocate sub-UI at the full row height so widgets inside
                // (sparkline) see the true available_height and can grow to
                // fill the card without leaving a chin at the bottom.
                ui.allocate_ui_with_layout(
                    Vec2::new(card_w, row_h),
                    Layout::top_down(Align::Min),
                    |ui| {
                        ui.set_width(card_w);
                        ui.set_height(row_h);
                        paint_card(ui, app, snap, cards[i], row_h, capacity);
                    },
                );
            }
        });
    }
}

/// Paint a single card by dispatching to its `panels::*` function.
///
/// The panel functions wrap themselves in `panels::card`, so this calls them
/// directly — wrapping again would double the frame.
fn paint_card(
    ui: &mut egui::Ui,
    app: &PerfApp,
    snap: &crate::ipc::Snapshot,
    card: Card,
    min_h: f32,
    capacity: crate::ui::capacity::Capacity,
) {
    let theme = &app.theme;
    let unit = app.config.unit;
    match card {
        Card::Cpu => {
            if let Some(cpu) = &snap.cpu {
                panels::cpu::cpu_panel(
                    ui,
                    theme,
                    cpu,
                    app.history.cpu.as_ref(),
                    unit,
                    app.config.cpu_heat_map,
                    capacity,
                    min_h,
                );
            }
        }
        Card::Gpu(i) => {
            if let Some(gpu) = snap.gpu.as_ref().and_then(|g| g.get(i)) {
                panels::gpu::gpu_panel(ui, theme, gpu, app.history.gpu(i), unit, capacity, min_h);
            }
        }
        Card::Igpu => {
            if let Some(igpu) = &snap.igpu {
                // No history tracked for the iGPU yet — the panel renders
                // without the trailing sparkline when `None`.
                panels::gpu::gpu_panel(ui, theme, igpu, None, unit, capacity, min_h);
            }
        }
        Card::Ram => {
            if let Some(ram) = &snap.ram {
                panels::ram::ram_panel(
                    ui,
                    theme,
                    ram,
                    app.history.ram.as_ref(),
                    unit,
                    capacity,
                    min_h,
                );
            }
        }
        Card::Storage => {
            if let Some(disks) = &snap.storage {
                panels::storage::storage_panel(ui, theme, disks, unit, capacity, min_h);
            }
        }
        Card::Sensors => {
            panels::sensors::sensors_panel(
                ui,
                theme,
                snap.board.as_ref(),
                snap.fans.as_deref().unwrap_or(&[]),
                snap.voltages.as_deref().unwrap_or(&[]),
                unit,
                capacity,
                min_h,
            );
        }
        Card::Network => {
            panels::network::network_panel(
                ui,
                theme,
                snap.net.as_ref(),
                app.history.network.as_ref(),
                capacity,
                min_h,
            );
        }
        Card::Battery => {
            if let Some(batt) = &snap.battery {
                panels::battery::battery_panel(ui, theme, batt, capacity, min_h);
            }
        }
    }
}

/// Draw the no-snapshot placeholder: one centred, dimmed line.
fn waiting_note(ui: &mut egui::Ui, theme: &Theme) {
    ui.vertical_centered(|ui| {
        ui.add_space(40.0);
        ui.label(
            RichText::new("waiting for sensord\u{2026}")
                .family(theme.font_data.egui())
                .size(12.0)
                .color(theme.dim),
        );
    });
}

/// Width of the `sensord`-down error card (snug around its text + button).
const ERROR_CARD_WIDTH: f32 = 320.0;
/// Space dropped above the error card so it sits a little below centre.
const ERROR_CARD_TOP_GAP: f32 = 48.0;
/// `RESPAWN` button padding — chunkier than the title-bar `.pw-chip`.
const RESPAWN_PAD: Vec2 = Vec2::new(16.0, 9.0);

/// Draw the sensor-feed-down state: a centred card explaining that `sensord`
/// has exited, with a one-click `RESPAWN` control.
///
/// Shown by [`card_grid`] in place of the whole grid whenever
/// [`Status::SensordDown`] is set — the stale panels are not painted behind it.
/// The card carries a `hot` heading, a `dim` explanatory line and an
/// `accent`-bordered button; clicking the button calls
/// [`PerfApp::respawn_sensord`], which restarts the child and (on success)
/// flips `status` back to [`Status::Running`] so the next frame draws the grid.
pub fn error_overlay(ui: &mut egui::Ui, app: &mut PerfApp, ctx: &egui::Context) {
    let theme = app.theme.clone();
    let mut respawn = false;

    // Centre the fixed-width card horizontally (`waiting_note`'s approach),
    // nudged below the top edge so it reads as the body's focal point.
    ui.vertical_centered(|ui| {
        ui.add_space(ERROR_CARD_TOP_GAP);
        ui.allocate_ui_with_layout(
            Vec2::new(ERROR_CARD_WIDTH, 0.0),
            Layout::top_down(Align::Center),
            |ui| {
                ui.set_width(ERROR_CARD_WIDTH);
                panels::card(ui, &theme, "ERROR", 0.0, |ui| {
                    // `hot` heading — the alarm line, in the display font.
                    ui.label(
                        RichText::new(letter_spaced("SENSOR FEED STOPPED"))
                            .family(theme.font_display.egui())
                            .size(13.0)
                            .color(theme.hot),
                    );
                    // `dim` explanatory line, wrapped to the card width.
                    ui.label(
                        RichText::new("The sensor process exited. Hardware readings are paused.")
                            .family(theme.font_data.egui())
                            .size(11.0)
                            .color(theme.dim),
                    );
                    if respawn_button(ui, &theme).clicked() {
                        respawn = true;
                    }
                });
            },
        );
    });

    // Applied after the layout closures so `app` is borrowed mutably just once.
    if respawn {
        if app.dev_mode {
            app.respawn_sensord(ctx);
        } else {
            app.restart_connect(ctx);
        }
    }
}

/// Draw the `RESPAWN` button: a bordered clickable box, like the title-bar
/// [`chip`] but larger and `accent`-bordered, filling `accent` on hover.
/// Returns its click-sensing response.
fn respawn_button(ui: &mut egui::Ui, theme: &Theme) -> egui::Response {
    let font = FontId::new(11.0, theme.font_data.egui());
    // Laid out with `PLACEHOLDER` so `painter.galley`'s fallback colour tints
    // the glyphs to the hover-dependent colour chosen below.
    let galley =
        ui.painter()
            .layout_no_wrap(letter_spaced("RESPAWN"), font, egui::Color32::PLACEHOLDER);

    let size = galley.size() + RESPAWN_PAD * 2.0;
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());

    if ui.is_rect_visible(rect) {
        let painter = ui.painter_at(rect);
        // Idle: `accent`-outlined with `accent` text. Hover: `accent` fill with
        // `bg` text, echoing the title-bar chip's active look (and `close_button`).
        let hovered = response.hovered();
        let text_color = if hovered { theme.bg } else { theme.accent };
        if hovered {
            painter.rect_filled(rect, 0.0, theme.accent);
        }
        painter.rect_stroke(
            rect,
            0.0,
            Stroke::new(1.0_f32, theme.accent),
            egui::StrokeKind::Inside,
        );
        // The galley's glyphs are `PLACEHOLDER`, so this `text_color` is the
        // fallback that actually colours them — `accent` idle, `bg` on hover.
        painter.galley(rect.min + RESPAWN_PAD, galley, text_color);
    }
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
}

/// Advance a theme through the fixed cycle.
fn next_theme(id: ThemeId) -> ThemeId {
    match id {
        ThemeId::Amber => ThemeId::Slate,
        ThemeId::Slate => ThemeId::Phosphor,
        ThemeId::Phosphor => ThemeId::Light,
        ThemeId::Light => ThemeId::Synthwave,
        ThemeId::Synthwave => ThemeId::Crimson,
        ThemeId::Crimson => ThemeId::Amber,
    }
}
