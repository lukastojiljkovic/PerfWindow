# PerfWindow

[![CI](https://github.com/lukastojiljkovic/PerfWindow/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/lukastojiljkovic/PerfWindow/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/lukastojiljkovic/PerfWindow)](https://github.com/lukastojiljkovic/PerfWindow/releases/latest)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

A lightweight, open-source Windows hardware monitor — real-time **usage,
temperatures, power and lifetime metrics** for CPU, GPU (discrete and
integrated), RAM, storage, motherboard, fans, voltages, battery and network
throughput, in a single retro-utilitarian dashboard.

PerfWindow has no resident background process while its window is closed and
no system tray. It is installed by a standard Windows setup program and
removed cleanly through Add/Remove Programs.

## What it monitors

- **CPU** — load (overall and per-core or per-thread), package and per-core
  temperatures, current clock (with bus clock on hover), Vcore, RAPL power
  (Package, plus a Cores / DRAM / Platform breakdown on hover) and
  distance-to-TjMax for throttling headroom. On Intel hybrid CPUs P-Cores and
  E-Cores are visually separated in both the default strip view (P at full
  accent opacity, E dimmer, with a gap between the clusters) and the opt-in
  per-core heat-map (P / E border colours and corner tags; hovering a cell
  shows that core's current clock).
- **GPU** — discrete (NVIDIA / AMD) and integrated (Intel) on their own cards,
  with load, core temperature, GPU hot spot, GDDR memory junction temperature,
  core and memory clocks, video-engine (encode / decode) load, fan RPM, power
  draw, VRAM split (dedicated vs DXGI-shared on discretes;
  megabytes-of-system-memory-mapped on iGPUs), PCIe Rx/Tx throughput and core
  voltage. The integrated-GPU card hides rows the hardware does not populate,
  so it stays compact instead of showing a wall of em-dashes on machines
  whose iGPU exposes only a subset of sensors.
- **RAM** — used / free / cached, pagefile usage, and per-module DIMM
  temperature when the SPD hub exposes a thermal sensor (every DDR5 SO-DIMM,
  most DDR4 desktop kits). The hottest module shows on the card; hover for
  the per-module breakdown — vendor / part number, capacity, temperature and
  a DDR timing summary (e.g. "CL40-40-40-80 @ 5602 MT/s").
- **Storage** — per-drive temperature (coloured against the drive's own
  warning / critical thresholds when it reports them), activity, capacity,
  remaining health. Hover any drive row for SMART lifetime: power-on hours
  (with year/day breakdown), cold-start cycles, NVMe Available Spare, NVMe
  wear (Percentage Used) and total data written.
- **Motherboard / sensors** — board and VRM temperatures, fan RPMs, voltage
  rails when the Super-I/O chip is supported, plus a hardware-identity
  caption: motherboard model, BIOS version and date.
- **Network** — active adapter throughput (down / up) and link utilisation;
  on Wi-Fi, the connected SSID, signal quality and negotiated PHY rate.
- **Battery** — charge level, charge / discharge rate with direction arrow,
  estimated time remaining while on battery and battery health (Full / Design
  capacity).
- **Footer** — system uptime, sensor poll status, monitor model names with
  resolution and refresh rate, and a clickable version number that opens an
  in-app changelog viewer with proper markdown rendering.

Every stat row has a **hover tooltip** with a one-sentence plain-language
explanation, so the dashboard is readable even without prior hardware-monitoring
vocabulary.

## Installing

Download `PerfWindow-Setup.exe` from the [latest
release](https://github.com/lukastojiljkovic/PerfWindow/releases/latest) and
run it. PerfWindow installs into `C:\Program Files\PerfWindow`, adds a Start
Menu shortcut and registers an Add/Remove Programs entry. The installer
requests administrator rights so it can register the
`PerfWindowSensor` Windows Service (see *Architecture* below); the dashboard
itself runs as the current user from that point on.

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

- **`PerfWindow.exe`** — the dashboard: a Rust application using
  [`eframe`/`egui`](https://github.com/emilk/egui). Runs as the current user
  (no elevation manifest) and renders the UI.
- **`PerfWindowSensor`** — a Windows Service running `sensord.exe --service`
  as `LocalSystem`. Built on
  [LibreHardwareMonitorLib](https://github.com/LibreHardwareMonitor/LibreHardwareMonitor),
  it polls the hardware and emits NDJSON snapshots over a named pipe.

The installer registers the service with the SCM as **demand-start**
(admin-only). Each dashboard launch triggers a single UAC-elevated
`sc start PerfWindowSensor`; the service exits as soon as the dashboard
disconnects, so nothing remains resident while PerfWindow is closed. IPC is
the full-duplex named pipe `\\.\pipe\PerfWindowSensor` (single client,
NDJSON snapshots downstream, control messages upstream). `sensord` is
published self-contained, so no separate .NET runtime installation is
needed.

Sensor startup is staged: the service opens CPU and RAM sensors first — the
first readings reach the dashboard in roughly a second — then enables the
motherboard, GPU, storage, network, controller and battery categories
between snapshots, streaming progress messages the loading screen renders
as a per-category checklist. The phases before that ("Connecting to sensor
service" → "Windows will ask for permission" → "Starting sensor service")
keep their own status lines, with RETRY / EXIT actions if the service fails
to come up. The dashboard renders with OpenGL (glow) by default and falls
back to wgpu when OpenGL is unavailable at startup; if neither backend can
initialise, a message box points at the log instead of exiting silently.

`sensord` exposes two non-service modes for development and diagnostics:

- Run with **no arguments**, `sensord.exe` starts in console mode (NDJSON on
  stdout, control on stdin). This is what the dashboard's own `--dev` flag
  spawns as a child (`cargo run -p perfwindow -- --dev`), so you can work on
  the dashboard without installing the service. Note that, run as a normal
  user, this path cannot read the admin-only sensors (CPU MSR temperatures /
  clocks, NVMe SMART, so no Storage card) — those need the `LocalSystem`
  service.
- `sensord.exe --probe` dumps the full LibreHardwareMonitor hardware /
  sensor tree to stdout from an elevated prompt; useful for understanding
  which sensors are available on a given machine without any dashboard
  mapping in between.

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

`PerfWindow.exe` runs as the current user — its manifest is `asInvoker`, so
Windows never prompts to elevate the dashboard itself.

The elevation requirement was moved into the `PerfWindowSensor` Windows
Service (introduced in 0.8.0): the service runs as `LocalSystem` and is the
only thing that needs admin-level access to LibreHardwareMonitor's
kernel-mode sensors (PawnIO MSRs, NVMe SMART, Super-I/O voltages). On each
launch the dashboard issues one UAC prompt to authorise
`sc start PerfWindowSensor`; if you cancel it, the dashboard surfaces a
"service start was cancelled" message on the loading screen with RETRY /
EXIT actions instead of crashing or running in a degraded mode.

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
