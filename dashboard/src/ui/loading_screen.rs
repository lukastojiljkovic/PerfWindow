//! Full-viewport overlay shown while the connect state machine is running
//! or after it has reached a failure state. Rendered by `card_grid` in
//! place of the cards whenever `Status::Connecting(_)` is active, plus the
//! Running-with-no-snapshot gap (mapped onto `LoadingSensors`). Returns the
//! user's button choice as a `LoadingAction` so the caller can drive Retry /
//! Exit semantics from a single place.

use crate::format::letter_spaced;
use crate::ipc::connect::{ConnectPhase, FailedReason};
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
pub fn loading_screen(ui: &mut egui::Ui, theme: &Theme, phase: &ConnectPhase) -> LoadingAction {
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
            ConnectPhase::LoadingSensors => phrase_with_spinner(ui, theme, "Loading sensors..."),
            ConnectPhase::Failed(reason) => {
                action = failed(ui, theme, reason);
            }
        }
    });
    action
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
    ui.horizontal(|ui| {
        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
            // `max(0.0)`: on a sub-200-px-wide window the offset goes
            // negative, which historically pushed the cursor past max_rect
            // and produced non-finite `Rect`s in the buttons that follow.
            let offset = ((ui.available_width() / 2.0) - 100.0).max(0.0);
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
