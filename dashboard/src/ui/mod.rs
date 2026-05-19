//! The dashboard chrome: the title bar, the adaptive card grid and the footer.
//!
//! These are the three regions the render method assembles every frame —
//! [`title_bar`] in a top panel, [`footer`] in a bottom panel and [`card_grid`]
//! in the central scroll area. The grid simply orders the [`crate::panels`]
//! cards from the latest snapshot and places them; each panel paints its own
//! [`crate::panels::card`] frame, so the grid never wraps a cell itself.

use crate::app::{PerfApp, Status};
use crate::config::ThemeId;
use crate::format::{finite, format_bytes_per_sec, format_percent, letter_spaced, TempUnit};
use crate::panels;
use crate::theme::Theme;
use egui::{Align, FontId, Layout, Margin, RichText, Sense, Stroke, Vec2};

/// Title-bar / footer strip height (mockup `.pw-tb` / `.pw-foot` padding).
const STRIP_PADDING_X: i8 = 13;
const STRIP_PADDING_Y_TB: i8 = 9;
const STRIP_PADDING_Y_FOOT: i8 = 7;
/// Gap between cards in the grid (mockup `.pw-grid { gap: 10px }`).
const GRID_GAP: f32 = 10.0;
/// Inner padding around the grid (mockup `.pw-body { padding: 13px }`).
const GRID_BODY_PADDING: i8 = 13;
/// Period, in seconds, of the wordmark's blinking block cursor
/// (mockup `.cur { animation: blink 1.1s }`).
const CURSOR_BLINK_PERIOD: f64 = 1.1;
/// Column-count breakpoints, in pixels of available grid width.
const BREAKPOINT_4_COLS: f32 = 1100.0;
const BREAKPOINT_3_COLS: f32 = 820.0;

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
    let time = ui.input(|i| i.time);
    // On for the first half of each period, off for the second.
    let phase = time % CURSOR_BLINK_PERIOD;
    let cursor_visible = phase < CURSOR_BLINK_PERIOD / 2.0;
    // egui's `time` only advances when a frame is drawn, and the refresh-rate
    // watchdog repaints far too slowly for a 1.1 s blink. Schedule a repaint
    // exactly at the next on/off transition so the cursor keeps proper time.
    let to_next_edge = if cursor_visible {
        CURSOR_BLINK_PERIOD / 2.0 - phase
    } else {
        CURSOR_BLINK_PERIOD - phase
    };
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_secs_f64(to_next_edge));

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
/// response. Matches `.pw-chip` / `.pw-chip.on` in the mockup.
fn chip(ui: &mut egui::Ui, theme: &Theme, label: &str, on: bool) -> egui::Response {
    let font = FontId::new(10.0, theme.font_data.egui());
    let text_color = if on { theme.bg } else { theme.dim };
    let galley = ui
        .painter()
        .layout_no_wrap(letter_spaced(label), font, text_color);

    // Padding mirrors the mockup's `.pw-chip { padding: 5px 8px }`.
    let pad = Vec2::new(8.0, 5.0);
    let size = galley.size() + pad * 2.0;
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());

    if ui.is_rect_visible(rect) {
        let painter = ui.painter_at(rect);
        // Active chip: `accent` fill with an `accent` outline. Inactive chip:
        // no fill, a 1 px `border` outline (mockup `.pw-chip` / `.pw-chip.on`).
        let stroke_color = if on { theme.accent } else { theme.border };
        if on {
            painter.rect_filled(rect, 0.0, theme.accent);
        }
        painter.rect_stroke(
            rect,
            0.0,
            Stroke::new(1.0, stroke_color),
            egui::StrokeKind::Inside,
        );
        let text_pos = rect.min + pad;
        painter.galley(text_pos, galley, text_color);
    }
    response
}

/// Draw the footer: a `chrome`-filled strip opened by a 1 px `border` top rule.
///
/// Left to right: a live/no-signal indicator, the configured refresh rate and
/// the latest snapshot's headline CPU / GPU / network figures. Matches
/// `.pw-foot` in the mockup.
pub fn footer(ui: &mut egui::Ui, app: &PerfApp) {
    let theme = &app.theme;
    let frame = egui::Frame::NONE
        .fill(theme.chrome)
        .inner_margin(Margin::symmetric(STRIP_PADDING_X, STRIP_PADDING_Y_FOOT));

    frame.show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 15.0;

            // Live / no-signal status dot.
            let running = app.status == Status::Running;
            let (dot_text, dot_color) = if running {
                ("\u{25cf} LIVE", theme.ok)
            } else {
                ("\u{25cf} NO SIGNAL", theme.hot)
            };
            ui.label(
                RichText::new(dot_text)
                    .family(theme.font_data.egui())
                    .size(10.0)
                    .color(dot_color),
            );

            // Refresh rate.
            foot_item(
                ui,
                theme,
                &format!("refresh {}", app.config.refresh.label()),
            );

            // Headline snapshot figures.
            if let Some(snap) = &app.latest {
                if let Some(cpu) = &snap.cpu {
                    foot_item(ui, theme, &format!("CPU {}%", format_percent(cpu.load)));
                }
                if let Some(gpus) = &snap.gpu {
                    if let Some(gpu) = gpus.first() {
                        foot_item(ui, theme, &format!("GPU {}%", format_percent(gpu.load)));
                    }
                }
                if let Some(net) = &snap.net {
                    let down = format_bytes_per_sec(finite(net.down_bps).unwrap_or(0.0));
                    let up = format_bytes_per_sec(finite(net.up_bps).unwrap_or(0.0));
                    foot_item(ui, theme, &format!("NET \u{2193}{down} \u{2191}{up}"));
                }
            }
        });
    });
}

/// One dimmed ~10 px footer figure.
fn foot_item(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    ui.label(
        RichText::new(text)
            .family(theme.font_data.egui())
            .size(10.0)
            .color(theme.dim),
    );
}

/// Render the adaptive card grid into `ui`.
///
/// The card list is built from `app.latest`: a CPU card, one card per GPU in
/// array order, a RAM card, a double-width Storage card, then the always-present
/// Sensors and Network cards. The column count adapts to the available width
/// (4 / 3 / 2). When no snapshot has arrived yet, a single centred dimmed line
/// is shown instead of the grid.
pub fn card_grid(ui: &mut egui::Ui, app: &mut PerfApp) {
    // The grid is read-only; reborrow `app` immutably so the snapshot can be
    // borrowed (rather than cloned) for the lifetime of the layout.
    let app: &PerfApp = app;
    let frame = egui::Frame::NONE.inner_margin(Margin::same(GRID_BODY_PADDING));
    frame.show(ui, |ui| {
        let Some(snap) = &app.latest else {
            waiting_note(ui, &app.theme);
            return;
        };

        // Build the ordered list of cards to draw. Each variant carries only an
        // index; the panel data is read back out of `snap` / `app` at paint time.
        let mut cards: Vec<Card> = Vec::new();
        if snap.cpu.is_some() {
            cards.push(Card::Cpu);
        }
        if let Some(gpus) = &snap.gpu {
            for i in 0..gpus.len() {
                cards.push(Card::Gpu(i));
            }
        }
        if snap.ram.is_some() {
            cards.push(Card::Ram);
        }
        if snap.storage.is_some() {
            cards.push(Card::Storage);
        }
        cards.push(Card::Sensors);
        cards.push(Card::Network);

        let avail = ui.available_width();
        let cols = column_count(avail);
        let col_width = ((avail - GRID_GAP * (cols as f32 - 1.0)) / cols as f32).max(1.0);

        layout_cards(ui, app, snap, &cards, cols, col_width);
    });
}

/// One slot in the ordered card list. Indices refer into the snapshot's `gpu`
/// array; data-bearing cards re-read their section at paint time.
#[derive(Clone, Copy)]
enum Card {
    Cpu,
    Gpu(usize),
    Ram,
    Storage,
    Sensors,
    Network,
}

impl Card {
    /// How many grid columns this card spans. Only Storage is double-width.
    fn span(self) -> usize {
        match self {
            Card::Storage => 2,
            _ => 1,
        }
    }
}

/// Choose a column count from the available grid width.
fn column_count(avail: f32) -> usize {
    if avail >= BREAKPOINT_4_COLS {
        4
    } else if avail >= BREAKPOINT_3_COLS {
        3
    } else {
        2
    }
}

/// Place `cards` left-to-right into `cols`-wide rows, wrapping as columns fill.
///
/// Each row is one `ui.horizontal`; a single-width card is allocated
/// `col_width`, the double-width Storage card `2*col_width + GRID_GAP`. A card
/// that would not fit in the columns left on the current row starts a new row.
fn layout_cards(
    ui: &mut egui::Ui,
    app: &PerfApp,
    snap: &crate::ipc::Snapshot,
    cards: &[Card],
    cols: usize,
    col_width: f32,
) {
    ui.spacing_mut().item_spacing = Vec2::splat(GRID_GAP);

    let mut idx = 0;
    while idx < cards.len() {
        // Greedily fill one row, respecting card spans and the column budget.
        let mut row: Vec<Card> = Vec::new();
        let mut used = 0;
        while idx < cards.len() {
            let span = cards[idx].span().min(cols);
            if used + span > cols {
                break;
            }
            used += span;
            row.push(cards[idx]);
            idx += 1;
        }

        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = GRID_GAP;
            for card in &row {
                let span = card.span().min(cols);
                let width = col_width * span as f32 + GRID_GAP * (span as f32 - 1.0);
                ui.allocate_ui_with_layout(
                    Vec2::new(width, 0.0),
                    Layout::top_down(Align::Min),
                    |ui| {
                        ui.set_width(width);
                        paint_card(ui, app, snap, *card);
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
fn paint_card(ui: &mut egui::Ui, app: &PerfApp, snap: &crate::ipc::Snapshot, card: Card) {
    let theme = &app.theme;
    let unit = app.config.unit;
    match card {
        Card::Cpu => {
            if let Some(cpu) = &snap.cpu {
                panels::cpu::cpu_panel(ui, theme, cpu, app.history.cpu.as_ref(), unit);
            }
        }
        Card::Gpu(i) => {
            if let Some(gpu) = snap.gpu.as_ref().and_then(|g| g.get(i)) {
                panels::gpu::gpu_panel(ui, theme, gpu, app.history.gpus.get(i), unit);
            }
        }
        Card::Ram => {
            if let Some(ram) = &snap.ram {
                panels::ram::ram_panel(ui, theme, ram, app.history.ram.as_ref());
            }
        }
        Card::Storage => {
            if let Some(disks) = &snap.storage {
                panels::storage::storage_panel(ui, theme, disks, unit);
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
            );
        }
        Card::Network => {
            panels::network::network_panel(ui, theme, snap.net.as_ref());
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

/// Advance a theme through the fixed four-theme cycle.
fn next_theme(id: ThemeId) -> ThemeId {
    match id {
        ThemeId::Amber => ThemeId::Slate,
        ThemeId::Slate => ThemeId::Phosphor,
        ThemeId::Phosphor => ThemeId::Light,
        ThemeId::Light => ThemeId::Amber,
    }
}
