use serde::{Deserialize, Serialize};

/// Temperature display unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TempUnit {
    Celsius,
    Fahrenheit,
}

impl TempUnit {
    /// Convert a Celsius reading into this unit.
    pub fn convert(self, celsius: f64) -> f64 {
        match self {
            TempUnit::Celsius => celsius,
            TempUnit::Fahrenheit => celsius * 9.0 / 5.0 + 32.0,
        }
    }
}

/// `"58°"`, or `"—"` when the reading is absent or non-finite. Rounded to a
/// whole degree.
pub fn format_temp(celsius: Option<f64>, unit: TempUnit) -> String {
    match finite(celsius) {
        Some(c) => format!("{}°", unit.convert(c).round() as i64),
        None => "—".to_string(),
    }
}

/// Compact temperature: like [`format_temp`], but documented as the right
/// choice when the surrounding UI is too small for a unit suffix (the active
/// unit is already implied by the title-bar °C/°F chip).
pub fn format_temp_compact(celsius: Option<f64>, unit: TempUnit) -> String {
    match finite(celsius) {
        Some(c) => format!("{}°", unit.convert(c).round() as i64),
        None => "—".to_string(),
    }
}

/// Render a link speed (bits/sec) as `"1 Gbps"` or `"100 Mbps"`. Truncates
/// toward zero so a marketing `1_000_000_000` bps Ethernet link reads as
/// `1 Gbps`. Absent or sub-Mbps values render as `"—"`.
pub fn format_link(bps: Option<i64>) -> String {
    match bps {
        Some(b) if b >= 1_000_000_000 => format!("{} Gbps", b / 1_000_000_000),
        Some(b) if b >= 1_000_000 => format!("{} Mbps", b / 1_000_000),
        _ => "—".to_string(),
    }
}

/// Human throughput, e.g. `"4.2 MB/s"`. Bytes/sec in; 1024-based KB/MB/GB out
/// (binary, consistent with `format_gb_from_mb`). A non-finite reading
/// renders `"—"` rather than a literal `"NaN KB/s"` / `"inf GB/s"`.
pub fn format_bytes_per_sec(bps: f64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;
    if !bps.is_finite() {
        "—".to_string()
    } else if bps >= GB {
        format!("{:.1} GB/s", bps / GB)
    } else if bps >= MB {
        format!("{:.1} MB/s", bps / MB)
    } else {
        format!("{:.0} KB/s", bps / KB)
    }
}

/// Megabytes rendered as a GB number string (one decimal), or `"—"` when
/// absent or non-finite.
pub fn format_gb_from_mb(mb: Option<f64>) -> String {
    match finite(mb) {
        Some(mb) => format!("{:.1}", mb / 1024.0),
        None => "—".to_string(),
    }
}

/// A megabytes pair rendered as `"<used> / <total> GB"`, one decimal each, or
/// `"—"` when either value is absent or non-finite.
pub fn format_gb_pair(used_mb: Option<f64>, total_mb: Option<f64>) -> String {
    match (finite(used_mb), finite(total_mb)) {
        (Some(u), Some(t)) => format!("{:.1} / {:.1} GB", u / 1024.0, t / 1024.0),
        _ => "—".to_string(),
    }
}

/// Discard a non-finite (`NaN` or infinite) reading. A sensor that returns
/// garbage must degrade to "absent" rather than render as a confident value or
/// a literal `"NaN"`. A `Some` finite value passes through unchanged.
pub fn finite(x: Option<f64>) -> Option<f64> {
    x.filter(|v| v.is_finite())
}

/// Round a percentage-style `Option<f64>` to a whole-number string; an absent
/// or non-finite value renders `"—"`.
pub fn format_percent(load: Option<f64>) -> String {
    match finite(load) {
        Some(v) => format!("{}", v.round() as i64),
        None => "—".to_string(),
    }
}

/// Render a positive seconds count as `"3d 14h"`, `"12h 30m"`, `"45m"` or
/// `"<1m"`. Negative input is clamped to zero.
pub fn format_uptime(secs: i64) -> String {
    let s = secs.max(0);
    let d = s / 86_400;
    let h = (s % 86_400) / 3_600;
    let m = (s % 3_600) / 60;
    match (d, h, m) {
        (d, h, _) if d > 0 => format!("{d}d {h}h"),
        (_, h, m) if h > 0 => format!("{h}h {m}m"),
        (_, _, m) if m > 0 => format!("{m}m"),
        _ => "<1m".to_string(),
    }
}

/// Insert thin spaces between characters to approximate CSS `letter-spacing`
/// on the wordmark, chips and arrow labels.
pub fn letter_spaced(text: &str) -> String {
    // Each inter-character gap is a 3-byte thin space; reserve generously.
    let mut out = String::with_capacity(text.len() * 4);
    for (i, ch) in text.chars().enumerate() {
        if i > 0 {
            out.push('\u{2009}'); // thin space
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_celsius_to_fahrenheit() {
        assert_eq!(TempUnit::Fahrenheit.convert(100.0), 212.0);
        assert_eq!(TempUnit::Fahrenheit.convert(0.0), 32.0);
        assert_eq!(TempUnit::Celsius.convert(58.0), 58.0);
    }

    #[test]
    fn formats_temperature_with_unit_suffix() {
        assert_eq!(format_temp(Some(58.0), TempUnit::Celsius), "58°");
        assert_eq!(format_temp(Some(58.0), TempUnit::Fahrenheit), "136°");
        assert_eq!(format_temp(None, TempUnit::Celsius), "—");
    }

    #[test]
    fn formats_temperature_compactly_without_unit_suffix() {
        assert_eq!(format_temp_compact(Some(58.0), TempUnit::Celsius), "58°");
        assert_eq!(
            format_temp_compact(Some(58.0), TempUnit::Fahrenheit),
            "136°"
        );
        assert_eq!(format_temp_compact(None, TempUnit::Celsius), "—");
    }

    #[test]
    fn formats_link_speed() {
        assert_eq!(format_link(Some(1_000_000_000)), "1 Gbps");
        assert_eq!(format_link(Some(2_500_000_000)), "2 Gbps");
        assert_eq!(format_link(Some(100_000_000)), "100 Mbps");
        assert_eq!(format_link(Some(10_000_000)), "10 Mbps");
        assert_eq!(format_link(None), "—");
    }

    #[test]
    fn formats_throughput_per_second() {
        assert_eq!(format_bytes_per_sec(4_404_019.0), "4.2 MB/s");
        assert_eq!(format_bytes_per_sec(629_145.0), "614 KB/s");
        assert_eq!(format_bytes_per_sec(0.0), "0 KB/s");
    }

    #[test]
    fn formats_gigabytes_from_megabytes() {
        assert_eq!(format_gb_from_mb(Some(15462.0)), "15.1");
        assert_eq!(format_gb_from_mb(None), "—");
    }

    #[test]
    fn formats_a_gb_pair() {
        assert_eq!(format_gb_pair(Some(6348.0), Some(12288.0)), "6.2 / 12.0 GB");
        assert_eq!(format_gb_pair(None, Some(12288.0)), "—");
        assert_eq!(format_gb_pair(Some(6348.0), None), "—");
    }

    #[test]
    fn non_finite_temperatures_render_as_em_dash() {
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(format_temp(Some(bad), TempUnit::Celsius), "—");
            assert_eq!(format_temp(Some(bad), TempUnit::Fahrenheit), "—");
            assert_eq!(format_temp_compact(Some(bad), TempUnit::Celsius), "—");
        }
    }

    #[test]
    fn non_finite_throughput_renders_as_em_dash() {
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(format_bytes_per_sec(bad), "—");
        }
    }

    #[test]
    fn non_finite_megabyte_figures_render_as_em_dash() {
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(format_gb_from_mb(Some(bad)), "—");
            assert_eq!(format_gb_pair(Some(bad), Some(12288.0)), "—");
            assert_eq!(format_gb_pair(Some(6348.0), Some(bad)), "—");
        }
    }

    #[test]
    fn non_finite_percentages_render_as_em_dash() {
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(format_percent(Some(bad)), "—");
        }
    }

    #[test]
    fn finite_discards_non_finite_readings() {
        assert_eq!(finite(Some(42.0)), Some(42.0));
        assert_eq!(finite(Some(f64::NAN)), None);
        assert_eq!(finite(Some(f64::INFINITY)), None);
        assert_eq!(finite(Some(f64::NEG_INFINITY)), None);
        assert_eq!(finite(None), None);
    }

    #[test]
    fn formats_whole_percentages() {
        assert_eq!(format_percent(Some(34.2)), "34");
        assert_eq!(format_percent(Some(99.6)), "100");
        assert_eq!(format_percent(Some(f64::NAN)), "—");
        assert_eq!(format_percent(None), "—");
    }

    #[test]
    fn letter_spaces_with_thin_spaces() {
        assert_eq!(letter_spaced("AB"), "A\u{2009}B");
        assert_eq!(letter_spaced("X"), "X");
        assert_eq!(letter_spaced(""), "");
    }

    #[test]
    fn formats_uptime_in_human_terms() {
        assert_eq!(format_uptime(0), "<1m");
        assert_eq!(format_uptime(30), "<1m");
        assert_eq!(format_uptime(60), "1m");
        assert_eq!(format_uptime(45 * 60), "45m");
        assert_eq!(format_uptime(3 * 3600), "3h 0m");
        assert_eq!(format_uptime(3 * 3600 + 25 * 60), "3h 25m");
        assert_eq!(format_uptime(4 * 86_400 + 14 * 3600), "4d 14h");
        assert_eq!(format_uptime(-5), "<1m");
    }
}
