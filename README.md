# PerfWindow

A lightweight, open-source Windows hardware monitor — real-time **usage and
temperatures** for CPU, GPU, RAM, storage, motherboard, fans, voltages and
network throughput, in a single retro-utilitarian dashboard.

> **Status: design phase.** The design specification is complete; implementation
> has not started. See
> [`docs/superpowers/specs/2026-05-19-perfwindow-design.md`](docs/superpowers/specs/2026-05-19-perfwindow-design.md).

## Planned highlights

- **Zero idle footprint** — runs only while its window is open. No background
  process, no service, no system tray.
- **Single self-contained `.exe`** — no installer.
- **Four themes** — three retro (Amber Mainframe, Cyber Slate, Phosphor
  Tactical) plus a clean Light theme, with an optional "follow Windows" mode.
- **Live dashboard** — gauges, sparklines and bars; °C/°F; configurable refresh.

## Architecture (planned)

A Rust + [`egui`](https://github.com/emilk/egui) dashboard plus an embedded
.NET sensor process built on
[LibreHardwareMonitorLib](https://github.com/LibreHardwareMonitor/LibreHardwareMonitor).
The two ship as one file; the sensor process is extracted at launch and removed
on exit. Full detail is in the design specification linked above.

Reading temperatures requires administrator rights (a kernel driver), so the
app requests elevation at launch.

## License

[MIT](LICENSE). Hardware sensing uses LibreHardwareMonitorLib (MPL-2.0); bundled
fonts (IBM Plex Mono, Chakra Petch, Space Mono) are under the SIL Open Font
License.
