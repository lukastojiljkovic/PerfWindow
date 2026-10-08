use crate::format::TempUnit;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Identifies one of the six themes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeId {
    Amber,
    Slate,
    Phosphor,
    Light,
    Synthwave,
    Crimson,
}

/// The fixed set of poll intervals offered in Settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RefreshRate {
    Ms500,
    S1,
    S2,
    S5,
}

impl RefreshRate {
    pub fn as_millis(self) -> u32 {
        match self {
            RefreshRate::Ms500 => 500,
            RefreshRate::S1 => 1000,
            RefreshRate::S2 => 2000,
            RefreshRate::S5 => 5000,
        }
    }

    pub const ALL: [RefreshRate; 4] = [
        RefreshRate::Ms500,
        RefreshRate::S1,
        RefreshRate::S2,
        RefreshRate::S5,
    ];

    pub fn label(self) -> &'static str {
        match self {
            RefreshRate::Ms500 => "0.5 s",
            RefreshRate::S1 => "1 s",
            RefreshRate::S2 => "2 s",
            RefreshRate::S5 => "5 s",
        }
    }
}

/// Persisted user settings. This is the only state PerfWindow writes to disk.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    pub theme: ThemeId,
    pub follow_windows: bool,
    pub unit: TempUnit,
    pub refresh: RefreshRate,
    #[serde(default = "default_check_updates")]
    pub check_updates_on_startup: bool,
    #[serde(default = "default_cpu_heat_map")]
    pub cpu_heat_map: bool,
    #[serde(default = "default_always_on_top")]
    pub always_on_top: bool,
    /// Mini mode: a thin strip docked to the top of a monitor instead of the
    /// full window. Persisted so a relaunch comes back in the same mode.
    #[serde(default = "default_mini_strip")]
    pub mini_strip: bool,
    /// Device name (`\\.\DISPLAYn`) of the monitor the strip is docked to.
    /// `None` (and a name whose monitor no longer exists) means "the monitor
    /// the window is on".
    ///
    /// `skip_serializing_if` is not optional here: the TOML serializer rejects
    /// `None` outright, and one rejected field would turn the whole write into
    /// an empty file that silently resets every setting on the next launch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mini_strip_monitor: Option<String>,
    /// Which readings the strip shows. Last field on purpose: it serializes as
    /// a `[mini_strip_values]` table, and TOML requires tables after plain
    /// values.
    #[serde(default)]
    pub mini_strip_values: MiniStripValues,
}

fn default_check_updates() -> bool {
    true
}

fn default_cpu_heat_map() -> bool {
    false
}

fn default_always_on_top() -> bool {
    false
}

fn default_mini_strip() -> bool {
    false
}

/// Which readings the mini strip shows, left to right.
///
/// CPU load and CPU temperature are separate switches but share the strip's
/// single `CPU` label (as do GPU load and GPU temperature), so a user can keep
/// the temperature without the load figure or the other way round.
///
/// `#[serde(default)]` at the container level fills in any field an older
/// config file is missing from [`MiniStripValues::default`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MiniStripValues {
    pub cpu_load: bool,
    pub cpu_temp: bool,
    pub gpu_load: bool,
    pub gpu_temp: bool,
    pub vram: bool,
    pub ram: bool,
    pub network: bool,
    pub battery: bool,
}

impl Default for MiniStripValues {
    /// Everything but Battery: a desktop has no battery, and asking for one
    /// should be a deliberate click rather than a permanently empty reading.
    fn default() -> Self {
        Self {
            cpu_load: true,
            cpu_temp: true,
            gpu_load: true,
            gpu_temp: true,
            vram: true,
            ram: true,
            network: true,
            battery: false,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: ThemeId::Slate,
            follow_windows: false,
            unit: TempUnit::Celsius,
            refresh: RefreshRate::S1,
            check_updates_on_startup: true,
            cpu_heat_map: false,
            always_on_top: false,
            mini_strip: false,
            mini_strip_monitor: None,
            mini_strip_values: MiniStripValues::default(),
        }
    }
}

impl Config {
    /// `%APPDATA%\PerfWindow\config.toml`.
    pub fn path() -> Option<PathBuf> {
        let appdata = std::env::var_os("APPDATA")?;
        Some(
            PathBuf::from(appdata)
                .join("PerfWindow")
                .join("config.toml"),
        )
    }

    /// Load config from disk; any failure (missing, malformed) yields defaults.
    pub fn load() -> Self {
        Self::path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map(|s| Self::from_toml_str(&s))
            .unwrap_or_default()
    }

    /// Write config to disk, creating the directory. Errors are logged, not fatal.
    pub fn save(&self) {
        let Some(path) = Self::path() else { return };
        self.save_to(&path);
    }

    /// Atomic write via a sibling `.tmp` + rename, mirroring the update
    /// cache: a crash mid-write must leave the previous config intact, not a
    /// torn file that silently resets every setting on the next load.
    pub(crate) fn save_to(&self, path: &std::path::Path) {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let tmp = path.with_extension("toml.tmp");
        let result =
            std::fs::write(&tmp, self.to_toml_string()).and_then(|()| std::fs::rename(&tmp, path));
        if let Err(e) = result {
            eprintln!("PerfWindow: failed to save config: {e}");
        }
    }

    pub fn from_toml_str(s: &str) -> Self {
        toml::from_str(s).unwrap_or_default()
    }

    pub fn to_toml_string(&self) -> String {
        toml::to_string_pretty(self).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_toml() {
        let c = Config {
            theme: ThemeId::Amber,
            follow_windows: true,
            unit: TempUnit::Fahrenheit,
            refresh: RefreshRate::S2,
            check_updates_on_startup: true,
            cpu_heat_map: false,
            always_on_top: false,
            mini_strip: true,
            mini_strip_monitor: Some(r"\\.\DISPLAY2".to_string()),
            mini_strip_values: MiniStripValues {
                battery: true,
                network: false,
                ..MiniStripValues::default()
            },
        };
        let parsed = Config::from_toml_str(&c.to_toml_string());
        assert_eq!(parsed.theme, ThemeId::Amber);
        assert!(parsed.follow_windows);
        assert_eq!(parsed.unit, TempUnit::Fahrenheit);
        assert_eq!(parsed.refresh, RefreshRate::S2);
        assert!(parsed.mini_strip);
        assert_eq!(parsed.mini_strip_monitor.as_deref(), Some(r"\\.\DISPLAY2"));
        assert!(parsed.mini_strip_values.battery);
        assert!(!parsed.mini_strip_values.network);
        assert!(parsed.mini_strip_values.cpu_load);
    }

    #[test]
    fn malformed_or_empty_toml_falls_back_to_defaults() {
        let d = Config::default();
        assert_eq!(Config::from_toml_str("not = valid = toml").theme, d.theme);
        assert_eq!(Config::from_toml_str("").refresh, d.refresh);
    }

    #[test]
    fn refresh_rate_maps_to_milliseconds() {
        assert_eq!(RefreshRate::Ms500.as_millis(), 500);
        assert_eq!(RefreshRate::S1.as_millis(), 1000);
        assert_eq!(RefreshRate::S5.as_millis(), 5000);
    }

    #[test]
    fn check_updates_defaults_to_true_when_missing() {
        let parsed = Config::from_toml_str("");
        assert!(parsed.check_updates_on_startup);
    }

    #[test]
    fn check_updates_round_trips_when_false() {
        let c = Config {
            check_updates_on_startup: false,
            ..Config::default()
        };
        let parsed = Config::from_toml_str(&c.to_toml_string());
        assert!(!parsed.check_updates_on_startup);
    }

    #[test]
    fn cpu_heat_map_defaults_to_false_when_missing() {
        let parsed = Config::from_toml_str("");
        assert!(!parsed.cpu_heat_map);
    }

    #[test]
    fn cpu_heat_map_round_trips_when_true() {
        let c = Config {
            cpu_heat_map: true,
            ..Config::default()
        };
        let parsed = Config::from_toml_str(&c.to_toml_string());
        assert!(parsed.cpu_heat_map);
    }

    #[test]
    fn always_on_top_defaults_to_false_when_missing() {
        let parsed = Config::from_toml_str("");
        assert!(!parsed.always_on_top);
    }

    #[test]
    fn always_on_top_round_trips_when_true() {
        let c = Config {
            always_on_top: true,
            ..Config::default()
        };
        let parsed = Config::from_toml_str(&c.to_toml_string());
        assert!(parsed.always_on_top);
    }

    #[test]
    fn mini_strip_defaults_are_off_with_every_reading_but_battery() {
        let parsed = Config::from_toml_str("");
        assert!(!parsed.mini_strip);
        assert!(parsed.mini_strip_monitor.is_none());
        assert_eq!(parsed.mini_strip_values, MiniStripValues::default());
        assert!(!parsed.mini_strip_values.battery);
        assert!(parsed.mini_strip_values.cpu_load);
    }

    #[test]
    fn a_config_file_without_the_mini_fields_still_parses() {
        // The shape a pre-mini-mode PerfWindow wrote.
        let old = "\
theme = \"Amber\"
follow_windows = false
unit = \"Celsius\"
refresh = \"S5\"
check_updates_on_startup = false
cpu_heat_map = true
always_on_top = true
";
        let parsed = Config::from_toml_str(old);
        assert_eq!(parsed.theme, ThemeId::Amber);
        assert_eq!(parsed.refresh, RefreshRate::S5);
        assert!(parsed.cpu_heat_map && parsed.always_on_top);
        // The new fields fall back to their defaults.
        assert!(!parsed.mini_strip);
        assert!(parsed.mini_strip_monitor.is_none());
        assert_eq!(parsed.mini_strip_values, MiniStripValues::default());
    }

    #[test]
    fn mini_strip_values_round_trip_through_a_partial_table() {
        // The device name is written as a TOML literal string so the trailing
        // backslashes need no escape.
        let text = r#"
theme = "Slate"
follow_windows = false
unit = "Celsius"
refresh = "S1"
mini_strip = true
mini_strip_monitor = '\\.\DISPLAY1'

[mini_strip_values]
cpu_temp = false
battery = true
"#;
        let parsed = Config::from_toml_str(text);
        assert!(parsed.mini_strip);
        assert_eq!(parsed.mini_strip_monitor.as_deref(), Some(r"\\.\DISPLAY1"));
        assert!(!parsed.mini_strip_values.cpu_temp);
        assert!(parsed.mini_strip_values.battery);
        // Fields the table does not mention keep their defaults.
        assert!(parsed.mini_strip_values.ram);
    }

    #[test]
    fn serializing_without_a_strip_monitor_writes_a_non_empty_file() {
        // The `Option` field must be skipped, not written as an empty value:
        // a TOML write that fails halfway would leave the config unreadable.
        let text = Config::default().to_toml_string();
        assert!(!text.is_empty(), "a skipped None must not fail the write");
        assert!(!text.contains("mini_strip_monitor"));
        assert_eq!(Config::from_toml_str(&text), Config::default());
    }

    #[test]
    fn save_to_writes_a_parseable_file_and_leaves_no_tmp() {
        let dir = std::env::temp_dir().join(format!("pw-config-save-test-{}", std::process::id()));
        let path = dir.join("config.toml");
        let c = Config {
            theme: ThemeId::Amber,
            ..Config::default()
        };
        c.save_to(&path);
        let body = std::fs::read_to_string(&path).expect("config file exists");
        assert_eq!(Config::from_toml_str(&body).theme, ThemeId::Amber);
        assert!(!path.with_extension("toml.tmp").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
