# PerfWindow

A lightweight, open-source Windows hardware monitor — real-time **usage and
temperatures** for CPU, GPU, RAM, storage, motherboard, fans, voltages and
network throughput, in a single retro-utilitarian dashboard.

PerfWindow runs only while its window is open — no background service and no
system tray. It is installed by a standard Windows setup program and removed
cleanly through Add/Remove Programs.

## Installing

Run `PerfWindow-Setup.exe` and follow the wizard. PerfWindow installs into
`C:\Program Files\PerfWindow`, adds a Start Menu shortcut and registers an
Add/Remove Programs entry. The installer requests administrator rights.

One wizard page offers to add a **Windows Defender exclusion**. PerfWindow
reads CPU temperature, clock and power through the `WinRing0` kernel driver,
which is on Microsoft's vulnerable-driver list — Windows Defender quarantines
it on sight, which disables those readings. The exclusion tells Defender to
skip PerfWindow's own folders and the `sensord` process, and nothing else. It
is recommended but optional: leaving it unchecked simply means CPU temperature,
clock and power read as unavailable.

To remove PerfWindow, use **Add/Remove Programs** or the Start Menu uninstall
shortcut. The uninstaller deletes every installed file, the per-user
configuration, the `WinRing0` driver service and the Defender exclusions — the
machine is left as it was before installation.

## Architecture

PerfWindow is two processes shipped as a single file:

- **`sensord`** — a .NET 8 console process built on
  [LibreHardwareMonitorLib](https://github.com/LibreHardwareMonitor/LibreHardwareMonitor).
  It polls the hardware and writes sensor snapshots to standard output.
- **`PerfWindow.exe`** — the dashboard: a Rust application using
  [`eframe`/`egui`](https://github.com/emilk/egui). It renders the UI and owns
  the `sensord` lifecycle.

The installer places `PerfWindow.exe` and `sensord.exe` side by side in the
install directory. At launch the dashboard spawns the neighbouring
`sensord.exe` as a child process and reads sensor snapshots over its stdout
pipe; when the window closes, the child is terminated. `sensord` is published
self-contained, so no separate .NET runtime installation is needed.

The full design specification is in
[`docs/superpowers/specs/2026-05-19-perfwindow-design.md`](docs/superpowers/specs/2026-05-19-perfwindow-design.md).

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
--release` for the dashboard, then compiles the installer with Inno Setup. Run
it from a developer command prompt, or otherwise ensure the MSVC environment is
on `PATH` (for example by sourcing `vcvars64.bat`).

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

Code signing of the released binaries and the installer is a pending final step.
