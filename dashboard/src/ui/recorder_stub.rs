//! No-op stand-in for [`crate::ui::recorder`] in production builds.
//!
//! Mirrors the recording API so call sites compile unchanged; every function
//! is empty, which is what keeps the geometry recorder out of the shipping
//! binary.

use egui::Rect;

/// Replaces [`crate::ui::recorder::Record`] in production builds.
#[derive(Debug, Clone, PartialEq)]
pub enum Record {}

pub fn begin() {}

pub fn card(_name: &str, _slot: Rect, _rect: Rect) {}

pub fn stat_row(_label: Rect, _value: Rect, _elided: bool, _text: &str) {}

pub fn central(_rect: Rect) {}

pub fn grid(_rect: Rect) {}

/// Production never records, so this always yields an empty list.
pub fn take() -> Vec<Record> {
    Vec::new()
}
