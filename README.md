# PerfWindow

A lightweight, open-source Windows hardware monitor — real-time **usage and
temperatures** for CPU, GPU, RAM, storage, motherboard, fans, voltages and
network throughput, in a single retro-utilitarian dashboard.

PerfWindow runs only while its window is open: no background service, no system
tray, no installer. It ships as one self-contained `PerfWindow.exe`.

## Architecture

PerfWindow is two processes shipped as a single file:

- **`sensord`** — a .NET 8 console process built on
  [LibreHardwareMonitorLib](https://github.com/LibreHardwareMonitor/LibreHardwareMonitor).
  It polls the hardware and writes sensor snapshots to standard output.
- **`PerfWindow.exe`** — the dashboard: a Rust application using
  [`eframe`/`egui`](https://github.com/emilk/egui). It renders the UI and owns
  the `sensord` lifecycle.

`sensord` is published as a self-contained executable and embedded into
`PerfWindow.exe` at build time (`include_bytes!`). At launch the dashboard
extracts `sensord` to a temporary location, spawns it as a child process, and
reads sensor snapshots over its stdout pipe. When the window closes, the child
is terminated and the extracted file removed — nothing is left running and
nothing is installed.

The full design specification is in
[`docs/superpowers/specs/2026-05-19-perfwindow-design.md`](docs/superpowers/specs/2026-05-19-perfwindow-design.md).

## Building

Prerequisites:

- The [.NET 8 SDK](https://dotnet.microsoft.com/download) (for `sensord`).
- A [Rust](https://www.rust-lang.org/tools/install) toolchain with the MSVC
  target, plus the Visual Studio Build Tools (the MSVC linker and resource
  compiler are required to embed the manifest and icon).

Build the release executable with the orchestration script:

```powershell
.\build\build.ps1
```

It publishes `sensord` (self-contained, `win-x64`), then runs
`cargo build --release` for the dashboard. Run it from a developer command
prompt, or otherwise ensure the MSVC environment is on `PATH` (for example by
sourcing `vcvars64.bat`), so the linker and resource compiler are available.

The result is `dashboard/target/release/PerfWindow.exe`. It is a large binary
(tens of megabytes) because the self-contained `sensord` runtime is embedded
inside it.

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

PerfWindow ships four themes:

- **Amber Mainframe** — amber-on-black, heavy CRT styling.
- **Cyber Slate** — teal accent on dark slate, subtle effects.
- **Phosphor Tactical** — green phosphor, heavy CRT styling.
- **Light** — a clean light theme with no CRT effects.

An optional "follow Windows" mode switches between a light and a dark theme
with the system setting.

## License

PerfWindow is released under the [MIT License](LICENSE).

Third-party components:

- **[LibreHardwareMonitorLib](https://github.com/LibreHardwareMonitor/LibreHardwareMonitor)**
  — hardware sensing, under the Mozilla Public License 2.0 (MPL-2.0).
- **Bundled fonts** — IBM Plex Mono, Chakra Petch and Space Mono, each under
  the SIL Open Font License 1.1 (OFL-1.1). The license texts are in
  `dashboard/assets/fonts/`.

Code signing of the released executable is a pending final step.
