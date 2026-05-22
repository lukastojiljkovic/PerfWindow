# Changelog

All notable changes to PerfWindow are documented in this file.

The format is based on [Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- In-app update detection that polls GitHub Releases on startup and surfaces
  a banner when a newer version is available, with a guided installer
  hand-off.
- `CHANGELOG.md` covering the release history from 0.1.0 onward.
- Settings → Updates section: opt-out toggle, manual "Check for updates now"
  action and last-checked timestamp.

### Changed
- The build pipeline (`build/build.ps1`, `build/PerfWindow.iss`,
  `sensord/src/sensord.csproj`) now derives the application version from the
  single `version` field in `dashboard/Cargo.toml`.

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

[Unreleased]: https://github.com/lukastojiljkovic/PerfWindow/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/lukastojiljkovic/PerfWindow/releases/tag/v0.1.0
