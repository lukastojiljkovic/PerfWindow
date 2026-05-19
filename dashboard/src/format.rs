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

/// `"58°"`, or `"—"` when the reading is absent. Rounded to a whole degree.
pub fn format_temp(celsius: Option<f64>, unit: TempUnit) -> String {
    match celsius {
        Some(c) => format!("{}°", unit.convert(c).round() as i64),
        None => "—".to_string(),
    }
}

/// Human throughput, e.g. `"4.2 MB/s"`. Bytes/sec in; 1024-based KB/MB/GB out
/// (binary, consistent with `format_gb_from_mb` and the mockup).
pub fn format_bytes_per_sec(bps: f64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;
    if bps >= GB {
        format!("{:.1} GB/s", bps / GB)
    } else if bps >= MB {
        format!("{:.1} MB/s", bps / MB)
    } else {
        format!("{:.0} KB/s", bps / KB)
    }
}

/// Megabytes rendered as a GB number string (one decimal), or `"—"`.
pub fn format_gb_from_mb(mb: Option<f64>) -> String {
    match mb {
        Some(mb) => format!("{:.1}", mb / 1024.0),
        None => "—".to_string(),
    }
}

/// A megabytes pair rendered as `"<used> / <total> GB"`, one decimal each, or
/// `"—"` when either value is absent.
pub fn format_gb_pair(used_mb: Option<f64>, total_mb: Option<f64>) -> String {
    match (used_mb, total_mb) {
        (Some(u), Some(t)) => format!("{:.1} / {:.1} GB", u / 1024.0, t / 1024.0),
        _ => "—".to_string(),
    }
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
}
