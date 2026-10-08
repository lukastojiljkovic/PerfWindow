pub mod bars;
pub mod gauge;
pub mod sparkline;
pub mod stat;

use crate::theme::Theme;
use egui::Color32;

/// Severity bands for a temperature. `kind` selects the per-component
/// thresholds — CPU/GPU run hotter than disks before they are "warn".
#[derive(Clone, Copy)]
pub enum TempKind {
    Processor, // CPU, GPU
    Disk,
    Board,
}

/// ok / warn / hot colour for a Celsius reading, per the spec's colour coding.
pub fn temp_color(celsius: f64, kind: TempKind, theme: &Theme) -> Color32 {
    temp_alert_color(celsius, kind, theme).unwrap_or(theme.ok)
}

/// The warn/hot colour for a Celsius reading, or `None` when the reading is in
/// the "ok" band. Callers that paint calm values in their own colour take this
/// instead of [`temp_color`], so a value only turns amber or red where the
/// dashboard itself would colour it: the mini strip leaves calm readings in
/// `ink` rather than turning every cool component green.
pub fn temp_alert_color(celsius: f64, kind: TempKind, theme: &Theme) -> Option<Color32> {
    let (warn, hot) = match kind {
        TempKind::Processor => (75.0, 88.0),
        TempKind::Disk => (50.0, 60.0),
        TempKind::Board => (60.0, 80.0),
    };
    if celsius >= hot {
        Some(theme.hot)
    } else if celsius >= warn {
        Some(theme.warn)
    } else {
        None
    }
}

/// ok / warn / hot colour for a drive's remaining-life percentage.
/// 100 % is a new drive, 0 % is end-of-life.
pub fn health_color(percent: f64, theme: &Theme) -> Color32 {
    if percent < 50.0 {
        theme.hot
    } else if percent < 80.0 {
        theme.warn
    } else {
        theme.ok
    }
}
