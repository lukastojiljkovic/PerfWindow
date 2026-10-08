//! The shared background-opacity slider: one track-and-knob drawing and one
//! click-to-value mapping, used by Settings → Display and by the mini strip.
//!
//! Both sliders edit the same [`crate::config::Config::background_opacity`]
//! setting; only their geometry differs. Keeping the drawing and the mapping
//! here means the strip's scaled-down copy can never drift from the full-size
//! one.

use crate::theme::Theme;
use egui::{Pos2, Rect, Stroke, StrokeKind, Vec2};

/// Step between the slider's stops. The 30..=100 range comes from
/// `crate::config::{MIN,MAX}_BACKGROUND_OPACITY`.
pub const STEP: u8 = 5;

/// The opacity a click at `x` on `track` selects: the nearest [`STEP`] stop
/// between the configured minimum and maximum, clamped at both ends.
pub fn opacity_at(track: Rect, x: f32) -> u8 {
    let min = crate::config::MIN_BACKGROUND_OPACITY;
    let max = crate::config::MAX_BACKGROUND_OPACITY;
    let travel = track.width().max(1.0);
    let frac = ((x - track.left()) / travel).clamp(0.0, 1.0);
    let stops = (max - min) / STEP;
    min + (frac * stops as f32).round() as u8 * STEP
}

/// `value` stepped by `notches` [`STEP`] stops (positive raises the opacity),
/// clamped to the configured range.
pub fn stepped(value: u8, notches: i32) -> u8 {
    let min = crate::config::MIN_BACKGROUND_OPACITY;
    let max = crate::config::MAX_BACKGROUND_OPACITY;
    let step = notches * i32::from(STEP);
    (i32::from(value.clamp(min, max)) + step).clamp(i32::from(min), i32::from(max)) as u8
}

/// Paint the knob position for `value` on `track`, as an absolute x.
pub fn knob_x(track: Rect, value: u8) -> f32 {
    let min = crate::config::MIN_BACKGROUND_OPACITY;
    let max = crate::config::MAX_BACKGROUND_OPACITY;
    let frac = (value.clamp(min, max) - min) as f32 / (max - min) as f32;
    track.left() + track.width() * frac
}

/// Draw an opacity slider's track and knob: a square-cornered track filled with
/// `accent` up to the knob and `track` after it, a 1 px `border` outline, and a
/// square `accent` knob with a `border` outline.
///
/// `knob_d` is the knob's side, so the caller can scale the whole control
/// down; the drawing itself is identical at any size.
pub fn paint(painter: &egui::Painter, theme: &Theme, track: Rect, value: u8, knob_d: f32) {
    let x = knob_x(track, value);
    painter.rect_filled(track, 0.0, theme.track);
    if x > track.left() {
        painter.rect_filled(
            Rect::from_min_max(track.min, Pos2::new(x, track.max.y)),
            0.0,
            theme.accent,
        );
    }
    painter.rect_stroke(
        track,
        0.0,
        Stroke::new(1.0_f32, theme.border),
        StrokeKind::Inside,
    );

    let knob = Rect::from_center_size(Pos2::new(x, track.center().y), Vec2::splat(knob_d));
    painter.rect_filled(knob, 0.0, theme.accent);
    painter.rect_stroke(
        knob,
        0.0,
        Stroke::new(1.0_f32, theme.border),
        StrokeKind::Inside,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The full-size Settings track: a 100 px run between the knob extremes.
    fn track() -> Rect {
        Rect::from_min_max(Pos2::new(10.0, 0.0), Pos2::new(110.0, 5.0))
    }

    #[test]
    fn the_track_ends_land_on_the_configured_bounds() {
        assert_eq!(opacity_at(track(), 10.0), 30);
        assert_eq!(opacity_at(track(), 110.0), 100);
        // Clicks past either end clamp instead of running away.
        assert_eq!(opacity_at(track(), -50.0), 30);
        assert_eq!(opacity_at(track(), 500.0), 100);
    }

    #[test]
    fn a_click_snaps_to_the_nearest_five_percent_stop() {
        // The run is 30..=100 in 5 % stops: 15 stops over 100 px.
        for stop in 0..=14 {
            let x = 10.0 + 100.0 * stop as f32 / 14.0;
            let value = opacity_at(track(), x);
            assert_eq!((value - 30) % 5, 0, "x {x} produced {value}");
            assert!((30..=100).contains(&value));
        }
        // Just under half a stop rounds down, just over rounds up.
        assert_eq!(opacity_at(track(), 10.0 + 100.0 * 0.4 / 14.0), 30);
        assert_eq!(opacity_at(track(), 10.0 + 100.0 * 0.6 / 14.0), 35);
    }

    #[test]
    fn a_degenerate_track_still_maps_to_a_valid_stop() {
        let point = Rect::from_min_max(Pos2::new(4.0, 0.0), Pos2::new(4.0, 0.0));
        assert_eq!(opacity_at(point, 4.0), 30);
    }

    #[test]
    fn the_wheel_steps_five_percent_and_clamps_at_both_ends() {
        assert_eq!(stepped(60, 1), 65);
        assert_eq!(stepped(60, -1), 55);
        assert_eq!(stepped(60, 3), 75);
        assert_eq!(stepped(100, 1), 100);
        assert_eq!(stepped(100, 5), 100);
        assert_eq!(stepped(30, -1), 30);
        assert_eq!(stepped(35, -2), 30);
        // An out-of-range stored value is clamped before it steps.
        assert_eq!(stepped(10, 0), 30);
        assert_eq!(stepped(200, 0), 100);
    }

    #[test]
    fn the_knob_sits_between_the_track_ends() {
        assert_eq!(knob_x(track(), 30), 10.0);
        assert_eq!(knob_x(track(), 100), 110.0);
        assert_eq!(knob_x(track(), 65), 60.0);
    }
}
