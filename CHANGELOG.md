# Changelog

All notable changes to PerfWindow are documented in this file.

The format is based on [Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.2] — 2026-05-23

### Fixed
- The installer now detects the Microsoft Visual C++ 2015-2022 Redistributable
  on the target machine and installs it silently if missing. Previously the
  application could fail to launch on a machine without the runtime, with no
  visible error (the Windows PE loader exits the process before any of our
  code runs). The bundled redistributable is the official x64 build hosted by
  Microsoft.

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

[Unreleased]: https://github.com/lukastojiljkovic/PerfWindow/compare/v0.2.2...HEAD
[0.2.2]: https://github.com/lukastojiljkovic/PerfWindow/releases/tag/v0.2.2
[0.2.1]: https://github.com/lukastojiljkovic/PerfWindow/releases/tag/v0.2.1
[0.2.0]: https://github.com/lukastojiljkovic/PerfWindow/releases/tag/v0.2.0
[0.1.0]: https://github.com/lukastojiljkovic/PerfWindow/releases/tag/v0.1.0
