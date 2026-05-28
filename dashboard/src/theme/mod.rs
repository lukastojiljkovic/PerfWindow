pub mod system;

use crate::config::ThemeId;
use egui::{Color32, FontData, FontDefinitions};

/// Which bundled font family a theme uses for a role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontFamily {
    PlexMono,
    PlexMonoBold,
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
            ThemeId::Synthwave => Theme {
                id,
                name: "Synthwave Neon",
                dark: true,
                bg: hex("#0a0612"),
                panel: hex("#15102a"),
                border: hex("#3d2a6e"),
                track: hex("#1f1843"),
                chrome: hex("#050208"),
                ink: hex("#e8dcff"),
                dim: hex("#8b78c4"),
                faint: hex("#4d4068"),
                accent: hex("#c44eff"),
                accent_soft: hex("#d97aff"),
                ok: hex("#4ade80"),
                warn: hex("#f5a524"),
                hot: hex("#ff4d6d"),
                font_display: FontFamily::ChakraPetch,
                font_data: FontFamily::SpaceMono,
                scanline_opacity: 0.16,
                vignette: 0.5,
                glow_px: 8.0,
            },
            ThemeId::Crimson => Theme {
                id,
                name: "Crimson Terminal",
                dark: true,
                bg: hex("#0e0606"),
                panel: hex("#1a0c0c"),
                border: hex("#4a1818"),
                track: hex("#261212"),
                chrome: hex("#050202"),
                ink: hex("#f4d8d8"),
                dim: hex("#b07878"),
                faint: hex("#6e4848"),
                accent: hex("#dc2626"),
                accent_soft: hex("#ef4444"),
                ok: hex("#4ade80"),
                warn: hex("#fbbf24"),
                hot: hex("#ff8e3a"),
                font_display: FontFamily::PlexMonoBold,
                font_data: FontFamily::PlexMono,
                scanline_opacity: 0.16,
                vignette: 0.5,
                glow_px: 7.0,
            },
        }
    }
}

/// Register the three bundled families with egui. Call once at startup.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();

    macro_rules! font {
        ($name:literal, $file:literal) => {
            fonts.font_data.insert(
                $name.to_owned(),
                std::sync::Arc::new(FontData::from_static(include_bytes!(concat!(
                    "../../assets/fonts/",
                    $file
                )))),
            );
        };
    }
    font!("plex", "IBMPlexMono-Medium.ttf");
    font!("plex-bold", "IBMPlexMono-SemiBold.ttf");
    font!("chakra", "ChakraPetch-SemiBold.ttf");
    font!("space", "SpaceMono-Regular.ttf");

    use egui::FontFamily::Name;
    // egui's default Monospace family carries the symbol / emoji fallback
    // fonts (notably `Hack`, which has the block-drawing and geometric
    // glyphs). Append it to every bundled family so UI symbols the bundled
    // fonts lack — ▚ ▮ ◐ ⚙ ● ↓ ↑ ✕ — render instead of showing as tofu.
    let fallback: Vec<String> = fonts
        .families
        .get(&egui::FontFamily::Monospace)
        .cloned()
        .unwrap_or_default();
    let family = |primary: &str| {
        let mut chain = vec![primary.to_owned()];
        chain.extend(fallback.iter().cloned());
        chain
    };
    fonts.families.insert(Name("plex".into()), family("plex"));
    fonts
        .families
        .insert(Name("plex-bold".into()), family("plex-bold"));
    fonts
        .families
        .insert(Name("chakra".into()), family("chakra"));
    fonts.families.insert(Name("space".into()), family("space"));
    fonts
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default()
        .insert(0, "plex".into());

    ctx.set_fonts(fonts);
}

impl FontFamily {
    /// The egui family handle for this role.
    pub fn egui(self) -> egui::FontFamily {
        match self {
            FontFamily::PlexMono => egui::FontFamily::Name("plex".into()),
            FontFamily::PlexMonoBold => egui::FontFamily::Name("plex-bold".into()),
            FontFamily::ChakraPetch => egui::FontFamily::Name("chakra".into()),
            FontFamily::SpaceMono => egui::FontFamily::Name("space".into()),
        }
    }
}

impl Theme {
    /// Push this theme's palette into egui's `Visuals` so default widgets and
    /// the window background match. Panels paint their own colours explicitly.
    ///
    /// `set_theme` is called first so `set_visuals` writes into the correct
    /// style slot (egui keeps two: `dark_style` and `light_style`, and picks
    /// the writer based on the current preference). Without it, switching
    /// between a dark and light PerfWindow theme keeps writing into whichever
    /// slot egui picked for the OS preference at startup, which both makes
    /// the new colours land in the wrong slot and clones the `Arc<Style>` on
    /// every switch — a slow leak that the cumulative theme-cycle stresses.
    pub fn apply(&self, ctx: &egui::Context) {
        ctx.set_theme(if self.dark {
            egui::ThemePreference::Dark
        } else {
            egui::ThemePreference::Light
        });
        let mut visuals = if self.dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };
        visuals.panel_fill = self.bg;
        visuals.window_fill = self.panel;
        visuals.extreme_bg_color = self.track;
        visuals.override_text_color = Some(self.ink);
        visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, self.border);
        ctx.set_visuals(visuals);
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
            ThemeId::Synthwave,
            ThemeId::Crimson,
        ] {
            let t = Theme::for_id(id);
            assert_eq!(t.id, id);
        }
    }

    #[test]
    fn rapid_theme_cycling_does_not_panic() {
        // Regression test for a cumulative-style-allocation crash that fired
        // after 4–6 quick theme clicks: `apply` used to write into whichever
        // style slot egui picked at startup rather than the slot matching the
        // new palette, so each call cloned the underlying Arc<Style> and
        // bloated the egui context. set_theme now forces the correct slot
        // first so set_visuals lands in place. Twenty full cycles (120 calls)
        // covers that crash window with comfortable headroom.
        let ctx = egui::Context::default();
        let palette = [
            ThemeId::Amber,
            ThemeId::Slate,
            ThemeId::Phosphor,
            ThemeId::Light,
            ThemeId::Synthwave,
            ThemeId::Crimson,
        ];
        for _ in 0..20 {
            for id in palette {
                Theme::for_id(id).apply(&ctx);
            }
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
        for id in [
            ThemeId::Amber,
            ThemeId::Slate,
            ThemeId::Phosphor,
            ThemeId::Synthwave,
            ThemeId::Crimson,
        ] {
            assert!(Theme::for_id(id).scanline_opacity > 0.0);
        }
    }
}
