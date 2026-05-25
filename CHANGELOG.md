# Changelog

All notable changes to PerfWindow are documented in this file.

The format is based on [Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.4.1] — 2026-05-25

### Added
- **Keep window always on top** toggle. Available two ways:
  a pushpin (`📌`) chip in the title bar next to the heat-map and
  unit chips for a quick flip, and a matching **DISPLAY** section
  toggle inside Settings for discoverability. The preference is
  persisted to `config.toml` (`always_on_top`, default `false`)
  and applied to the window via `egui::ViewportCommand::WindowLevel`.
  Launching with the preference on opens the window at the correct
  Z-level immediately — no startup flash.

## [0.4.0] — 2026-05-25

### Added
- **Battery panel** for laptops: charge donut, instantaneous charge or
  discharge rate (W) with a direction arrow, estimated time remaining
  while discharging, and battery health (Full Charged Capacity ÷ Design
  Capacity, colour-banded like disk health). The card is conditional on
  a battery being present; desktops are unaffected.
- **Per-drive HEALTH column** in the Storage panel, showing remaining
  life (0–100 %, 100 = new). Sourced from LHM's "Remaining Life" sensor
  with a fallback to the NVMe-spec "Available Spare". Colour-coded: ok
  at ≥ 80 %, warn at 50–80 %, hot below 50 %. Drives that expose
  neither sensor render an em-dash and stay neutral.
- **Per-disk Read / Write throughput** under each storage row, formatted
  with the same human-friendly bytes-per-second formatter as the
  Network panel. Disk row height grew from 25 to 38 px to fit the
  secondary line.
- **GPU Hot Spot temperature** stat row in the GPU panel, when the
  vendor driver exposes it. On most NVIDIA discrete GPUs hot-spot runs
  10–15 °C above the GPU Core reading.
- **CPU Vcore** stat row in the CPU panel, when the sensor is exposed.
  Sources `Vcore`, `CPU VID`, or `CPU Core` from the LHM voltage
  sensors in that order.
- **System uptime** in the footer, formatted as `UP 3d 14h`, sourced
  from `Environment.TickCount64` on the sensord side.
- **Clickable footer version** opens an in-app changelog viewer that
  embeds `CHANGELOG.md` at compile time and parses it with a tiny
  hand-rolled markdown scanner. No browser hand-off, no new
  dependency; closes with the window ✕ or ESC.

### Changed
- The `sensord` NDJSON schema gains optional fields: `uptime_sec`,
  `battery`, `storage[].health`, `storage[].read_bps`,
  `storage[].write_bps`, `cpu.voltage_v`, and `gpu[].hot_spot_temp`.
  All are absent when the underlying hardware doesn't expose the
  reading; the dashboard tolerates older `sensord` builds via
  `#[serde(default)]`.
- Battery-aware layout: when a battery is present, the second grid row
  shows Battery + Storage (+ Sensors if populated). Storage's column
  span shrinks to keep the row to exactly the available column count.

## [0.3.0] — 2026-05-24

### Added
- GitHub Actions CI: every push and pull request now runs the full
  Rust + .NET build, test, lint, and security audit, with a smoke
  build of the Windows installer.
- GitHub Actions release pipeline: pushing a `v*` tag now builds the
  installer, computes its SHA256, extracts the matching CHANGELOG
  section as release notes, and publishes the release. The
  maintainer's release flow is now `git tag -a vX.X.X && git push`.
- Snapshot-based UI regression coverage via `egui_kittest`: the
  settings and update modals are captured per theme so a future
  theme or layout regression fails CI with a pixel-diff PNG.
- Dependabot weekly updates for cargo, NuGet, and GitHub Actions
  dependencies.
- README badges for CI status, latest release, and license.

### Changed
- The dashboard's `cargo clippy` warnings are now build errors in CI
  (`-D warnings`). The `update::mod` module no longer re-exports
  three names that were never used outside the module; `ModalPhase`
  now derives `Default`; `ymdhm_local` carries a local
  `allow(upper_case_acronyms)` for its Win32 type names.
- Dashboard test count grew from 61 to 83; sensord test count grew
  from 24 to 43. Approximate coverage: dashboard ≥ 85 % on non-UI
  code, sensord ≥ 90 % (measured locally with `cargo llvm-cov`; not
  published to a coverage service).

User-facing application behaviour is unchanged.

## [0.2.6] — 2026-05-24

### Fixed
- The Settings and Update modals can no longer extend past the bottom of
  the app window. Previously, on a non-maximised or short window, the
  modals grew to their natural content height, clipping the title bar's
  close button and the modals' bottom controls off-screen. The body of
  each modal is now wrapped in a `ScrollArea` whose maximum height is
  capped at the viewport height minus the chrome; if the body fits, the
  modal shrinks to it, otherwise a scrollbar appears so the title bar's
  ✕ and the action buttons stay reachable.

## [0.2.5] — 2026-05-24

### Changed
- The footer status dot now shows the running app version (e.g. `● v0.2.5`)
  instead of the hardcoded `● LIVE` label when sensord is streaming. The
  version is baked in at compile time from `Cargo.toml`, so the footer
  cannot drift from the actual binary. The `● NO SIGNAL` state is
  unchanged.

## [0.2.4] — 2026-05-24

### Fixed
- The installer now registers a Windows Defender Allow rule for the
  WinRing0 threat signature (`VulnerableDriver:WinNT/Winring0`,
  ThreatID `2147937641`) in addition to the existing folder and process
  exclusions, so the recurring "Severe Trojan" alert no longer surfaces
  after the application starts. The detection fires inside Defender's
  kernel scanner once LibreHardwareMonitor loads the driver service and
  is not suppressed by file-path exclusions; a ThreatID-level allow is
  the only setting that stops it. The installer also reconciles any
  existing WinRing0 detections found in Defender's history and allows
  their threat IDs too, so a future Microsoft definition update that
  renumbers the signature self-heals on the next install without
  shipping a new installer. The uninstaller reverses every change.

## [0.2.3] — 2026-05-24

### Added
- Two new dark themes: **Synthwave Neon** (electric magenta-purple on
  deep purple-black, ChakraPetch + SpaceMono fonts) and **Crimson Terminal**
  (deep crimson on warm red-black, PlexMonoBold + PlexMono fonts). Selectable
  from Settings → Theme alongside the existing four themes.

### Changed
- The Settings theme picker now lays out cards as 3 columns × 2 rows instead
  of 4 × 1, to accommodate the additional themes without shrinking each card.

## [0.2.2] — 2026-05-23

### Fixed
- The installer now detects the Microsoft Visual C++ 2015-2022 Redistributable
  on the target machine and installs it silently if missing. Previously the
  application could fail to launch on a machine without the runtime, with no
  visible error (the Windows PE loader exits the process before any of our
  code runs). The bundled redistributable is the official x64 build hosted by
  Microsoft. If the silent install ever fails, the installer surfaces an
  error dialog offering to open the Microsoft download page in the browser
  and aborts rather than shipping a binary that cannot start.

## [0.2.1] — 2026-05-22

### Removed
- The `FAN` stat row in the GPU panel. The underlying sensor field
  (`GpuInfo.fan_rpm`) is unchanged — it stays in the snapshot schema —
  but LibreHardwareMonitor does not expose a GPU fan reading on the
  hardware the project targets, so the row was permanently empty.

### Changed
- Top-row card height tightened by one stat-row worth now that the GPU
  card has four rows instead of five.

## [0.2.0] — 2026-05-22

### Added
- In-app update detection that polls GitHub Releases on startup and surfaces
  a banner when a newer version is available, with a guided installer
  hand-off.
- `CHANGELOG.md` covering the release history from 0.1.0 onward.
- Settings → Updates section: opt-out toggle, manual "Check for updates now"
  action and last-checked timestamp.
- GPU dual-line history graph: compute load overlaid with the GPU memory
  controller load on the same sparkline canvas, with a `MEM USE` stat row.
- Per-core CPU heat-map, toggled from a new title-bar chip. Each core's
  cell is coloured by load and labelled with the per-core temperature
  when available.
- Per-direction network throughput history feeding a dual-line sparkline.

### Changed
- Card grid restructured to a 4-column layout on wide windows: Network
  joins CPU, GPU and RAM in the top row; Storage spans the second row.
- The Sensors card is now hidden entirely on machines without readable
  motherboard / fan / voltage sensors; Storage expands to fill its row.
- Cards in the same row are aligned to a uniform height from a static
  per-card-kind table, with sparklines growing to fill the leftover
  vertical space — no empty band below the grid.
- The Network panel was rewritten in the dashboard's standard idiom:
  donut + stat rows (DOWN/UP/LINK) + dual-line throughput sparkline,
  replacing the previous arrow meters.
- Empty-state notes inside panels now wrap to the card width instead of
  being clipped.
- The build pipeline (`build/build.ps1`, `build/PerfWindow.iss`,
  `sensord/src/sensord.csproj`) now derives the application version from
  the single `version` field in `dashboard/Cargo.toml`.
- Default window size tightened to 1180×580, with a minimum height equal
  to the default height so the cards can never be clipped.

## [0.1.0] — 2026-05-19

### Added
- Real-time hardware dashboard for Windows: CPU, GPU, RAM, storage,
  motherboard sensors, fans, voltages and network throughput.
- Four themes (Amber Mainframe, Cyber Slate, Phosphor Tactical, Light) and
  an optional follow-Windows light/dark mode.
- Configurable refresh rate and temperature unit (°C / °F).
- `sensord` sensor backend built on LibreHardwareMonitorLib, shipped as a
  sibling executable.
- Inno Setup installer with a Windows Defender exclusion wizard page and a
  matching uninstaller that removes the exclusions, the `R0sensord` driver
  service, the install directory and the per-user data directory.

[Unreleased]: https://github.com/lukastojiljkovic/PerfWindow/compare/v0.4.1...HEAD
[0.4.1]: https://github.com/lukastojiljkovic/PerfWindow/releases/tag/v0.4.1
[0.4.0]: https://github.com/lukastojiljkovic/PerfWindow/releases/tag/v0.4.0
[0.3.0]: https://github.com/lukastojiljkovic/PerfWindow/releases/tag/v0.3.0
[0.2.6]: https://github.com/lukastojiljkovic/PerfWindow/releases/tag/v0.2.6
[0.2.5]: https://github.com/lukastojiljkovic/PerfWindow/releases/tag/v0.2.5
[0.2.4]: https://github.com/lukastojiljkovic/PerfWindow/releases/tag/v0.2.4
[0.2.3]: https://github.com/lukastojiljkovic/PerfWindow/releases/tag/v0.2.3
[0.2.2]: https://github.com/lukastojiljkovic/PerfWindow/releases/tag/v0.2.2
[0.2.1]: https://github.com/lukastojiljkovic/PerfWindow/releases/tag/v0.2.1
[0.2.0]: https://github.com/lukastojiljkovic/PerfWindow/releases/tag/v0.2.0
[0.1.0]: https://github.com/lukastojiljkovic/PerfWindow/releases/tag/v0.1.0
