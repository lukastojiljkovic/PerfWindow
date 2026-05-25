# PerfWindow

[![CI](https://github.com/lukastojiljkovic/PerfWindow/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/lukastojiljkovic/PerfWindow/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/lukastojiljkovic/PerfWindow)](https://github.com/lukastojiljkovic/PerfWindow/releases/latest)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

A lightweight, open-source Windows hardware monitor — real-time **usage,
temperatures, power and lifetime metrics** for CPU, GPU (discrete and
integrated), RAM, storage, motherboard, fans, voltages, battery and network
throughput, in a single retro-utilitarian dashboard.

PerfWindow runs only while its window is open — no background service and no
system tray. It is installed by a standard Windows setup program and removed
cleanly through Add/Remove Programs.

## What it monitors

- **CPU** — load (overall and per-core or per-thread), package and per-core
  temperatures, current clock, Vcore, RAPL power (Package, plus a Cores / DRAM
  / Platform breakdown on hover) and distance-to-TjMax for throttling headroom.
  On Intel hybrid CPUs P-Cores and E-Cores are visually separated in both the
  default strip view (P at full accent opacity, E dimmer, with a gap between
  the clusters) and the opt-in per-core heat-map (P / E border colours and
  corner tags).
- **GPU** — discrete (NVIDIA / AMD) and integrated (Intel) on their own cards,
  with load, core temperature, GPU hot spot, GDDR memory junction temperature,
  current clock, fan RPM, power draw, VRAM split (dedicated vs DXGI-shared
  on discretes; megabytes-of-system-memory-mapped on iGPUs), PCIe Rx/Tx
  throughput and core voltage. The integrated-GPU card hides rows the
  hardware does not populate, so it stays compact instead of showing a wall
  of em-dashes on machines whose iGPU exposes only a subset of sensors.
- **RAM** — used / free / cached, pagefile usage, and per-module DIMM
  temperature when the SPD hub exposes a thermal sensor (every DDR5 SO-DIMM,
  most DDR4 desktop kits). The hottest module shows on the card; hover for
  per-module breakdown.
- **Storage** — per-drive temperature, activity, capacity, remaining health.
  Hover any drive row for SMART lifetime: power-on hours (with year/day
  breakdown), cold-start cycles and NVMe Available Spare.
- **Motherboard / sensors** — board and VRM temperatures, fan RPMs, voltage
  rails when the Super-I/O chip is supported.
- **Network** — active adapter throughput (down / up) and link utilisation.
- **Battery** — charge level, charge / discharge rate with direction arrow,
  estimated time remaining while on battery and battery health (Full / Design
  capacity).
- **Footer** — system uptime, sensor poll status and a clickable version
  number that opens an in-app changelog viewer with proper markdown
  rendering.

Every stat row has a **hover tooltip** with a one-sentence plain-language
explanation, so the dashboard is readable even without prior hardware-monitoring
vocabulary.

## Installing

Download `PerfWindow-Setup.exe` from the [latest
release](https://github.com/lukastojiljkovic/PerfWindow/releases/latest) and
run it. PerfWindow installs into `C:\Program Files\PerfWindow`, adds a Start
Menu shortcut and registers an Add/Remove Programs entry. The installer
requests administrator rights.

The installer bundles **two third-party components** and chain-installs them
silently:

- **PawnIO** — a small kernel driver
  ([pawnio.eu](https://pawnio.eu)) PerfWindow uses to read MSR-backed CPU
  sensors (temperature, per-core clock, RAPL power). PawnIO is digitally signed
  by its author, so Windows Defender does not flag it. Skipped automatically
  when a current PawnIO install is already present.
- **Microsoft Visual C++ 2015-2022 Redistributable** — skipped automatically
  on machines that already have a 14.x runtime.

If PawnIO installation fails for any reason, PerfWindow still installs and
launches — only the CPU temperature, clock and power readings will be
unavailable until PawnIO is installed by other means.

To remove PerfWindow, use **Add/Remove Programs** or the Start Menu uninstall
shortcut. The uninstaller deletes every installed file and the per-user
configuration directory; PawnIO is left in place since other
LibreHardwareMonitor-based tools may rely on it.

## Updates

PerfWindow checks GitHub Releases for a newer version once at launch and
shows an in-window banner when one is available. The banner links to a
modal that downloads the installer and hands off to it; closing the
banner with **Later** suppresses it until the next launch. The check can
be disabled from **Settings → Updates**, where there is also a manual
**Check for updates now** action and the timestamp of the last check.

No telemetry is sent. The check is a single anonymous HTTPS request to
GitHub's public Releases API.

## Architecture

PerfWindow is two processes shipped as a single installer:

- **`sensord`** — a .NET 8 console process built on
  [LibreHardwareMonitorLib](https://github.com/LibreHardwareMonitor/LibreHardwareMonitor).
  It polls the hardware and writes sensor snapshots to standard output as
  NDJSON.
- **`PerfWindow.exe`** — the dashboard: a Rust application using
  [`eframe`/`egui`](https://github.com/emilk/egui). It renders the UI and owns
  the `sensord` lifecycle.

The installer places `PerfWindow.exe` and `sensord.exe` side by side in the
install directory. At launch the dashboard spawns the neighbouring
`sensord.exe` as a child process and reads sensor snapshots over its stdout
pipe; when the window closes, the child is terminated. `sensord` is published
self-contained, so no separate .NET runtime installation is needed.

`sensord` exposes a diagnostic mode — running `sensord.exe --probe` from an
elevated prompt dumps the full LibreHardwareMonitor hardware / sensor tree
to stdout, useful for understanding which sensors are available on a given
machine without any dashboard mapping in between.

## Building

Prerequisites:

- The [.NET 8 SDK](https://dotnet.microsoft.com/download) (for `sensord`).
- A [Rust](https://www.rust-lang.org/tools/install) toolchain with the MSVC
  target, plus the Visual Studio Build Tools (the MSVC linker and resource
  compiler are required to embed the manifest and icon).
- [Inno Setup 6](https://jrsoftware.org/isdl.php) for the installer
  (`winget install -e --id JRSoftware.InnoSetup`).

Build everything with the orchestration script:

```powershell
.\build\build.ps1
```

It publishes `sensord` (self-contained, `win-x64`), runs `cargo build
--release` for the dashboard, downloads the pinned PawnIO and Visual C++
Redistributable installers into `build/vendor/` (skipped if already
cached and SHA-256-checked), then compiles the installer with Inno Setup.
Run it from a developer command prompt, or otherwise ensure the MSVC
environment is on `PATH` (for example by sourcing `vcvars64.bat`).

The build produces `dashboard/target/release/PerfWindow.exe` with its
`sensord.exe` sibling, and the installer `dist/PerfWindow-Setup.exe`.

`build/make-icon.ps1` regenerates `dashboard/assets/PerfWindow.ico` from code;
it only needs to be run if the icon is changed.

## Administrator elevation

`PerfWindow.exe` carries an application manifest that requests
`requireAdministrator`, so Windows prompts for elevation at launch.

Elevation is required because LibreHardwareMonitor reads low-level sensors
(temperatures, voltages, fan speeds) through a kernel-mode driver and direct
hardware access. Those interfaces are not available to a standard-privilege
process; without administrator rights most temperature and voltage readings
would be unavailable.

## Themes

PerfWindow ships six themes, picked from **Settings → Theme**:

- **Amber Mainframe** — amber-on-black, heavy CRT styling.
- **Cyber Slate** — teal accent on dark slate, subtle effects.
- **Phosphor Tactical** — green phosphor, heavy CRT styling.
- **Synthwave** — magenta / cyan on indigo, neon glow.
- **Crimson** — red accent on near-black, restrained effects.
- **Light** — a clean light theme with no CRT effects.

An optional "follow Windows" mode pairs a light theme with a dark theme and
switches between them with the system setting.

A **pushpin chip** in the title bar (also exposed in Settings) toggles
*always on top*, so the dashboard can stay above other windows when desired.

## License

PerfWindow is released under the [MIT License](LICENSE).

Third-party components:

- **[LibreHardwareMonitorLib](https://github.com/LibreHardwareMonitor/LibreHardwareMonitor)**
  — hardware sensing, under the Mozilla Public License 2.0 (MPL-2.0).
- **[PawnIO](https://pawnio.eu)** — kernel driver bundled by the installer,
  digitally signed by namazso. Source available under an open-source
  licence (see the upstream project).
- **Bundled fonts** — IBM Plex Mono, Chakra Petch and Space Mono, each under
  the SIL Open Font License 1.1 (OFL-1.1). The license texts are in
  `dashboard/assets/fonts/`.

Code signing of the released PerfWindow binaries themselves is a pending
final step.
