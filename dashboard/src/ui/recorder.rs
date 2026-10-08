//! Test-only geometry recorder.
//!
//! The whole UI (chrome + grid) is analysed as one zoomed layout, so the
//! acceptance tests need the actual rectangles the renderer produced — not an
//! approximation. Widgets push their rectangles here while a test is
//! recording; [`take`] drains them.
//!
//! Compiled only with `cfg(test)` or the `test-support` feature. Production
//! builds use [`crate::ui::recorder_stub`] instead, whose functions are empty,
//! so recording costs the shipping binary nothing.

use egui::Rect;
use std::cell::RefCell;

/// One captured rectangle.
#[derive(Debug, Clone, PartialEq)]
pub enum Record {
    /// A card's outer frame. `slot` is the grid cell the layout allocated and
    /// `rect` is the frame the card actually painted — the pair that proves
    /// the content stayed inside its cell.
    Card {
        name: String,
        slot: Rect,
        rect: Rect,
    },
    /// A stat row: the label text rectangle, the value text rectangle, and
    /// whether the value had to be elided. `text` is the full value.
    StatRow {
        label: Rect,
        value: Rect,
        elided: bool,
        text: String,
    },
    /// The whole central panel's body rectangle.
    Central(Rect),
    /// The grid's used rectangle (all cards plus padding).
    Grid(Rect),
}

thread_local! {
    static EVENTS: RefCell<Vec<Record>> = const { RefCell::new(Vec::new()) };
}

/// Drop everything recorded so far. Called once per rendered frame so a test
/// that runs several frames only inspects the last one.
pub fn begin() {
    EVENTS.with(|e| e.borrow_mut().clear());
}

/// Capture a card's allocated cell and its painted frame.
pub fn card(name: &str, slot: Rect, rect: Rect) {
    push(Record::Card {
        name: name.to_owned(),
        slot,
        rect,
    });
}

/// Capture one stat row's label/value rectangles.
pub fn stat_row(label: Rect, value: Rect, elided: bool, text: &str) {
    push(Record::StatRow {
        label,
        value,
        elided,
        text: text.to_owned(),
    });
}

/// Capture the central panel body rectangle.
pub fn central(rect: Rect) {
    push(Record::Central(rect));
}

/// Capture the grid's used rectangle.
pub fn grid(rect: Rect) {
    push(Record::Grid(rect));
}

/// Drain every record captured since the last [`begin`].
pub fn take() -> Vec<Record> {
    EVENTS.with(|e| std::mem::take(&mut *e.borrow_mut()))
}

fn push(record: Record) {
    EVENTS.with(|e| e.borrow_mut().push(record));
}
