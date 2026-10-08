//! The chrome every modal shares: the shadowed frame, the `chrome` title strip
//! with its wordmark, and the bordered close button. Settings, the update
//! modal and the changelog viewer draw through these, so the three never drift
//! apart.

use crate::format::letter_spaced;
use crate::theme::Theme;
use egui::{Color32, Margin, Response, Sense, Stroke, StrokeKind, Vec2};

const TB_PADDING_X: i8 = 15;
const TB_PADDING_Y: i8 = 12;

/// The modal's outer frame: `bg` fill, 1 px `border`, and a soft drop shadow.
pub fn frame(theme: &Theme) -> egui::Frame {
    egui::Frame::NONE
        .fill(theme.bg)
        .stroke(Stroke::new(1.0_f32, theme.border))
        .shadow(egui::epaint::Shadow {
            offset: [0, 18],
            blur: 48,
            spread: 0,
            color: Color32::from_black_alpha(140),
        })
}

/// Draw the title strip: `label` letter-spaced in the display font on the
/// left, the close button on the right, and a 1 px `border` rule along the
/// bottom. Returns the close button's response.
pub fn title_bar(ui: &mut egui::Ui, theme: &Theme, label: &str) -> Response {
    let inner = egui::Frame::NONE
        .fill(theme.chrome)
        .inner_margin(Margin::symmetric(TB_PADDING_X, TB_PADDING_Y))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(letter_spaced(label))
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

    let rect = inner.response.rect;
    ui.painter().add(egui::Shape::line_segment(
        [rect.left_bottom(), rect.right_bottom()],
        Stroke::new(1.0_f32, theme.border),
    ));
    inner.inner
}

/// The close button: an X inside a 1 px `border` box that fills `accent` on
/// hover. The X is painted as two line segments rather than a glyph, because
/// not every bundled font carries U+2715 and a text X rendered as tofu.
pub fn close_button(ui: &mut egui::Ui, theme: &Theme) -> Response {
    let pad = Vec2::new(9.0, 5.0);
    let glyph = Vec2::new(11.0, 11.0);
    let (rect, response) = ui.allocate_exact_size(glyph + pad * 2.0, Sense::click());

    if ui.is_rect_visible(rect) {
        let painter = ui.painter_at(rect);
        let (fill, stroke_color, mark_color) = if response.hovered() {
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
            Stroke::new(1.0_f32, stroke_color),
            StrokeKind::Inside,
        );
        let inner = egui::Rect::from_min_size(rect.min + pad, glyph);
        let stroke = Stroke::new(1.5_f32, mark_color);
        painter.line_segment([inner.left_top(), inner.right_bottom()], stroke);
        painter.line_segment([inner.right_top(), inner.left_bottom()], stroke);
    }
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
}
