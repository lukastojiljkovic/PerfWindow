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
    let (warn, hot) = match kind {
        TempKind::Processor => (75.0, 88.0),
        TempKind::Disk => (50.0, 60.0),
        TempKind::Board => (60.0, 80.0),
    };
    if celsius >= hot {
        theme.hot
    } else if celsius >= warn {
        theme.warn
    } else {
        theme.ok
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
