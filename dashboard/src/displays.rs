//! Active-display enumeration for the footer, read in the dashboard process.
//!
//! `sensord` runs as the LocalSystem service `PerfWindowSensor` in session 0,
//! which has no interactive desktop: the Win32 display APIs there report a
//! single virtualised 1024x768@60 mode no matter what the hardware is doing.
//! The dashboard runs in the user's own session, so it reads the active paths
//! itself with the CCD API (`GetDisplayConfigBufferSizes` +
//! `QueryDisplayConfig(QDC_ONLY_ACTIVE_PATHS)`) and renders them in the footer.
//!
//! The Win32 call is the only impure part. Everything about how the entries
//! are built, ordered and turned into footer text is a pure function so it can
//! be unit-tested without a desktop.

use std::time::{Duration, Instant};

#[cfg(windows)]
use windows::Win32::Devices::Display::{
    DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE, DISPLAYCONFIG_PATH_INFO,
};

/// How often the display list is re-read. Slow enough that the per-frame cost
/// is nil, fast enough that plugging a monitor or switching a mode shows up
/// within a few seconds.
pub const REFRESH_CADENCE: Duration = Duration::from_secs(3);

/// Widest number of display chips the footer renders individually. With more
/// displays than this the footer shows one `N DISPLAYS` chip and lists the
/// details in its tooltip, so the strip cannot overflow the window.
pub const MAX_EXPANDED_DISPLAYS: usize = 2;

/// A monitor's refresh rate as the rational CCD reports it (e.g. 7497/100 for
/// a "75 Hz" panel, 5994/100 for 59.94 Hz).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RefreshRate {
    pub numerator: u32,
    pub denominator: u32,
}

impl RefreshRate {
    /// The rate in Hz, or `None` when the driver reported no usable rate.
    pub fn hz(self) -> Option<f64> {
        if self.numerator == 0 || self.denominator == 0 {
            return None;
        }
        Some(self.numerator as f64 / self.denominator as f64)
    }

    /// Footer label: an integer when the rational is within 0.05 Hz of one
    /// (74.97 -> "75"), otherwise two decimals (59.94 -> "59.94"). "?" when
    /// the driver reported nothing usable.
    pub fn label(self) -> String {
        match self.hz() {
            None => "?".to_string(),
            Some(hz) => {
                let rounded = hz.round();
                if (hz - rounded).abs() <= 0.05 {
                    format!("{}", rounded as i64)
                } else {
                    format!("{hz:.2}")
                }
            }
        }
    }
}

/// One active monitor, as the footer needs it.
#[derive(Debug, Clone, PartialEq)]
pub struct DisplayMode {
    /// GDI device name, e.g. `\\.\DISPLAY5`.
    pub gdi_name: String,
    /// EDID friendly name, when the driver exposes one.
    pub model: Option<String>,
    pub width: u32,
    pub height: u32,
    pub refresh: RefreshRate,
    /// Desktop position of the source rectangle, in virtual-screen pixels.
    pub position: (i32, i32),
    /// True for the display whose source sits at (0,0).
    pub primary: bool,
}

/// The per-target record the Win32 layer produces, before ordering and the
/// primary flag are applied. Kept separate so [`assemble`] is pure.
#[derive(Debug, Clone, PartialEq)]
pub struct PathRecord {
    pub gdi_name: String,
    pub model: Option<String>,
    pub width: u32,
    pub height: u32,
    pub refresh: RefreshRate,
    pub position: (i32, i32),
}

/// Turn raw path records into the ordered display list: primary first, then
/// left to right, top to bottom. Ordering is stable, so cloned targets (several
/// monitors driven from one source) keep the order the driver reported.
pub fn assemble(records: Vec<PathRecord>) -> Vec<DisplayMode> {
    let mut displays: Vec<DisplayMode> = records
        .into_iter()
        .map(|r| DisplayMode {
            primary: r.position == (0, 0),
            gdi_name: r.gdi_name,
            model: r.model,
            width: r.width,
            height: r.height,
            refresh: r.refresh,
            position: r.position,
        })
        .collect();
    displays.sort_by_key(|d| (!d.primary, d.position.1, d.position.0));
    displays
}

/// One footer chip: the text to draw plus the tooltip explaining it.
#[derive(Debug, Clone, PartialEq)]
pub struct FooterChip {
    pub text: String,
    pub tooltip: String,
}

/// Footer text for the active displays. One chip per display while there are
/// at most [`MAX_EXPANDED_DISPLAYS`] of them; beyond that a single count chip
/// with every display named in its tooltip, so the strip never overflows.
pub fn footer_chips(displays: &[DisplayMode]) -> Vec<FooterChip> {
    if displays.is_empty() {
        return Vec::new();
    }
    if displays.len() <= MAX_EXPANDED_DISPLAYS {
        return displays.iter().map(display_chip).collect();
    }
    vec![FooterChip {
        text: format!("{} DISPLAYS", displays.len()),
        tooltip: displays
            .iter()
            .map(display_tooltip)
            .collect::<Vec<_>>()
            .join("\n"),
    }]
}

fn display_chip(display: &DisplayMode) -> FooterChip {
    FooterChip {
        text: format!(
            "{}x{} @ {}Hz",
            display.width,
            display.height,
            display.refresh.label()
        ),
        tooltip: display_tooltip(display),
    }
}

/// Tooltip line for one display: monitor model, GDI name, desktop position and
/// whether it is the primary.
fn display_tooltip(display: &DisplayMode) -> String {
    let model = display
        .model
        .as_deref()
        .map(str::trim)
        .filter(|m| !m.is_empty())
        .unwrap_or("Unknown monitor");
    let role = if display.primary {
        "primary"
    } else {
        "secondary"
    };
    format!(
        "{model}\n{} \u{b7} {},{} \u{b7} {role}",
        display.gdi_name, display.position.0, display.position.1
    )
}

/// The live display list plus the timestamp of the last read. Refreshed from
/// the dashboard's update loop on [`REFRESH_CADENCE`].
#[derive(Debug, Clone)]
pub struct DisplayCache {
    displays: Vec<DisplayMode>,
    refreshed_at: Option<Instant>,
}

impl Default for DisplayCache {
    fn default() -> Self {
        Self::new()
    }
}

impl DisplayCache {
    /// Read the active displays once and remember when.
    pub fn new() -> Self {
        let mut cache = Self {
            displays: Vec::new(),
            refreshed_at: None,
        };
        cache.refresh(Instant::now());
        cache
    }

    /// An empty cache that never touches Win32. Only for tests.
    pub fn empty() -> Self {
        Self {
            displays: Vec::new(),
            refreshed_at: None,
        }
    }

    pub fn displays(&self) -> &[DisplayMode] {
        &self.displays
    }

    /// Re-read the displays when the last read is older than
    /// [`REFRESH_CADENCE`]. Called once per frame; cheap when not due.
    pub fn maybe_refresh(&mut self, now: Instant) {
        let due = match self.refreshed_at {
            None => true,
            Some(last) => now.saturating_duration_since(last) >= REFRESH_CADENCE,
        };
        if due {
            self.refresh(now);
        }
    }

    /// Unconditionally re-read the displays.
    pub fn refresh(&mut self, now: Instant) {
        self.displays = enumerate();
        self.refreshed_at = Some(now);
    }
}

/// Enumerate every active monitor, ordered for the footer.
pub fn enumerate() -> Vec<DisplayMode> {
    assemble(enumerate_platform())
}

/// Trim an EDID friendly name; a blank descriptor reads as "no model" rather
/// than an empty label.
pub fn clean_model(friendly_name: &str) -> Option<String> {
    let trimmed = friendly_name.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// Index of this path's source mode in the `modes` array.
///
/// `sourceInfo.modeInfoIdx` is a union: normally the plain index into `modes`,
/// but the same word carries the clone-group bookkeeping for a cloned source,
/// and then it cannot be used as an index. When the direct lookup is unusable,
/// match the source's adapter/id against the mode array instead: a source-mode
/// entry carries exactly that adapter/id pair.
#[cfg(windows)]
fn mode_index_for_source(
    path: &DISPLAYCONFIG_PATH_INFO,
    modes: &[DISPLAYCONFIG_MODE_INFO],
) -> Option<usize> {
    // SAFETY: reading either arm of the union is a plain u32 read.
    let raw = unsafe { path.sourceInfo.Anonymous.modeInfoIdx as usize };
    if raw < modes.len()
        && modes[raw].infoType == DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE
        && modes[raw].adapterId == path.sourceInfo.adapterId
        && modes[raw].id == path.sourceInfo.id
    {
        return Some(raw);
    }
    modes.iter().position(|mode| {
        mode.infoType == DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE
            && mode.adapterId == path.sourceInfo.adapterId
            && mode.id == path.sourceInfo.id
    })
}

#[cfg(windows)]
fn enumerate_platform() -> Vec<PathRecord> {
    use std::mem::size_of;
    use windows::Win32::Devices::Display::{
        DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes, QueryDisplayConfig,
        DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME, DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
        DISPLAYCONFIG_DEVICE_INFO_HEADER, DISPLAYCONFIG_MODE_INFO,
        DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE, DISPLAYCONFIG_PATH_INFO,
        DISPLAYCONFIG_SOURCE_DEVICE_NAME, DISPLAYCONFIG_TARGET_DEVICE_NAME, QDC_ONLY_ACTIVE_PATHS,
    };

    // The CCD calls are only safe with buffers sized by the API itself, and the
    // topology can change between the size query and the query; a failed second
    // call just yields no displays until the next cadence tick.
    unsafe {
        let mut path_count: u32 = 0;
        let mut mode_count: u32 = 0;
        if GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut path_count, &mut mode_count).0
            != 0
        {
            return Vec::new();
        }

        let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); path_count as usize];
        let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); mode_count as usize];
        if QueryDisplayConfig(
            QDC_ONLY_ACTIVE_PATHS,
            &mut path_count,
            paths.as_mut_ptr(),
            &mut mode_count,
            modes.as_mut_ptr(),
            None,
        )
        .0 != 0
        {
            return Vec::new();
        }
        paths.truncate(path_count as usize);
        modes.truncate(mode_count as usize);

        let mut records = Vec::with_capacity(paths.len());
        for path in &paths {
            // A path whose source mode cannot be resolved is not renderable;
            // skip it instead of indexing out of bounds.
            let Some(mode_index) = mode_index_for_source(path, &modes) else {
                continue;
            };
            let mode_info = &modes[mode_index];
            if mode_info.infoType != DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE {
                continue;
            }
            let source_mode = mode_info.Anonymous.sourceMode;

            let mut source_name = DISPLAYCONFIG_SOURCE_DEVICE_NAME {
                header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
                    r#type: DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
                    size: size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32,
                    adapterId: path.sourceInfo.adapterId,
                    id: path.sourceInfo.id,
                },
                ..Default::default()
            };
            if DisplayConfigGetDeviceInfo(&mut source_name.header) != 0 {
                continue;
            }

            let mut target_name = DISPLAYCONFIG_TARGET_DEVICE_NAME {
                header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
                    r#type: DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
                    size: size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() as u32,
                    adapterId: path.targetInfo.adapterId,
                    id: path.targetInfo.id,
                },
                ..Default::default()
            };
            let model = (DisplayConfigGetDeviceInfo(&mut target_name.header) == 0)
                .then(|| clean_model(&wide_to_string(&target_name.monitorFriendlyDeviceName)))
                .flatten();

            records.push(PathRecord {
                gdi_name: wide_to_string(&source_name.viewGdiDeviceName),
                model,
                width: source_mode.width,
                height: source_mode.height,
                refresh: RefreshRate {
                    numerator: path.targetInfo.refreshRate.Numerator,
                    denominator: path.targetInfo.refreshRate.Denominator,
                },
                position: (source_mode.position.x, source_mode.position.y),
            });
        }
        records
    }
}

#[cfg(not(windows))]
fn enumerate_platform() -> Vec<PathRecord> {
    Vec::new()
}

/// Decode a NUL-terminated UTF-16 buffer as the Win32 APIs return it.
#[cfg(windows)]
fn wide_to_string(buffer: &[u16]) -> String {
    let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..len])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(clippy::too_many_arguments)]
    fn record(
        gdi: &str,
        model: Option<&str>,
        w: u32,
        h: u32,
        num: u32,
        den: u32,
        position: (i32, i32),
    ) -> PathRecord {
        PathRecord {
            gdi_name: gdi.to_string(),
            model: model.map(str::to_string),
            width: w,
            height: h,
            refresh: RefreshRate {
                numerator: num,
                denominator: den,
            },
            position,
        }
    }

    #[test]
    fn refresh_label_rounds_within_five_hundredths() {
        assert_eq!(
            RefreshRate {
                numerator: 7497,
                denominator: 100
            }
            .label(),
            "75"
        );
        assert_eq!(
            RefreshRate {
                numerator: 75,
                denominator: 1
            }
            .label(),
            "75"
        );
    }

    #[test]
    fn refresh_label_keeps_two_decimals_when_fractional() {
        assert_eq!(
            RefreshRate {
                numerator: 5994,
                denominator: 100
            }
            .label(),
            "59.94"
        );
        assert_eq!(
            RefreshRate {
                numerator: 4927,
                denominator: 100
            }
            .label(),
            "49.27"
        );
    }

    #[test]
    fn refresh_label_is_question_mark_without_a_rate() {
        assert_eq!(
            RefreshRate {
                numerator: 0,
                denominator: 0
            }
            .label(),
            "?"
        );
        assert_eq!(
            RefreshRate {
                numerator: 60,
                denominator: 0
            }
            .label(),
            "?"
        );
    }

    #[test]
    fn assemble_orders_primary_first_then_geometrically() {
        let displays = assemble(vec![
            // Secondary, to the right of the primary.
            record("\\\\.\\DISPLAY9", None, 1280, 1024, 60, 1, (1920, 0)),
            // Primary.
            record("\\\\.\\DISPLAY5", None, 1920, 1080, 75, 1, (0, 0)),
            // Secondary to the left of the primary.
            record("\\\\.\\DISPLAY1", None, 1920, 1080, 60, 1, (-1920, 0)),
            // Secondary below everything.
            record("\\\\.\\DISPLAY7", None, 1024, 768, 60, 1, (0, 1080)),
        ]);

        let order: Vec<&str> = displays.iter().map(|d| d.gdi_name.as_str()).collect();
        assert_eq!(
            order,
            [
                "\\\\.\\DISPLAY5",
                "\\\\.\\DISPLAY1",
                "\\\\.\\DISPLAY9",
                "\\\\.\\DISPLAY7"
            ]
        );
        assert!(displays[0].primary);
        assert!(displays[1..].iter().all(|d| !d.primary));
    }

    #[test]
    fn assemble_keeps_every_target_of_a_cloned_source() {
        // Cloned displays: one source at (0,0) feeding two different monitors.
        let displays = assemble(vec![
            record(
                "\\\\.\\DISPLAY5",
                Some("PHL 273V7"),
                1920,
                1080,
                75,
                1,
                (0, 0),
            ),
            record(
                "\\\\.\\DISPLAY5",
                Some("NE156FHM-NX6"),
                1920,
                1080,
                75,
                1,
                (0, 0),
            ),
        ]);

        assert_eq!(displays.len(), 2);
        assert_eq!(displays[0].gdi_name, displays[1].gdi_name);
        assert_eq!(displays[0].position, displays[1].position);
        assert_eq!(displays[0].model.as_deref(), Some("PHL 273V7"));
        assert_eq!(displays[1].model.as_deref(), Some("NE156FHM-NX6"));
    }

    #[test]
    fn footer_shows_one_chip_per_display_up_to_two() {
        let displays = assemble(vec![
            record(
                "\\\\.\\DISPLAY5",
                Some("PHL 273V7"),
                1920,
                1080,
                75,
                1,
                (0, 0),
            ),
            record(
                "\\\\.\\DISPLAY1",
                Some("NE156FHM-NX6"),
                1920,
                1080,
                5994,
                100,
                (-1920, 0),
            ),
        ]);
        let chips = footer_chips(&displays);

        assert_eq!(chips.len(), 2);
        assert_eq!(chips[0].text, "1920x1080 @ 75Hz");
        assert_eq!(chips[1].text, "1920x1080 @ 59.94Hz");
        assert!(chips[0].tooltip.contains("PHL 273V7"));
        assert!(chips[0].tooltip.contains("\\\\.\\DISPLAY5"));
        assert!(chips[0].tooltip.contains("0,0"));
        assert!(chips[0].tooltip.contains("primary"));
        assert!(chips[1].tooltip.contains("secondary"));
    }

    #[test]
    fn footer_collapses_three_or_more_displays_into_one_chip() {
        let displays = assemble(vec![
            record("\\\\.\\DISPLAY5", Some("A"), 1920, 1080, 75, 1, (0, 0)),
            record("\\\\.\\DISPLAY1", Some("B"), 1920, 1080, 60, 1, (-1920, 0)),
            record("\\\\.\\DISPLAY2", Some("C"), 1280, 1024, 60, 1, (1920, 0)),
        ]);
        let chips = footer_chips(&displays);

        assert_eq!(chips.len(), 1);
        assert_eq!(chips[0].text, "3 DISPLAYS");
        // Every display is still named in the tooltip.
        for model in ["A", "B", "C"] {
            assert!(chips[0].tooltip.contains(model));
        }
    }

    #[test]
    fn footer_is_empty_without_displays() {
        assert!(footer_chips(&[]).is_empty());
    }

    #[test]
    fn model_blank_or_whitespace_reads_as_none() {
        assert_eq!(clean_model(""), None);
        assert_eq!(clean_model("   "), None);
        assert_eq!(clean_model("  PHL 273V7  "), Some("PHL 273V7".to_string()));
    }

    #[test]
    fn cache_refreshes_only_when_the_cadence_elapses() {
        let mut cache = DisplayCache::empty();
        let t0 = Instant::now();
        assert!(cache.displays().is_empty());

        // Force a fixed, non-empty list so the assertion does not depend on
        // what the test machine's desktop looks like.
        cache.displays = assemble(vec![record(
            "\\\\.\\DISPLAY1",
            None,
            640,
            480,
            60,
            1,
            (0, 0),
        )]);
        cache.refreshed_at = Some(t0);

        cache.maybe_refresh(t0 + REFRESH_CADENCE / 2);
        assert_eq!(cache.displays().len(), 1);
        assert_eq!(cache.displays()[0].width, 640);

        // Past the cadence the cache re-reads from the platform, so the fake
        // 640x480 entry can never survive.
        cache.maybe_refresh(t0 + REFRESH_CADENCE);
        assert!(cache
            .displays()
            .iter()
            .all(|d| !(d.width == 640 && d.height == 480)));
    }
}
