use crate::config::{Config, ThemeId};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::Instant;

/// The theme to actually display, given the config and whether Windows is in
/// light mode. Pure: all OS access happens in `windows_is_light`.
pub fn effective_theme_id(config: &Config, os_is_light: bool) -> ThemeId {
    if config.follow_windows && os_is_light {
        ThemeId::Light
    } else {
        config.theme
    }
}

/// Reads `HKCU\..\Themes\Personalize\AppsUseLightTheme`. Defaults to dark
/// (`false`) when the key is unreadable.
pub fn windows_is_light() -> bool {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    hkcu.open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize")
        .and_then(|k| k.get_value::<u32, _>("AppsUseLightTheme"))
        .map(|v| v == 1)
        .unwrap_or(false)
}

/// Minimum interval between registry reads for the live follow-Windows poll.
const POLL_INTERVAL_MS: u64 = 2_000;
/// `last_check_ms` sentinel for "never checked" — the first poll always reads.
const NEVER_CHECKED: u64 = u64::MAX;

/// Throttle + cache around an OS theme reader, so a per-frame poll costs an
/// atomic load almost always and a registry read at most every 2 s. Lock-free
/// (frame loop, no contention concerns) and instantiable so tests can drive
/// it with a fake clock and reader.
struct OsThemePoll {
    last_check_ms: AtomicU64,
    cached_light: AtomicBool,
}

impl OsThemePoll {
    const fn new() -> Self {
        Self {
            last_check_ms: AtomicU64::new(NEVER_CHECKED),
            cached_light: AtomicBool::new(false),
        }
    }

    fn poll_at(&self, now_ms: u64, read: impl FnOnce() -> bool) -> bool {
        let last = self.last_check_ms.load(Ordering::Relaxed);
        if last == NEVER_CHECKED || now_ms.saturating_sub(last) >= POLL_INTERVAL_MS {
            self.last_check_ms.store(now_ms, Ordering::Relaxed);
            let light = read();
            self.cached_light.store(light, Ordering::Relaxed);
            light
        } else {
            self.cached_light.load(Ordering::Relaxed)
        }
    }
}

static OS_THEME_POLL: OsThemePoll = OsThemePoll::new();

/// Milliseconds since the first call — a process-local monotonic clock that
/// fits in an atomic, which `Instant` does not.
fn now_ms() -> u64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_millis() as u64
}

/// Throttled change-detector for the follow-Windows theme: returns
/// `Some(is_light)` when the cached OS theme no longer matches `current`,
/// `None` otherwise. Safe to call every frame — the registry is re-read at
/// most once per [`POLL_INTERVAL_MS`].
pub fn os_light_flipped(current: bool) -> Option<bool> {
    let light = OS_THEME_POLL.poll_at(now_ms(), windows_is_light);
    (light != current).then_some(light)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, ThemeId};

    fn cfg(theme: ThemeId, follow: bool) -> Config {
        Config {
            theme,
            follow_windows: follow,
            ..Config::default()
        }
    }

    #[test]
    fn follow_off_uses_the_configured_theme() {
        let c = cfg(ThemeId::Amber, false);
        assert_eq!(effective_theme_id(&c, true), ThemeId::Amber);
        assert_eq!(effective_theme_id(&c, false), ThemeId::Amber);
    }

    #[test]
    fn follow_on_uses_light_when_os_is_light() {
        let c = cfg(ThemeId::Amber, true);
        assert_eq!(effective_theme_id(&c, true), ThemeId::Light);
    }

    #[test]
    fn follow_on_uses_the_configured_dark_theme_when_os_is_dark() {
        let c = cfg(ThemeId::Phosphor, true);
        assert_eq!(effective_theme_id(&c, false), ThemeId::Phosphor);
    }

    #[test]
    fn follow_on_with_light_configured_stays_light_in_dark_os() {
        // The user picked Light as their "dark-mode" theme — degenerate but valid.
        let c = cfg(ThemeId::Light, true);
        assert_eq!(effective_theme_id(&c, false), ThemeId::Light);
    }

    #[test]
    fn poll_reads_immediately_on_first_call() {
        let poll = OsThemePoll::new();
        let reads = std::cell::Cell::new(0u32);
        let got = poll.poll_at(0, || {
            reads.set(reads.get() + 1);
            true
        });
        assert!(got);
        assert_eq!(reads.get(), 1);
    }

    #[test]
    fn poll_serves_the_cache_within_the_throttle_window() {
        let poll = OsThemePoll::new();
        let reads = std::cell::Cell::new(0u32);
        let read = || {
            reads.set(reads.get() + 1);
            true
        };
        assert!(poll.poll_at(0, read));
        // 1.9 s later: still inside the window — cached value, no read.
        assert!(poll.poll_at(1_900, read));
        assert_eq!(reads.get(), 1);
    }

    #[test]
    fn poll_rereads_once_the_interval_elapses() {
        let poll = OsThemePoll::new();
        let reads = std::cell::Cell::new(0u32);
        assert!(poll.poll_at(0, || {
            reads.set(reads.get() + 1);
            true
        }));
        // The OS flipped to dark; past the window the poll must observe it.
        assert!(!poll.poll_at(2_000, || {
            reads.set(reads.get() + 1);
            false
        }));
        assert_eq!(reads.get(), 2);
    }
}
