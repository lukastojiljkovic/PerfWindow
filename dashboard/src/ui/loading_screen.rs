//! Full-viewport overlay shown while the connect state machine is running
//! or after it has reached a failure state. Rendered by `card_grid` in
//! place of the cards whenever `Status::Connecting(_)` is active, plus the
//! Running-with-no-snapshot gap (mapped onto `LoadingSensors`). Returns the
//! user's button choice as a `LoadingAction` so the caller can drive Retry /
//! Exit semantics from a single place.

use crate::format::letter_spaced;
use crate::ipc::connect::{ConnectPhase, FailedReason};
use crate::ipc::ProgressInfo;
use crate::theme::Theme;
use egui::{Align, FontId, Layout, RichText, Sense, Stroke, Vec2};

const WORDMARK_SIZE: f32 = 24.0;
const PHRASE_SIZE: f32 = 13.0;
const TOP_GAP: f32 = 96.0;
const PHRASE_GAP: f32 = 18.0;
const BUTTON_GAP: f32 = 24.0;
const BUTTON_PAD: Vec2 = Vec2::new(20.0, 9.0);
const SPINNER_GLYPHS: [&str; 4] = ["\u{25d0}", "\u{25d3}", "\u{25d1}", "\u{25d2}"];
const SPINNER_PERIOD_S: f64 = 1.0;
/// Width of the staged-init checklist block. Fixed so the left-aligned rows
/// sit as one centred column instead of each row centring on its own width.
const CHECKLIST_WIDTH: f32 = 200.0;
/// Width reserved for a row's status glyph, so the category names line up in
/// a column regardless of the glyph (✓ / spinner / blank).
const CHECKLIST_GLYPH_W: f32 = 14.0;
const CHECKLIST_ROW_FONT: f32 = 11.0;
/// Below this card width the Retry / Exit buttons stack vertically instead
/// of going side-by-side — fits roughly one full button (~96 px) plus padding
/// on either side. Same threshold the egui examples use for narrow-window
/// layouts.
const HORIZONTAL_BUTTON_BREAKPOINT: f32 = 240.0;

/// User action taken on this frame. `None` is the normal case (no click).
#[derive(Debug, Clone, Copy)]
pub enum LoadingAction {
    None,
    Retry,
    Exit,
}

/// Paint the loading screen for the given `phase`. Returns the button
/// choice. Caller wires Retry to a fresh `spawn_connect` and Exit to
/// `want_quit = true`.
///
/// `progress` is the latest staged-init progress line from sensord; it only
/// matters in the `LoadingSensors` phase, where it replaces the static
/// spinner phrase with a per-category checklist. `None` (old sensord, or no
/// progress line yet) keeps the spinner fallback.
pub fn loading_screen(
    ui: &mut egui::Ui,
    theme: &Theme,
    phase: &ConnectPhase,
    progress: Option<&ProgressInfo>,
) -> LoadingAction {
    let mut action = LoadingAction::None;
    ui.vertical_centered(|ui| {
        ui.add_space(TOP_GAP);
        ui.label(
            RichText::new(letter_spaced("\u{259a} PERFWINDOW"))
                .family(theme.font_display.egui())
                .size(WORDMARK_SIZE)
                .color(theme.accent),
        );
        ui.add_space(PHRASE_GAP);

        match phase {
            ConnectPhase::OpeningPipe => {
                phrase_with_spinner(ui, theme, "Connecting to sensor service...")
            }
            ConnectPhase::RequestingElevation => phrase(
                ui,
                theme,
                "Windows will ask for permission to start the sensor service.",
            ),
            ConnectPhase::StartingService => {
                phrase_with_spinner(ui, theme, "Starting sensor service...")
            }
            ConnectPhase::LoadingSensors => match progress.filter(|p| has_rows(p)) {
                Some(p) => progress_checklist(ui, theme, p),
                None => phrase_with_spinner(ui, theme, "Loading sensors..."),
            },
            ConnectPhase::Failed(reason) => {
                action = failed(ui, theme, reason);
            }
        }
    });
    action
}

/// A progress line with every list empty (adversarial / degenerate input)
/// would paint a blank screen; treat it like absent progress instead.
fn has_rows(progress: &ProgressInfo) -> bool {
    progress.loading.is_some() || !progress.done.is_empty() || !progress.pending.is_empty()
}

/// Render the staged-init checklist: one row per category — done in accent
/// with a check mark, the in-flight one with the spinner glyph, pending dim.
///
/// Rows come straight off the wire in `done` → `loading` → `pending` order:
/// for a conforming sensord that concatenation IS the canonical category
/// order (cpu, ram, motherboard, gpu, storage, network, controller, battery),
/// and an unknown category from a future sensord renders as-is in whatever
/// slot the service put it — never dropped, never a panic.
fn progress_checklist(ui: &mut egui::Ui, theme: &Theme, progress: &ProgressInfo) {
    ui.allocate_ui_with_layout(
        Vec2::new(CHECKLIST_WIDTH, 0.0),
        Layout::top_down(Align::Min),
        |ui| {
            ui.spacing_mut().item_spacing.y = 5.0;
            for name in &progress.done {
                checklist_row(ui, theme, "\u{2713}", theme.accent, name, theme.accent);
            }
            if let Some(name) = &progress.loading {
                let t = ui.input(|i| i.time);
                let idx = ((t / SPINNER_PERIOD_S * SPINNER_GLYPHS.len() as f64) as usize)
                    % SPINNER_GLYPHS.len();
                checklist_row(
                    ui,
                    theme,
                    SPINNER_GLYPHS[idx],
                    theme.accent,
                    name,
                    theme.ink,
                );
            }
            for name in &progress.pending {
                checklist_row(ui, theme, "", theme.dim, name, theme.dim);
            }
        },
    );
    // The spinner glyph only animates while frames keep coming; idle connect
    // threads produce none, so schedule the next one ourselves.
    if progress.loading.is_some() {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(120));
    }
}

/// One checklist row: a fixed-width glyph slot followed by the upper-cased,
/// letter-spaced category name. The fixed slot keeps the name column aligned
/// across the ✓ / spinner / blank states.
fn checklist_row(
    ui: &mut egui::Ui,
    theme: &Theme,
    glyph: &str,
    glyph_color: egui::Color32,
    name: &str,
    name_color: egui::Color32,
) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let (rect, _) = ui.allocate_exact_size(
            Vec2::new(CHECKLIST_GLYPH_W, CHECKLIST_ROW_FONT + 4.0),
            Sense::hover(),
        );
        if !glyph.is_empty() && ui.is_rect_visible(rect) {
            let painter = ui.painter_at(rect);
            let font = FontId::new(CHECKLIST_ROW_FONT, theme.font_data.egui());
            let galley = painter.layout_no_wrap(glyph.to_owned(), font, glyph_color);
            let pos = egui::Pos2::new(
                rect.center().x - galley.size().x / 2.0,
                rect.center().y - galley.size().y / 2.0,
            );
            painter.galley(pos, galley, glyph_color);
        }
        ui.label(
            RichText::new(letter_spaced(&name.to_uppercase()))
                .family(theme.font_data.egui())
                .size(CHECKLIST_ROW_FONT)
                .color(name_color),
        );
    });
}

fn phrase(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    ui.label(
        RichText::new(text)
            .family(theme.font_data.egui())
            .size(PHRASE_SIZE)
            .color(theme.dim),
    );
}

fn phrase_with_spinner(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    phrase(ui, theme, text);
    ui.add_space(8.0);
    spinner(ui, theme);
    // egui's time advances only on repaints; ask for the next frame so the
    // spinner animates while the connect thread is otherwise idle.
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(120));
}

fn spinner(ui: &mut egui::Ui, theme: &Theme) {
    let t = ui.input(|i| i.time);
    let idx =
        ((t / SPINNER_PERIOD_S * SPINNER_GLYPHS.len() as f64) as usize) % SPINNER_GLYPHS.len();
    ui.label(
        RichText::new(SPINNER_GLYPHS[idx])
            .family(theme.font_display.egui())
            .size(20.0)
            .color(theme.accent),
    );
}

fn failed(ui: &mut egui::Ui, theme: &Theme, reason: &FailedReason) -> LoadingAction {
    let (line1, line2) = match reason {
        FailedReason::UacCancelled => (
            "SERVICE START WAS CANCELLED",
            "PerfWindow needs to start the sensor service to read hardware data.".to_string(),
        ),
        FailedReason::StartTimeout => (
            "SERVICE DID NOT START IN TIME",
            "The sensor service was launched but did not become ready within 15 seconds."
                .to_string(),
        ),
        FailedReason::PipeError(e) => ("SENSOR SERVICE COULD NOT CONNECT", e.clone()),
    };
    ui.label(
        RichText::new(letter_spaced(line1))
            .family(theme.font_display.egui())
            .size(14.0)
            .color(theme.hot),
    );
    ui.add_space(6.0);
    ui.label(
        RichText::new(line2)
            .family(theme.font_data.egui())
            .size(11.0)
            .color(theme.dim),
    );
    ui.add_space(BUTTON_GAP);
    let mut chosen = LoadingAction::None;
    // Side-by-side at the usual width; stack vertically when the window is
    // too narrow to seat both buttons on one row without clipping.
    let available_w = ui.available_width();
    let stack_vertical = !available_w.is_finite() || available_w < HORIZONTAL_BUTTON_BREAKPOINT;
    if stack_vertical {
        ui.vertical_centered(|ui| {
            if action_button(ui, theme, "RETRY").clicked() {
                chosen = LoadingAction::Retry;
            }
            ui.add_space(8.0);
            if action_button(ui, theme, "EXIT").clicked() {
                chosen = LoadingAction::Exit;
            }
        });
    } else {
        ui.horizontal(|ui| {
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                let offset = ((available_w / 2.0) - 100.0).max(0.0);
                ui.add_space(offset);
                if action_button(ui, theme, "RETRY").clicked() {
                    chosen = LoadingAction::Retry;
                }
                ui.add_space(12.0);
                if action_button(ui, theme, "EXIT").clicked() {
                    chosen = LoadingAction::Exit;
                }
            });
        });
    }
    chosen
}

fn action_button(ui: &mut egui::Ui, theme: &Theme, label: &str) -> egui::Response {
    let font = FontId::new(11.0, theme.font_data.egui());
    let galley =
        ui.painter()
            .layout_no_wrap(letter_spaced(label), font, egui::Color32::PLACEHOLDER);
    let size = galley.size() + BUTTON_PAD * 2.0;
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    if ui.is_rect_visible(rect) {
        let painter = ui.painter_at(rect);
        let hovered = response.hovered();
        let text_color = if hovered { theme.bg } else { theme.accent };
        if hovered {
            painter.rect_filled(rect, 0.0, theme.accent);
        }
        painter.rect_stroke(
            rect,
            0.0,
            Stroke::new(1.0, theme.accent),
            egui::StrokeKind::Inside,
        );
        painter.galley(rect.min + BUTTON_PAD, galley, text_color);
    }
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
}
