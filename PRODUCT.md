# Product

<!-- impeccable:product-schema 1 -->

## Platform
windows

## Users

People who want to watch the hardware in a Windows PC — load, temperatures,
clocks, power, fan speeds, drive health and network throughput — and who do not
want a background service or a tray icon running when they are not looking.

## Product Purpose

Show every sensor PerfWindow can read, in one window, and leave nothing running
when that window is closed. It reads the hardware and reports it; it never
changes a clock, a voltage, a fan speed or any other hardware setting.

## Positioning

A single retro-utilitarian dashboard over LibreHardwareMonitor, with the
elevation that kernel-mode sensors need moved into a demand-start Windows
Service instead of the app itself. The dashboard runs as the current user, asks
Windows once per launch to start the service, and exits the service when it
disconnects. It ships six themes, hides a card row the hardware does not
populate instead of filling it with placeholders, and states plainly which
readings need the PawnIO driver.

## Operating Context

- Windows 11, or Windows 10 version 1607 or later, x64.
- Installed by a standard setup program into `C:\Program Files\PerfWindow`, with
  a Start Menu shortcut and an Add/Remove Programs entry.
- The installer requests administrator rights to register the
  `PerfWindowSensor` service and to add PawnIO and the Visual C++ runtime when
  they are missing. The dashboard itself runs as the current user.
- One UAC prompt per launch, for `sc start PerfWindowSensor`; cancelling it
  shows a Retry / Exit screen instead of failing silently.
- The installer is not code-signed yet, so SmartScreen may warn on first run.
- The only network request is the update check against GitHub's public Releases
  API, which can be turned off in Settings.

## Capabilities and Constraints

Capabilities, from the README's *What it monitors*:

- CPU load (overall, per-core or per-thread), package and per-core
  temperatures, current clock, Vcore, RAPL power and distance to TjMax. Intel
  P-cores and E-cores are separated in both the strip view and the heat-map.
- Discrete and integrated GPUs: load, core / hot-spot / memory-junction
  temperatures, clocks, video-engine load, fan RPM, power, VRAM split,
  PCIe throughput and core voltage.
- RAM: used, free, cached, pagefile, and per-module DIMM temperatures with a
  vendor / part-number / timing breakdown when the SPD hub reports them.
- Storage: per-drive temperature against the drive's own thresholds, activity,
  capacity, remaining health, and SMART lifetime (power-on hours, cold-start
  cycles, NVMe spare and wear, data written) on hover.
- Motherboard and sensors: board and VRM temperatures, fan RPMs, voltage rails
  when the Super-I/O chip is supported, and board model / BIOS version and date.
- Network: active adapter throughput and link use; SSID, signal quality and PHY
  rate on Wi-Fi.
- Battery: charge level, charge / discharge rate, time remaining and health.
- Footer: uptime, sensor poll status, monitor names with resolution and refresh
  rate, and a version number that opens an in-app changelog viewer.
- Refresh interval of 0.5, 1, 2 or 5 seconds; Celsius or Fahrenheit; six themes
  with an optional follow-Windows mode; always-on-top.

Constraints, from the README and the Terms of Use:

- Which sensors appear depends on the hardware, firmware and drivers; some
  readings may be missing or inaccurate.
- Without PawnIO, processor temperature, clock and power readings are
  unavailable; the app still installs and runs.
- It reads sensors only. It does not overclock, undervolt or control fans.
- `PerfWindow.exe` runs as the current user (`asInvoker`); the elevation is
  needed only to start the service that reads administrator-only sensors.
- x64 only.

## Brand Commitments

- Free and open source under the MIT License.
- No telemetry, accounts, analytics, crash reporting or ads. Sensor readings
  never leave the PC.
- Nothing runs in the background once the window is closed: no tray icon, and
  the sensor service exits with the dashboard.
- Plain language: every stat row carries a one-sentence tooltip, so the
  dashboard is readable without prior hardware-monitoring vocabulary.
- Honest limits: the app names the readings it cannot show, rather than filling
  the space with placeholders.
- The interface and the documentation are in English.

## Evidence on Hand

- Screenshots in [docs/images](docs/images/): `dashboard-light.png`,
  `dashboard-dark.png`, `settings-light.png`, `settings-dark.png`,
  `smart-light.png`, `smart-dark.png`.
- The product site under [site/](site/) (`index.html`, `site.css`, `llms.txt`),
  published to GitHub Pages by `.github/workflows/pages.yml` at
  https://lukastojiljkovic.github.io/PerfWindow/.
- Releases: `.github/workflows/release.yml` builds `PerfWindow-Setup.exe` and
  publishes `PerfWindow-Setup.exe.sha256`, which the in-app updater verifies
  before it runs the installer. `build/build.ps1` verifies the SHA-256 of each
  vendored component (PawnIO, the Visual C++ redistributable) before packaging.
- CI gates in `.github/workflows/ci.yml`: `cargo build`, `cargo fmt --check`,
  `cargo clippy -D warnings`, `cargo test`, `cargo audit`, the .NET tests, a
  .NET format check and an installer smoke build.
- The snapshot suite in `dashboard/tests/ui_snapshot.rs`: ten UI scenarios
  rendered in all six themes, with baselines under
  `dashboard/tests/snapshots/<theme>/`.
- [CHANGELOG.md](CHANGELOG.md), in Keep a Changelog format, embedded in the
  dashboard for the in-app changelog viewer.

Not on hand: no user counts or other usage metrics, no testimonials, no
performance benchmarks, and no compatibility list beyond the Windows versions
in the README. Do not fabricate them.

## Product Principles

- Read the hardware; never change it.
- Send nothing about the user's PC anywhere except the update check the user can
  turn off.
- Leave nothing resident when the window is closed.
- Say what each number means, and say when a number is missing.
- Verify before running: the updater accepts only this repository's release
  download URLs and checks the published SHA-256 before launching the installer.
- Prefer plain language over jargon in the interface.

## Accessibility & Inclusion

- A light theme, plus a follow-Windows mode that pairs a light and a dark theme
  and switches with the Windows setting.
- Every stat row has a hover tooltip that explains it in one sentence, so the
  dashboard does not assume the reader already knows the vocabulary.
- Readings are labelled, not colour-only: temperature and load colours track
  the same values the text shows.
- The interface is English only.
