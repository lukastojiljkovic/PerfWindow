//! Component panels: one card per snapshot section (CPU, GPU, RAM, storage,
//! sensors, network).
//!
//! Each panel composes the `crate::widgets` primitives inside the shared
//! [`card`] frame. The grid layout that places these panels is wired up in a
//! later task.

pub mod cpu;
pub mod gpu;
pub mod network;
pub mod ram;
pub mod sensors;
pub mod storage;

use crate::theme::Theme;
use egui::{FontId, Frame, Margin, Pos2, RichText, Sense, Stroke, Vec2};

/// Inner padding of a panel card, in pixels (mockup `.pw-panel { padding: 11px }`).
const CARD_PADDING: i8 = 11;
/// Vertical gap between items stacked inside a card (mockup `.pw-panel { gap: 10px }`).
const CARD_ITEM_SPACING: f32 = 10.0;
/// Thickness of the accent line along the card's top edge
/// (mockup `.pw-panel { border-top: 2px solid var(--accent) }`).
const TOP_ACCENT_THICKNESS: f32 = 2.0;
/// Height of a panel's empty-state row — a single centred dimmed line.
const EMPTY_NOTE_H: f32 = 22.0;

/// Draw the standard panel card and run `contents` inside it.
///
/// The card has a `theme.panel` fill, a 1 px `theme.border` stroke, ~11 px of
/// inner padding and ~10 px of vertical spacing between stacked items. A 2 px
/// `theme.accent` line is painted over the card's top edge, matching the
/// mockup's `border-top: 2px solid var(--accent)`.
pub fn card(ui: &mut egui::Ui, theme: &Theme, contents: impl FnOnce(&mut egui::Ui)) {
    let frame = Frame::NONE
        .fill(theme.panel)
        .stroke(Stroke::new(1.0, theme.border))
        .inner_margin(Margin::same(CARD_PADDING));

    let inner = frame.show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = CARD_ITEM_SPACING;
        contents(ui);
    });

    // Paint the accent strip over the card's top edge. Drawn after the frame so
    // it sits on top of the 1 px border rather than being covered by it.
    let rect = inner.response.rect;
    let top = egui::Rect::from_min_size(rect.min, Vec2::new(rect.width(), TOP_ACCENT_THICKNESS));
    ui.painter().rect_filled(top, 0.0, theme.accent);
}

/// Draw a panel's title row: an upper-cased `title` on the left in the display
/// font and `theme.accent`, plus an optional right-aligned upper-cased `sub` in
/// the data font and `theme.faint`. Matches `.pw-pt` / `.pw-sub` in the mockup.
pub fn panel_title(ui: &mut egui::Ui, theme: &Theme, title: &str, sub: Option<&str>) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(title.to_uppercase())
                .family(theme.font_display.egui())
                .size(12.0)
                .color(theme.accent),
        );
        if let Some(sub) = sub {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(sub.to_uppercase())
                        .family(theme.font_data.egui())
                        .size(9.0)
                        .color(theme.faint),
                );
            });
        }
    });
}

/// Draw a panel's empty-state: one centred, dimmed line of `text`.
///
/// Used by panels that legitimately have nothing to show — a machine with no
/// readable motherboard sensors, or no active network adapter. It fabricates
/// nothing and cannot panic.
pub fn empty_note(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), EMPTY_NOTE_H),
        Sense::hover(),
    );
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter_at(rect);
    let font = FontId::new(11.0, theme.font_data.egui());
    let galley = painter.layout_no_wrap(text.to_owned(), font, theme.dim);
    painter.galley(
        Pos2::new(
            rect.center().x - galley.size().x / 2.0,
            rect.center().y - galley.size().y / 2.0,
        ),
        galley,
        theme.dim,
    );
}
