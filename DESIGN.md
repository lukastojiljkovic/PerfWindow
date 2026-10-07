---
name: PerfWindow
description: "A retro-utilitarian hardware-monitor dashboard. Six egui themes, each a named palette of background, panel, border, track, chrome, ink, dim, faint, accent, ok, warn and hot over one fixed card grid."
colors:
  bg: "#0a0e15"
  panel: "#121823"
  border: "#243144"
  track: "#1c2530"
  chrome: "#070a10"
  ink: "#d6dde8"
  dim: "#6d7989"
  faint: "#48515f"
  accent: "#34e0d0"
  accentSoft: "#7af0e4"
  ok: "#3ed089"
  warn: "#f5a524"
  hot: "#fb5b4e"
  webApp: "#0a756c"
  webAppDark: "#34e0d0"
typography:
  display: "ChakraPetch"
  data: "PlexMono"
  dataBold: "PlexMonoBold"
  displayAlt: "SpaceMono"
rounded:
  chrome: 0.0
  webButton: 2px
spacing:
  windowWidth: 1180.0
  windowHeight: 600.0
  windowMinWidth: 720.0
  windowMinHeight: 500.0
  gridGap: 10.0
  gridBodyPadding: 13
  cardPadding: 11
  cardItemSpacing: 10.0
  stripPaddingX: 13
  stripPaddingY: 9
  statRowHeight: 22.0
  statValueSize: 14.0
  statValueMinSize: 9.0
  statLabelGap: 6.0
  accentThickness: 2.0
  settingsWidth: 600.0
  updateModalWidth: 520.0
components:
  card:
    backgroundColor: "{colors.panel}"
    textColor: "{colors.ink}"
    rounded: "{rounded.chrome}"
    padding: "{spacing.cardPadding}"
  statRow:
    textColor: "{colors.ink}"
    typography: "{typography.data}"
    height: "{spacing.statRowHeight}"
  titleBar:
    backgroundColor: "{colors.chrome}"
    textColor: "{colors.dim}"
    height: "{spacing.stripPaddingY}"
  buttonPrimary:
    backgroundColor: "{colors.accent}"
    textColor: "{colors.bg}"
    typography: "{typography.data}"
    rounded: "{rounded.chrome}"
  buttonSecondary:
    backgroundColor: "{colors.panel}"
    textColor: "{colors.dim}"
    typography: "{typography.data}"
    rounded: "{rounded.chrome}"
  banner:
    backgroundColor: "{colors.panel}"
    textColor: "{colors.ink}"
    padding: "{spacing.stripPaddingX}"
  gaugeRing:
    backgroundColor: "{colors.track}"
    textColor: "{colors.accent}"
    size: "86.0"
---

# Design System: PerfWindow

## Overview

**Creative North Star: "the retro-utilitarian instrument panel"**

PerfWindow draws a fixed grid of sensor cards with egui, and every colour it
uses comes from a `Theme` struct defined in `dashboard/src/theme/mod.rs` (the
README calls the result "a single retro-utilitarian dashboard"). A theme is a
palette of named roles, two font roles and three CRT-effect amounts
(`scanline_opacity`, `vignette`, `glow_px`). The default is **Cyber Slate**;
the other five are Amber Mainframe, Phosphor Tactical, Synthwave Neon, Crimson
Terminal and Light.

**The One Palette Rule.** A panel paints only colours read from the active
`Theme`.

**The Named Role Rule.** Colours are used by role — `bg`, `panel`, `border`,
`track`, `chrome`, `ink`, `dim`, `faint`, `accent`, `accent_soft`, `ok`, `warn`,
`hot` — never by their hex string.

**The Slot Rule.** `Theme::apply` sets the egui theme preference before it
pushes the palette into `Visuals`, so a light or dark PerfWindow theme lands in
the matching egui style slot.

## Colors

The palette tokens are the fields of the `Theme` struct, and the values below
are the default theme (Cyber Slate) as defined in `dashboard/src/theme/mod.rs`:

| Token | Value | Role |
| --- | --- | --- |
| `bg` | `#0a0e15` | Window and card-grid background |
| `panel` | `#121823` | Card fill |
| `border` | `#243144` | Card outlines and rules |
| `track` | `#1c2530` | Bar and ring tracks |
| `chrome` | `#070a10` | Title bar and footer strip |
| `ink` | `#d6dde8` | Primary text |
| `dim` | `#6d7989` | Secondary text |
| `faint` | `#48515f` | Disabled and placeholder text |
| `accent` | `#34e0d0` | Selection, key figures, arcs |
| `accent_soft` | `#7af0e4` | Accent hover and secondary accent |
| `ok` | `#3ed089` | Healthy status |
| `warn` | `#f5a524` | Warning status |
| `hot` | `#fb5b4e` | Critical status |

The other themes keep the same role names and change the values: Amber
Mainframe (`accent` `#ffa31a`, `bg` `#100b04`), Phosphor Tactical (`accent`
`#3edc62`, `bg` `#060a07`), Synthwave Neon (`accent` `#c44eff`, `bg` `#0a0612`),
Crimson Terminal (`accent` `#dc2626`, `bg` `#0e0606`) and Light (`accent`
`#0f8e83`, `bg` `#eef0f3`, `dark: false`).

The website uses its own two colour variables, set in
`site/index.html`: `--app: #0a756c` and `--app-dark: #34e0d0`, with the rest of
the page taking its paper / ink / hairline values from `site/site.css`.

**The Six-Theme Rule.** Six `ThemeId` values ship, and `Theme::for_id` is the
only place a palette is spelled out.

**The Status-Colour Rule.** `ok`, `warn` and `hot` mark status, but the number
they describe is always printed too, so colour is never the only signal.

**The Accent-Restraint Rule.** In every theme the accent is the colour that
carries selection and key figures; the rest of a card stays on `panel`, `ink`
and `dim`.

## Typography

Four families ship in `dashboard/assets/fonts/` and are registered by
`install_fonts` in `dashboard/src/theme/mod.rs`:

| Token | Family | File |
| --- | --- | --- |
| `PlexMono` | IBM Plex Mono Medium | `IBMPlexMono-Medium.ttf` |
| `PlexMonoBold` | IBM Plex Mono SemiBold | `IBMPlexMono-SemiBold.ttf` |
| `ChakraPetch` | Chakra Petch SemiBold | `ChakraPetch-SemiBold.ttf` |
| `SpaceMono` | Space Mono Regular | `SpaceMono-Regular.ttf` |

A theme names one family for `font_display` and one for `font_data`. Cyber
Slate uses `ChakraPetch` for display and `PlexMono` for data; Amber Mainframe
uses `PlexMono` for both, Crimson Terminal uses `PlexMonoBold` for display and
`PlexMono` for data, and Phosphor Tactical uses Space Mono for both.

Sizes are set per widget rather than by a type ramp. The stat row uses
`VALUE_SIZE` 14.0 with a `MIN_VALUE_SIZE` of 9.0 in `widgets/stat.rs`, the
settings labels use `LABEL_FONT_SIZE` 10.0, the loading screen uses
`WORDMARK_SIZE` 24.0 and `PHRASE_SIZE` 13.0, and the gauge is 86.0 across.

**The Two-Role Rule.** Widgets ask the theme for a font role (`font_display` or
`font_data`), never for a family by name.

**The Bundled-Fonts Rule.** Fonts are embedded with `include_bytes!`, so the
dashboard never depends on a font installed on the system.

## Layout

- The window opens at 1180.0 × 600.0 and cannot shrink below 720.0 × 500.0
  (`dashboard/src/main.rs`); the min width sits just below the four-column
  breakpoint so the grid can fall to three columns.
- The card grid uses `GRID_GAP` 10.0 between cards and `GRID_BODY_PADDING` 13
  around the body (`ui/mod.rs`). Each card pads its contents by `CARD_PADDING`
  11 with `CARD_ITEM_SPACING` 10.0 between rows (`panels/mod.rs`).
- Card heights come from named constants: `STORAGE_BASE_HEIGHT` 72.0,
  `STORAGE_DISK_ROW_HEIGHT` 42.0, `SENSORS_CARD_HEIGHT` 210.0 and
  `BATTERY_CARD_HEIGHT` 170.0.
- The title bar pads by `STRIP_PADDING_X` 13 and `STRIP_PADDING_Y_TB` 9, and
  the footer strip uses `STRIP_PADDING_Y_FOOT` 7 (`ui/mod.rs`), and a stat row
  is `ROW_H` 22.0 tall with a `LABEL_GAP` of 6.0.
- The settings modal is 600.0 wide (`ui/settings.rs`); the update modal is 520.0
  (`ui/update_modal.rs`).
- The website lays out on a 12-column grid inside a `min(100% - 2 *
  var(--margin), 1344px)` wrap, with `--margin: clamp(20px, 3.4vw, 48px)` and
  `--gutter: 24px` from `site/site.css`.

**The Fixed-Grid Rule.** Cards sit on one grid; the available width decides how
many columns there are, not a per-panel layout.

**The Constants Rule.** Spacing lives in named `const`s, so a gap is changed in
one place instead of in every call site.

## Elevation & Depth

egui draws no shadows here. Depth is expressed with fills and a one-pixel
border: `Theme::apply` sets `panel_fill`, `window_fill` and `extreme_bg_color`,
and gives non-interactive widgets a `Stroke::new(1.0_f32, self.border)`
(`dashboard/src/theme/mod.rs`). The dark themes add CRT depth instead — each
has a `scanline_opacity`, a `vignette` and a `glow_px` value, drawn by
`ui/effects.rs` (`GRID_STEP` 25.0, `GRID_ALPHA` 0.05, `SCANLINE_STEP` 4.0,
`VIGNETTE_MAX_ALPHA` 80.0). The Light theme sets `scanline_opacity` and
`glow_px` to 0.

**The Flat-Surfaces Rule.** Surfaces are separated by a border and a fill
difference, not by a drop shadow.

**The Light-Is-Flat Rule.** The Light theme keeps the CRT effects off, and the
theme test asserts it.

## Shapes

Custom-painted chrome is square: the buttons, chips and modal frames call
`rect_filled` and `rect_stroke` with a corner radius of `0.0` (for example in
`ui/update_modal.rs`). The load-bar cells carry a `CORNER_PAD` of 3.0
(`widgets/bars.rs`), and the gauge is a ring of `OUTER_R` 43.0 with a
`RING_THICK` of 8.0 (`widgets/gauge.rs`). On the website, buttons have a 2px
radius and everything else is square (`site/site.css`).

**The Square-Chrome Rule.** A custom-drawn control has a zero corner radius, so
it reads as part of a machine panel.

**The Two-Pixel Web Rule.** On the website only buttons are rounded, and only by
2px.

## Components

| Component | Built from |
| --- | --- |
| Sensor card | The card frame in `panels/mod.rs`: `panel` fill, `border` outline, 11 padding, a 2.0 accent bar across the top, a title row on `ink` and rows on `dim`. |
| Stat row | `widgets/stat.rs`: label on `dim`, value on `ink` at 14.0, shrunk to a minimum of 9.0 before it may overlap the label. |
| Load bar / core strip | `widgets/bars.rs`: cells of 44.0 × 28.0 (32.0 × 18.0 in the compact form) with a 4.0 gap, filled on `accent` over a `track`. |
| Gauge | `widgets/gauge.rs`: an 86.0 ring, 43.0 radius, 8.0 thick, drawn on `track` and `accent`. |
| Sparkline | `widgets/sparkline.rs`: a history trace between a 30.0 minimum and 200.0 maximum height. |
| Title bar / footer | `ui/mod.rs`: `chrome` strip, `STRIP_PADDING_X` 13, `STRIP_PADDING_Y` 9, text on `dim`, the version number on `accent`. |
| Banners | `ui/update_banner.rs` and `ui/health_banner.rs`: a `panel` strip with an 8.0 gap between its parts. |
| Buttons | Primary: `accent` fill, `bg` text. Secondary: `panel` fill, `border` outline, `dim` text. Both are square. |
| Website plate | `site/site.css`: the app screenshot on a field mixed from `--app` at 16%, a hairline outline around it. |

**The Tooltip Rule.** Every stat row carries a one-sentence tooltip, so a
reading is explained where it appears.

**The Missing-Reading Rule.** A reading the hardware does not expose is hidden
or marked, never filled with a fake number.

## Do's and Don'ts

- **Do** take every colour from the active `Theme`; **don't** hard-code a hex
  outside `dashboard/src/theme/mod.rs`.
- **Do** use the display font for headings and the data font for figures and
  labels; **don't** mix a third family into a theme.
- **Do** leave the Light theme without CRT effects; **don't** add scanlines or
  glow to it.
- **Do** keep custom chrome square; **don't** round it on the desktop — the 2px
  radius belongs to the website's buttons.
- **Do** keep the dashboard on the shared grid and its named spacing constants;
  **don't** hand-place a panel with its own margins.
- **Do** print the number behind a status colour; **don't** let colour be the
  only thing that says something is wrong.
