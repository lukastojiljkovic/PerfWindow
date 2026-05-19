pub mod system;

use crate::config::ThemeId;
use egui::Color32;

/// Which bundled font family a theme uses for a role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontFamily {
    PlexMono,
    ChakraPetch,
    SpaceMono,
}

/// A complete visual theme: palette, fonts, CRT-effect parameters.
#[derive(Debug, Clone)]
pub struct Theme {
    pub id: ThemeId,
    pub name: &'static str,
    pub dark: bool,
    // palette
    pub bg: Color32,
    pub panel: Color32,
    pub border: Color32,
    pub track: Color32,
    pub chrome: Color32,
    pub ink: Color32,
    pub dim: Color32,
    pub faint: Color32,
    pub accent: Color32,
    pub accent_soft: Color32,
    pub ok: Color32,
    pub warn: Color32,
    pub hot: Color32,
    // typography
    pub font_display: FontFamily,
    pub font_data: FontFamily,
    // effects
    pub scanline_opacity: f32,
    pub vignette: f32,
    pub glow_px: f32,
}

/// `#rrggbb` hex -> `Color32`. For compile-time-known literals only.
const fn hex(s: &str) -> Color32 {
    let b = s.as_bytes();
    const fn nyb(c: u8) -> u8 {
        match c {
            b'0'..=b'9' => c - b'0',
            b'a'..=b'f' => c - b'a' + 10,
            b'A'..=b'F' => c - b'A' + 10,
            _ => 0,
        }
    }
    Color32::from_rgb(
        nyb(b[1]) * 16 + nyb(b[2]),
        nyb(b[3]) * 16 + nyb(b[4]),
        nyb(b[5]) * 16 + nyb(b[6]),
    )
}

impl Theme {
    pub fn for_id(id: ThemeId) -> Theme {
        match id {
            ThemeId::Amber => Theme {
                id,
                name: "Amber Mainframe",
                dark: true,
                bg: hex("#100b04"),
                panel: hex("#1c1409"),
                border: hex("#48360f"),
                track: hex("#2f2410"),
                chrome: hex("#0a0703"),
                ink: hex("#f1ddb0"),
                dim: hex("#a98f57"),
                faint: hex("#6f5a32"),
                accent: hex("#ffa31a"),
                accent_soft: hex("#ffce6e"),
                ok: hex("#ffb733"),
                warn: hex("#ffd277"),
                hot: hex("#ff5535"),
                font_display: FontFamily::PlexMono,
                font_data: FontFamily::PlexMono,
                scanline_opacity: 0.16,
                vignette: 0.5,
                glow_px: 7.0,
            },
            ThemeId::Slate => Theme {
                id,
                name: "Cyber Slate",
                dark: true,
                bg: hex("#0a0e15"),
                panel: hex("#121823"),
                border: hex("#243144"),
                track: hex("#1c2530"),
                chrome: hex("#070a10"),
                ink: hex("#d6dde8"),
                dim: hex("#6d7989"),
                faint: hex("#48515f"),
                accent: hex("#34e0d0"),
                accent_soft: hex("#7af0e4"),
                ok: hex("#3ed089"),
                warn: hex("#f5a524"),
                hot: hex("#fb5b4e"),
                font_display: FontFamily::ChakraPetch,
                font_data: FontFamily::PlexMono,
                scanline_opacity: 0.04,
                vignette: 0.34,
                glow_px: 5.0,
            },
            ThemeId::Phosphor => Theme {
                id,
                name: "Phosphor Tactical",
                dark: true,
                bg: hex("#060a07"),
                panel: hex("#0d150e"),
                border: hex("#21401f"),
                track: hex("#152119"),
                chrome: hex("#040603"),
                ink: hex("#c7e9c6"),
                dim: hex("#5f8a62"),
                faint: hex("#3f5e42"),
                accent: hex("#3edc62"),
                accent_soft: hex("#86f29a"),
                ok: hex("#3edc62"),
                warn: hex("#e8b53a"),
                hot: hex("#ff4d3d"),
                font_display: FontFamily::SpaceMono,
                font_data: FontFamily::SpaceMono,
                scanline_opacity: 0.18,
                vignette: 0.5,
                glow_px: 8.0,
            },
            ThemeId::Light => Theme {
                id,
                name: "Light",
                dark: false,
                bg: hex("#eef0f3"),
                panel: hex("#ffffff"),
                border: hex("#d9dde3"),
                track: hex("#e6e9ee"),
                chrome: hex("#e7eaef"),
                ink: hex("#222a35"),
                dim: hex("#737d8c"),
                faint: hex("#a2abb8"),
                accent: hex("#0f8e83"),
                accent_soft: hex("#3fb3a8"),
                ok: hex("#2f9e57"),
                warn: hex("#bd7d12"),
                hot: hex("#d23f33"),
                font_display: FontFamily::ChakraPetch,
                font_data: FontFamily::PlexMono,
                scanline_opacity: 0.0,
                vignette: 0.04,
                glow_px: 0.0,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ThemeId;

    #[test]
    fn every_theme_id_resolves() {
        for id in [
            ThemeId::Amber,
            ThemeId::Slate,
            ThemeId::Phosphor,
            ThemeId::Light,
        ] {
            let t = Theme::for_id(id);
            assert_eq!(t.id, id);
        }
    }

    #[test]
    fn light_theme_has_no_crt_effects() {
        let light = Theme::for_id(ThemeId::Light);
        assert_eq!(light.scanline_opacity, 0.0);
        assert_eq!(light.glow_px, 0.0);
    }

    #[test]
    fn dark_themes_enable_scanlines() {
        for id in [ThemeId::Amber, ThemeId::Slate, ThemeId::Phosphor] {
            assert!(Theme::for_id(id).scanline_opacity > 0.0);
        }
    }
}
