# AGENTS.md — PerfWindow

Rules for AI coding agents working on PerfWindow. Codex reads this file
directly, and Claude Code through `CLAUDE.md`, which imports it.

PerfWindow is a lightweight, open-source **Windows hardware monitor**: real-time
CPU, GPU, RAM, storage, motherboard, fan, voltage, battery and network readings
in one retro-utilitarian dashboard, a mini strip over the top of the screen, and
`perfwindow-cli` for terminals, scripts and AI agents. MIT licensed.

## Owner and git

- Luka Stojiljković (GitHub `lukastojiljkovic`) owns the repository. Every
  commit goes under his configured identity: never override it with
  `git -c user.*` or `--author`.
- No `Co-Authored-By` trailers and no "Generated with …" footers, in commits or
  pull requests.
- Changes reach `main` through pull requests. Don't push, open a pull request
  or merge without the owner's explicit go-ahead for that action.
- Conventional commits in English; stage explicit paths, never `git add .`.

## Architecture

Three programs from one installer:

- **`PerfWindow.exe`**, the dashboard: Rust with `eframe`/`egui`. It runs as the
  current user (`asInvoker` manifest) and leaves nothing resident. The renderer
  is glow (OpenGL), with wgpu as a logged fallback when glow fails at startup,
  and a native MessageBox when both fail, so it never exits silently.
- **`perfwindow-cli.exe`**: the same readings in a terminal. It opens a ratatui
  live dashboard on an interactive terminal, and otherwise prints an ASCII
  report or `--json`. It is off until Settings → Command line turns it on
  (`cli_enabled`), which also puts the install folder on the user `PATH`.
- **`PerfWindowSensor`**: a Windows service running `sensord.exe --service` as
  `LocalSystem`, from a folder publish next to the dashboard. It is
  demand-start and admin-only: each launch asks once, through UAC, to
  `sc start` it, and it exits as soon as its client disconnects.

IPC is a full-duplex named pipe, `\\.\pipe\PerfWindowSensor`, carrying NDJSON
to a single client, so the window and the CLI can't read at the same time.

- **Connecting** (`ipc/connect.rs`) walks `OpeningPipe → RequestingElevation →
  StartingService → LoadingSensors → Ready/Failed`. The window and the CLI share
  this path.
- **Sensor init is staged.** The control reader starts as soon as the client
  attaches. CPU and RAM sensors open first (the first snapshot comes in about a
  second), and the other LibreHardwareMonitor categories are enabled between
  snapshots, while `{"progress":{…}}` lines drive the loading checklist.
- **Unknown lines are ignored.** Progress lines are deliberately not
  snapshot-shaped, and both sides must always ignore a line they don't know.

`PerfWindow.exe --dev` spawns `sensord.exe` as a console child (NDJSON on
stdout) instead of using the service; `sensord.exe --probe` dumps the LHM sensor
tree. Both bypass the service.

## Layout

- **`dashboard/`**: the Rust crate (lib `perfwindow`, bins `PerfWindow` and
  `perfwindow-cli`).
  - `src/`: `app.rs` (state and dispatch), `ipc/`, `ui/`, `panels/`, `widgets/`,
    `theme/`, `update/`, `config.rs`, `strip_window.rs` (the mini strip's
    viewport), and `cli/` (`report.rs`, `term.rs`, the live dashboard in
    `live/`).
  - `tests/`: egui_kittest snapshots per theme, layout fit, headless render
    stress, and pipe integration.
- **`sensord/`**: the .NET 8 solution.
  - `src/`: `Program.cs` (mode dispatch), `Service/` (the service worker and the
    named-pipe server), `Sensors/` (the LHM wrapper and the ATK, display, disk
    and pagefile readers), `Model/` (the NDJSON schema).
  - `tests/`: xUnit.
- **`build/`**: `build.ps1` (the full build), `PerfWindow.iss` (Inno Setup),
  and `vendor/` (cached PawnIO and VC++ redistributable downloads).
- **`site/`**: the GitHub Pages site, including `llms.txt`.
- **`CHANGELOG.md`**: Keep a Changelog 1.1.0, embedded in the dashboard for the
  in-app changelog viewer.

## Tech stack

- **Rust**, edition 2021, MSVC target: `eframe`/`egui` 0.34, and `ratatui` for
  the CLI dashboard.
- **.NET 8**, as a self-contained `win-x64` publish.
- **LibreHardwareMonitorLib** for the sensors, and the signed **PawnIO** driver
  for MSR access, which the installer bundles.
- **Inno Setup 6** for the installer.

## Build and test

```powershell
.\build\build.ps1                                   # full build: dist\PerfWindow-Setup.exe
cargo build --release --manifest-path dashboard\Cargo.toml
cargo test --release --manifest-path dashboard\Cargo.toml -- --test-threads=1
dotnet test sensord\sensord.sln
```

- **One test thread.** The UI snapshot tests share one wgpu context.
- **Re-capturing snapshots:** set `UPDATE_SNAPSHOTS=1` for that run, then look
  at every image that changed.
- **The integration tests run a copied sensord.** They use
  `target\<profile>\sensord.exe`, which `build.rs` copies from sensord's
  publish folder. After touching sensord, run
  `dotnet publish sensord/src -c Release -r win-x64 --self-contained` again, or
  the tests exercise a stale service.
- **CI gates every pull request:**
  - `cargo fmt --check`, `cargo clippy --release -- -D warnings`, the Rust
    tests, and `cargo audit`;
  - the .NET tests and `dotnet format`;
  - an installer smoke build. Run them all before pushing.
- **Releasing.** `build.ps1` reads the version from `[package] version` in
  `dashboard/Cargo.toml`. A release bumps that, the lockfile and the bug report
  template's placeholder, dates the CHANGELOG section (its lead paragraph on one
  line), and re-captures the snapshots that print the version. Pushing the tag
  `v<version>` publishes the installer and its `.sha256`.

## Conventions

- **Polish matters.** Visuals, copy and behaviour should be coherent and
  production-quality.
- **English** for everything in the repository and on the site.
- **No invented numbers.** A number in a README, on the site or in the
  CHANGELOG is one that was measured.
- **Comments say why, not what,** and only when it isn't obvious. When you
  rewrite a section, sweep its old comments with it.
- **Installer comments.** Inno `[Code]` comments don't nest: a constant such
  as `{app}` inside `{ … }` ends the comment early.

## Pointers

- **The `Status` enum** (`app.rs`) drives every render decision. `ingest()`
  takes snapshots with `take()`, and moves `SensordDown → Running` when a
  snapshot arrives over a live feed. Every demotion deadline is
  silence-based: the startup watchdog stays armed until the first FULL
  snapshot, and a loaded feed with no line for 30 s (`FEED_SILENCE`) drops to
  NO SIGNAL. Never reintroduce a fixed first-snapshot deadline, because slow
  LHM enumeration is legitimate. `ingest()` runs from `App::logic`, which
  eframe calls even while the window is minimized; `ui()` is skipped then.
- **Control messages** (interval, shutdown) go through a writer thread
  (`ipc/mod.rs`). Never do pipe I/O on the UI thread.
- **Card layout** uses a per-card capacity model (`ui/capacity.rs`,
  `ui/stat_priority.rs`), so rows drop gracefully when a card is narrow.
- **Themes.** The palettes and fonts are in `theme/mod.rs`: six themes (Amber,
  Slate, Phosphor, Light, Synthwave, Crimson). The CLI dashboard draws in the
  same palette.
- **Warning colours.** The warn and hot thresholds live in shared helpers
  (`widgets::Severity`, `temp_severity`, `health_severity`). The window and the
  CLI both use them; never copy a threshold.
- **Updates** poll GitHub Releases (`update/`), cached in `%APPDATA%`. Download
  attempts are generation-counted, each with its own cancel state, and the
  installer is SHA-256 checked against the release's `.sha256`.
- **The pipe** (`ipc/pipe.rs`) is opened **blocking** and read synchronously
  through `BufReader::lines()`. Never use `FILE_FLAG_OVERLAPPED` there: a
  synchronous read on an overlapped handle returns `ERROR_IO_PENDING`, which
  Rust turns into an abort (`0xc0000409`, with no panic-hook entry). That was
  the 0.9.1–0.9.4 startup crash. Synchronous I/O on one handle is serialized,
  so a control write waits behind the reader's pending `ReadFile`:
  `shutdown()` cancels that read with `CancelIoEx` through a third cloned
  handle. Connecting retries a busy pipe for 4 s, because the old instance
  answers busy while sensord ends its previous session.
- **Per-core arrays are per core and parallel.** `cores`, `core_clocks_mhz`
  and `core_temps` hold one entry per physical core in one order (P-cores
  first on hybrid Intel). A core's load is the mean of its threads' loads,
  because LHM reports load per thread (`SnapshotBuilder.CollectPerCore`).
- **sensord guards every value.** Each sensor read goes through finite-guards
  (`SensorEx`): a NaN or Infinity reaching `JsonSerializer` kills every
  snapshot, and serde_json rejects NaN literals on the other side. Likewise
  `UpdateVisitor` catches each device's `Update()` failure and logs it once,
  because LHM's traversal has no guard and one throwing device stopped them
  all.
- **The CLI reads untrusted text.** It strips control characters from every
  string that comes from the sensors or Windows (SSIDs, device names) before
  printing, so a network name can't inject escape sequences or fake lines.
