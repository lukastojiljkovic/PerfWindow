//! The header-strip banner announcing a newer release.
//!
//! Rendered between the title bar and the central panel by [`crate::app`].
//! Visible only when [`crate::update::UpdateState::Available`] and the user
//! has not pressed `Later` in the current session. Styled to match the
//! existing title-bar chip idiom.

use crate::app::PerfApp;
use crate::theme::Theme;
use crate::update::UpdateState;
use egui::{Align, FontId, Layout, Margin, RichText, Sense, Stroke, StrokeKind, Vec2};

const STRIP_PADDING_X: i8 = 13;
const STRIP_PADDING_Y: i8 = 9;
const GAP: f32 = 8.0;

/// Returns true if a banner should be drawn this frame.
pub fn is_visible(app: &PerfApp) -> bool {
    if app.update_banner_dismissed {
        return false;
    }
    matches!(
        *app.update_state.lock().unwrap(),
        UpdateState::Available { .. }
    )
}

/// Paint the banner. Caller hosts this inside a `Panel::top` so the layout
/// math is identical to the title bar's chrome strip.
pub fn update_banner(ui: &mut egui::Ui, app: &mut PerfApp) {
    let theme = app.theme.clone();
    let (tag, headline) = {
        let guard = app.update_state.lock().unwrap();
        if let UpdateState::Available { release, .. } = &*guard {
            (
                "\u{25b2} NEW VERSION",
                format!("{} is available", release.name),
            )
        } else {
            return;
        }
    };

    let frame = egui::Frame::NONE
        .fill(theme.chrome)
        .inner_margin(Margin::symmetric(STRIP_PADDING_X, STRIP_PADDING_Y));

    let mut update_clicked = false;
    let mut later_clicked = false;

    frame.show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = GAP;

            ui.label(
                RichText::new(crate::format::letter_spaced(tag))
                    .family(theme.font_display.egui())
                    .size(10.0)
                    .color(theme.accent),
            );

            ui.label(
                RichText::new(headline)
                    .family(theme.font_data.egui())
                    .size(11.0)
                    .color(theme.ink),
            );

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if banner_chip(ui, &theme, "Later", false).clicked() {
                    later_clicked = true;
                }
                if banner_chip(ui, &theme, "Update", true).clicked() {
                    update_clicked = true;
                }
            });
        });
    });

    let rect = ui.min_rect();
    ui.painter().add(egui::Shape::line_segment(
        [rect.left_bottom(), rect.right_bottom()],
        Stroke::new(1.0, theme.border),
    ));

    if update_clicked {
        app.update_modal_open = true;
    }
    if later_clicked {
        app.update_banner_dismissed = true;
    }
}

/// A chip in the banner. Same visual idiom as the title-bar chip, with a
/// slightly larger label.
fn banner_chip(ui: &mut egui::Ui, theme: &Theme, label: &str, primary: bool) -> egui::Response {
    let font = FontId::new(11.0, theme.font_data.egui());
    let text_color = if primary { theme.bg } else { theme.dim };
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font, text_color);

    let pad = Vec2::new(10.0, 6.0);
    let size = galley.size() + pad * 2.0;
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());

    if ui.is_rect_visible(rect) {
        let painter = ui.painter_at(rect);
        let stroke_color = if primary { theme.accent } else { theme.border };
        if primary {
            painter.rect_filled(rect, 0.0, theme.accent);
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
