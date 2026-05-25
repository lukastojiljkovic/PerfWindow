# Changelog

All notable changes to PerfWindow are documented in this file.

The format is based on [Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.6.1] — 2026-05-25

### Fixed
- **PawnIO chain-install on upgrade.** Updating PerfWindow from 0.5.x to
  0.6.0 over an already-installed PawnIO 2.2.0 caused the bundled
  `PawnIO_setup.exe -install -silent` to abort with Windows error 183
  (`ERROR_ALREADY_EXISTS`) and surface a "PawnIO driver installation
  failed" dialog. The installer never overwrote the existing registry
  entry, so the silent `-install` switch refused to proceed. The 0.6.1
  installer now runs `-uninstall -silent` first as a best-effort cleanup
  (no-op on fresh machines, ignored on failure) and then `-install
  -silent`, mirroring the upstream winget manifest's
  `UpgradeBehavior: uninstallPrevious` policy. Fresh installs and
  upgrades both end with PawnIO fully installed and CPU MSR readings
  populated.

## [0.6.0] — 2026-05-25

A **layout & data-gap** release. v0.5.x exposed every new sensor the
PawnIO-era LibreHardwareMonitor surfaces, but the dashboard had two
remaining issues: CPU clock never displayed on Intel hybrid CPUs (12th
gen+) and the GPU panel was so tall that it dictated row 1's height,
leaving CPU / iGPU / RAM as empty boxes. Row 2 also wasted half its
width while Storage flowed under it. This release fixes those and adds
DIMM temperature, GPU clock, P-Core / E-Core distinction in the default
heat strip, and an iGPU panel that hides the rows its hardware does not
populate.

### Fixed
- **CPU CLOCK now displays on Intel hybrid CPUs.** `BuildCpu` filtered
  `SensorType.Clock` sensors by `Name.StartsWith("CPU Core")`, which
  never matches the `"P-Core #N"` / `"E-Core #N"` names LHM emits on
  12th-gen and later Intel CPUs. The filter now takes every Clock
  reading except the chipset `Bus Speed` base, so `Max()` picks the
  highest boost clock across all cores regardless of vendor.

### Added
- **GPU / iGPU CLOCK row.** `sensord` already reported it; the panel
  did not render it. Now appears in the left stat column between
  JUNCTION and V (or wherever fits), formatted as MHz under 1 GHz and
  GHz at and above.
- **Per-module DIMM temperature.** `sensord` collects the `"DIMM #N"`
  Temperature sensors LHM exposes from each memory module (DDR5 SO-DIMM
  on the dev laptop, most DDR4 desktop kits with an SPD thermal sensor).
  The hottest reading shows as a `DIMM` or `DIMM MAX` row in the RAM
  panel; hovering reveals the full per-module breakdown.
- **iGPU VRAM shows shared MB amount.** Instead of the literal
  `"shared"`, the row now reads `"332 MB shared"` (or whatever DXGI
  reports), with the dedicated / shared breakdown on hover.
- **P-Core / E-Core distinction in the default heat strip.** The strip
  view (the non-heat-map default) now paints P-Cores at the standard
  accent opacity and E-Cores dimmer, with a small extra gap between
  the two clusters. Heat-map mode keeps its existing border / tag
  distinction.

### Changed
- **GPU panel uses a two-column stat layout** (TEMP / HOTSPOT / JUNCTION
  / CLOCK / V on the left, MEM USE / VRAM / PCIE / POWER on the right).
  The card height drops from ~440 px to ~270 px and stops dictating
  row 1, so CPU / iGPU / RAM no longer carry huge empty bands.
- **iGPU panel hides rows the hardware does not populate.** Intel UHD
  has no temperature sensor or memory-controller load reading and
  reports near-zero power; those rows now skip rather than render as
  em-dashes, so the card stays compact and informative.
- **Layout: row 2 fills evenly.** Storage now shares row 2 with
  Network and Battery (and Sensors when present) instead of wrapping
  to its own row and leaving 50 % of row 2 empty. On a 4-column grid
  this places Network · Battery · Storage (span 2) on the same row.
- `ram_panel` takes the active temperature unit so the new DIMM row
  honours the Settings `°C` / `°F` toggle.

### Removed
- The "iGPU panel of em-dashes" anti-pattern: every row that used to
  read `"—"` or `"0 W"` for an integrated GPU is now suppressed when
  the underlying sensor is absent.

## [0.5.1] — 2026-05-25

A "sensor expansion" release. PerfWindow now bundles the **PawnIO**
kernel driver (which `LibreHardwareMonitor` 0.9.5+ requires for MSR
access — see the *Fixed* section), then leverages everything the new
driver and library expose: hot-spot temps, lifetime metrics, PCIe
throughput, an Intel-iGPU panel, P-Core / E-Core distinction in the
heat-map, and **hover tooltips** with plain-language explanations on
every metric so a non-expert reader can tell `TjMax` apart from
`Junction` apart from `Vcore` without opening Wikipedia.

### Added
- **Intel integrated-GPU panel** mirroring the discrete-GPU card.
  Renders only when an iGPU is enumerated (most laptops, hybrid
  desktops), with Load, Clock, Power and Voltage where the IGCL
  telemetry path exposes them.
- **GPU Memory Junction temperature** stat row, paralleling Hot Spot.
  VRAM thermal headroom matters during VRAM-heavy work (large texture
  budgets, on-GPU inference, video encode).
- **CPU Distance-to-TjMax** stat row showing how many degrees the
  hottest core has before throttling kicks in. More intuitive than the
  absolute temperature: `ΔTjMax 12 °C` reads as "12 °C of headroom
  left".
- **CPU power breakdown** — Package power stays primary; secondary dim
  line shows Cores / DRAM / Platform separately when the MSRs expose
  them. Useful for battery-life analysis on laptops.
- **GPU PCIe throughput** (Rx / Tx, bytes per second) sparkline in the
  GPU panel. Surfaces bus saturation during game streaming, model
  loading or any sustained host↔GPU transfer.
- **GPU VRAM split** between dedicated and DXGI-shared memory in the
  VRAM bar. Quick read on how much an app is overflowing into system
  RAM (a common cause of stutter on laptops with small VRAM).
- **Per-storage Power-On Hours, cycle count and Available Spare** on a
  secondary line under each drive: `5057 h · 1103 cycles · spare 100 %`.
  Together with the existing Health %, they form a complete NVMe
  lifetime picture.
- **P-Core / E-Core distinction** in the CPU heat-map for Intel hybrid
  CPUs. P-Cores and E-Cores are visually separated and color-toned
  differently so a glance tells whether load is on the performance
  cluster or the efficiency cluster.
- **Hover tooltips** on every stat row, chip and bar across all
  panels. Hovering for half a second produces a one-sentence
  explanation in plain language — what the metric measures, what a
  typical range looks like, and what a high value implies. Sourced
  from a single central description table so terminology stays
  consistent.
- The installer bundles the official **PawnIO 2.2.0 setup** (signed by
  the upstream author, SHA-256 pinned in `build.ps1` for build
  reproducibility) and chain-installs it silently with the
  `-install -silent` switches taken from the official winget manifest.
  Re-running the installer is a no-op for users who already have
  PawnIO 2.2.0; older PawnIO installs are upgraded in-place by the
  PawnIO setup itself.

### Fixed
- **CPU temperature, clock and power readings are back.** Bumping
  `LibreHardwareMonitorLib` to 0.9.6 in 0.4.0 silently swapped the
  kernel driver from WinRing0 to [PawnIO](https://pawnio.eu), which
  PerfWindow's installer did not ship. The sensor backend kept
  reporting CPU load (a Windows API path) but everything that needs an
  MSR — temperature, per-core clocks, package power — came back as
  `null`. The 0.5.0 installer bundles `PawnIO_setup.exe` 2.2.0 and
  runs it silently as part of the install sequence.
- The dashboard's changelog viewer renders inline markdown spans
  (`**bold**`, `*italic*`, `` `code` ``, `[text](url)`) with proper
  weight, italics, monospace tint, and a clickable hyperlink. Earlier
  builds stripped these to plain text and the modal looked like
  unformatted output.

### Changed
- The `sensord` NDJSON schema gains new optional fields under
  `cpu`, `gpu[]`, `storage[]` and a new top-level `igpu` field for the
  Intel integrated GPU. Every addition is annotated
  `#[serde(default)]` so older `sensord` builds keep parsing without
  losing existing readings.
- The Windows Defender exclusions PerfWindow 0.4.1 and earlier
  registered for WinRing0 are obsolete (PawnIO is signed and Defender
  does not flag it) and are now removed on install and uninstall.
  Upgrades from 0.4.1 finish with no PerfWindow-owned Defender state.
- The installer no longer presents the "Configure Windows Defender for
  PerfWindow" wizard page — nothing needs to be configured any more.

### Removed
- The legacy `R0sensord` driver service is stopped and deleted on
  install and uninstall if a pre-0.5.1 build left it behind. PawnIO
  manages its own driver service under a different name.

### Dependencies
- `System.Management` 10.0.2 → 10.0.8 (sensord). Patch update; no
  behaviour change expected.
- `xunit` 2.9.2 → 2.9.3 (sensord tests). Patch update.

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

[Unreleased]: https://github.com/lukastojiljkovic/PerfWindow/compare/v0.6.1...HEAD
[0.6.1]: https://github.com/lukastojiljkovic/PerfWindow/releases/tag/v0.6.1
[0.6.0]: https://github.com/lukastojiljkovic/PerfWindow/releases/tag/v0.6.0
[0.5.1]: https://github.com/lukastojiljkovic/PerfWindow/releases/tag/v0.5.1
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
