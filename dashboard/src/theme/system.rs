use crate::config::{Config, ThemeId};

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
}
