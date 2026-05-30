# Changelog

All notable changes to PerfWindow are documented in this file.

The format is based on [Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.9.5] — 2026-05-30

The release that **identifies and fixes** the startup crash 0.9.1–0.9.4 chased
with ever-deeper diagnostics. The fault was a Rust runtime `abort()` — neither
a `panic!` nor a native exception — which is exactly why neither the
`panic.log` hook (0.9.3) nor the SEH filter (0.9.4) ever caught it.

### Fixed

- **Dashboard no longer aborts a few seconds into every session
  (`0xc0000409` `STATUS_STACK_BUFFER_OVERRUN`).** The named-pipe client
  (`ipc/pipe.rs`) opened the pipe with `FILE_FLAG_OVERLAPPED` but then read it
  **synchronously** through `BufReader::lines()` (and wrote with blocking
  `writeln!`). On Windows a synchronous `ReadFile` against an overlapped handle
  can return `ERROR_IO_PENDING`; the Rust standard library treats that as
  unrecoverable and calls `abort()` — *"fatal runtime error: I/O error:
  operation failed to complete synchronously"*. The abort lowers to a Win32
  `__fastfail`, which bypasses **both** the Rust panic hook and
  `SetUnhandledExceptionFilter`, so it left no entry in `panic.log` — the dead
  end every prior diagnostic release hit. The reads now use a default
  **synchronous (blocking)** handle, which simply waits for data; the flag was
  never needed, since nothing in the client uses overlapped / IOCP I/O. The
  `--dev` child-process path never tripped this (its stdout is an ordinary
  synchronous pipe), which is why the crash only ever reproduced through the
  installed service. This also resolves the "Loading sensors… then crashes
  halfway" symptom: the reader thread was aborting the whole process mid-load.

### Added

- **Deterministic regression guard**
  (`tests/pipe_integration.rs::pipe_client_blocks_on_empty_pipe_without_aborting`):
  stands up a named-pipe server, connects the real client and delivers the first
  snapshot only after a delay, forcing the reader to block on an empty pipe —
  the exact timing that aborted the shipped builds. If the pipe is ever reopened
  overlapped, the reader aborts and fails the test binary outright.
- **Headless full-UI render harness** (`tests/render_stress.rs`): renders every
  panel across both temperature units, the heat-map, all six themes, several
  viewport sizes and DPI scales, fed realistic *and* adversarial snapshots
  (NaN / infinity / huge / empty / inconsistent core counts), running egui
  layout + tessellation on the CPU with no GPU. Built to rule the panel/widget
  layer out as the crash site; kept as a degenerate-geometry regression guard.

### Changed

- **Clean release profile restored** (`lto = true`, `strip = true`, `debug`
  off). The 0.9.4 diagnostic profile that kept symbols and disabled LTO has
  served its purpose now that the crash is identified and fixed.

## [0.9.4] — 2026-05-28

A **diagnostic** release for the still-unidentified dashboard startup crash
(0xc0000409 `STATUS_STACK_BUFFER_OVERRUN`) that 0.9.3's panic-to-file hook
failed to catch — confirming the crash is a Win32 structured exception
rather than a Rust `panic!`. This build adds the missing piece (a SEH
handler), a launch-marker, and keeps debug symbols in the release binary so
the faulting address actually means something.

### Added

- **Win32 SEH unhandled-exception filter** (`SetUnhandledExceptionFilter`)
  writes a `panic.log` entry with the exception code (named, e.g.
  `STACK_BUFFER_OVERRUN (fastfail)`), the faulting absolute address, the
  exception parameters and a backtrace. Complementary to the Rust panic
  hook (which only covers `panic!()` / `assert!()` / `unreachable!()`),
  this catches everything that bypasses the Rust panic infrastructure:
  fastfail aborts from inlined wgpu/eframe, MSVC `/GS` stack-canary
  failures, access violations, stack overflows, etc.
- **Process-start marker** appended to `panic.log` on every launch
  (version, timestamp, pid), so a postmortem can distinguish "binary
  never launched" from "binary launched then crashed", and so a crash
  entry can be matched to the launch that produced it.

### Changed

- **Release profile keeps debug info** (`debug = true`, `lto = false`,
  `strip = false`). The 0.9.3 release was LTO-merged and stripped, which
  reduced `PerfWindow.exe` size but made absolute crash addresses
  un-resolvable — fine for stable releases, fatal for triaging an opaque
  fastfail. The clean release profile will return as soon as the
  underlying crash is identified.

## [0.9.3] — 2026-05-28

A **crash diagnostics + hardening** hotfix. 0.9.1 and 0.9.2 were crashing on
some machines with an opaque `STATUS_STACK_BUFFER_OVERRUN` (0xc0000409) in
Event Log a few seconds after launch and no other breadcrumb. Without a
panic message we could only narrow the cause from a code review, so this
release adds an in-process panic log, defensive guards in the widget layer
against the likely-degenerate-Rect class of failure, and fixes a
service-side crash that was firing on every clean shutdown.

### Added

- **`%APPDATA%\PerfWindow\panic.log`**: every Rust `panic!` now appends a
  timestamped entry (location, message, full backtrace) before the process
  tears down. The previous behaviour — `windows_subsystem = "windows"`
  swallowing stderr and the OS reporting only an opaque fault offset — made
  a postmortem essentially impossible. Stack-buffer-overrun aborts that
  bypass `panic!` won't reach the log, but a double-panic *will* leave at
  least the inner panic message before the outer `abort()`.

### Fixed

- **Sensor service no longer crashes on every clean shutdown.**
  `ServeOne`'s writer was disposed via `using var`, which synchronously
  flushes — and flushing a closed pipe (the normal path when the dashboard
  exits) throws `IOException: Pipe is broken`. The exception escaped
  `ExecuteAsync` and surfaced as "BackgroundService failed" in Event Log
  every single time. Dispose is now manual with that flush failure
  swallowed. Defensive try/catch wrapped around `pipe.DisposeAsync()` and
  `HardwareMonitor.Dispose()` in the same `finally` block.
- **Topology-change monitor reconstruction is now retried-on-fail instead
  of swap-then-lose.** Previously a failing `new HardwareMonitor()` after a
  topology change would still go through `monitorRef.Monitor = newMonitor`
  before the throw, losing the previous (working) monitor for the rest of
  the session. The new monitor is held in a local first; only if its
  construction succeeds does the swap-and-dispose happen.
- **Settings → refresh-rate change no longer silently fails on a broken
  pipe.** `PipeSensord::set_interval` was `let _ = writeln!(...)` and the
  user got unchanged poll cadence with no signal. The write now drops the
  writer on `BrokenPipe`/I/O error and flips the shared `alive` flag so the
  next `ingest()` pass promotes `Status` to `SensordDown` and the error
  overlay is reachable.
- **Defensive NaN / negative-width guards in `widgets/sparkline.rs`,
  `widgets/bars.rs`, `widgets/stat.rs`.** Each widget's
  `allocate_exact_size` now rejects non-finite or non-positive widths
  *before* they reach the egui tessellator, which on Windows MSVC
  release-LTO builds can `__fastfail` (STATUS_STACK_BUFFER_OVERRUN) on a
  degenerate `Rect` instead of producing a recoverable Rust panic. This is
  the most plausible class of failure behind the 0.9.1/0.9.2 startup
  crashes; the panic log added in this release will confirm or rule it out
  on the next reproduction.
- **`Running` state with no first snapshot no longer hangs the loading
  screen forever.** When the connect machine emits `Ready` but the first
  NDJSON line never arrives (rare: sensord dies between accept and first
  write), the dashboard now demotes to `SensordDown` after 30 s so the
  RESPAWN overlay becomes reachable.

### Changed

- **Loading screen's RETRY / EXIT row stacks vertically on sub-240 px
  windows** instead of overflowing the available width. Replaces the
  previous "clamp the cursor offset to 0" workaround which still let the
  two buttons sit on one row when they didn't fit.
- **Diagnostic `sensord.exe --probe` scans for the PawnIO service** in
  addition to WinRing0 / SystemTemp paths. Older builds only knew about
  WinRing0 even though LHM 0.9.5+ has used PawnIO since 0.5.1.

### Internal

- **`.vscode/` added to `.gitignore`** (was already implied for `.vs/`,
  `.idea/`).
- **Test `apply_config_change_updates_*` renamed to
  `config_*_field_is_writable`** — the previous names implied the tests
  exercised `apply_config_change`, which they did not (they only assert
  the underlying `Config` field round-trips through the `&mut` borrow).
- **Stripped historic `v0.8.x` references** from `panels/battery.rs`,
  `panels/sensors.rs`, `panels/storage.rs`, `ui/capacity.rs`,
  `ui/stat_priority.rs`, `ipc/pipe.rs`, `theme/mod.rs` test, and
  `sensord/Sensors/DisplayReader.cs`. The version pins were not pulling
  weight beyond the history they implied; CHANGELOG remains the
  authoritative source.
- Doc fixes: `pipe::connect()` is documented as the production respawn
  path (not "retained for tests only" — it's called by
  `PerfApp::respawn_sensord` from the RESPAWN button on the sensord-down
  overlay); `MonitorRef` nullable rationale added.

## [0.9.2] — 2026-05-28

A **stability** hotfix focused on the dashboard-startup window and a class of
panic-induced crashes that left the UI frozen on "Loading sensors…".

### Fixed

- **"Loading sensors…" no longer times out on the first launch after a
  PawnIO install.** `SensorPipeWorker` previously did its full
  `HardwareMonitor.Open` + topology probe *before* creating the named
  pipe, so a slow LHM enumeration (several seconds on a cold start) blew
  the dashboard's connect deadline and the screen surfaced "service did
  not start in time". The pipe is now created first; HardwareMonitor
  initialises *after* the client has attached and the dashboard is
  already showing the loading screen.
- **`HardwareMonitor.Open` failures now surface as the dashboard's
  health banner** instead of a silent service exit. The worker emits a
  single `pawnio: "denied"` snapshot carrying the underlying exception
  message before exiting, so the user gets a concrete reason ("PawnIO
  access denied", quarantined driver binary, etc.) rather than a blank
  reconnect loop.
- **Dashboard no longer panics when the OS refuses to clone the pipe
  handle.** `start_reader`'s `try_clone().expect(…)` was on the
  connect hot path; a transient `DuplicateHandle` failure (kernel
  handle exhaustion, ACL race, peer disconnect mid-handshake) panicked
  the connect worker thread and froze the UI on "Loading sensors…"
  until the receiver hung up. The clone now propagates as
  `ConnectError::Io` and surfaces on the loading screen's Failed state
  with RETRY / EXIT actions.
- **`--dev` child-spawn dashboard shutdown no longer hangs.** The
  `Sensord::Child` `Drop` impl was joining the reader thread, which
  blocks in `BufReader::lines()` while the child's stdout handle is
  still open — the same bug class the pipe path fixed in 0.8.1. The
  reader is now detached on drop, mirroring the pipe path.
- **Loading-screen action buttons no longer produce non-finite layout
  rects on sub-200 px-wide windows.** The Retry / Exit button row's
  horizontal offset was computed as `available_width / 2 − 100`, which
  could go negative; it is now clamped to zero.
- **Loading screen is shown for the full startup window**, including
  the gap between the connect machine emitting `Ready` and the first
  NDJSON snapshot arriving. Previously a brief "waiting for sensord…"
  placeholder appeared in that window, which on slow machines lasted
  several seconds and read like an outright stall.

### Internal

- Removed dead `service_dialog_open` / `service_dialog_message` /
  `service_starting` fields from `PerfApp` (leftovers from the 0.8 → 0.9
  loading-screen migration).
- Architecture sections of `README.md` rewritten to match the
  post-0.8.0 Windows Service model — the previous wording still
  described the v0.7.x child-process IPC and a `requireAdministrator`
  dashboard manifest.
- Stripped dead `(mockup …)` parenthetical references from doc
  comments across `widgets/`, `panels/`, `ui/` and `format.rs`; the
  `docs/mockups/*.html` files they linked to had been removed.
- Several other comment fixes (six-card theme picker, no more T-tag
  workflow markers, accurate IPC descriptions in `pipe.rs` /
  `process.rs`).

## [0.9.1] — 2026-05-28

### Fixed

- **Display resolution and refresh rate now match the Windows Display panel
  on high-DPI screens.** Sensord opts in to Per-Monitor-Aware (V2) at
  process startup; previously Win32 returned virtualised modes (commonly
  1024x768 @ 60Hz instead of the panel's native mode) because only the
  dashboard process carried a DPI manifest.
- **Loading screen now renders during the 5-15 s service startup window.**
  `ingest()` no longer clobbers `Status::Connecting` to `SensordDown` on
  every frame, and `card_grid` routes the connecting state to the
  loading screen (with phase text and a retry/exit action on failure)
  instead of the misleading "sensor feed stopped" overlay. The footer
  shows `BOOTING` instead of `NO SIGNAL` during this window.
- **Restored GPU JUNCTION (VRAM memory die), PCIE throughput and core
  voltage readings.** The Full-tier capacity row budget was raised from
  6 to 9 so every GPU candidate fits at the default viewport.
- **Restored battery RATE row** (directional arrow + watts) as a dedicated
  candidate, separated from the STATE row.
- **Battery TIME row** is now always present (em-dash placeholder when the
  OS has no estimate) instead of vanishing while charging or idle.
- **Battery HEALTH** is reported again as remaining-life percentage (was
  inverted to WEAR in 0.9.0).
- **Storage TEMP column** is now drawn at every capacity tier; the
  previous single-column gate hid it in narrow windows.
- **Sensors panel ranks readings by category** (temperatures, fans,
  voltages) before truncating, so voltage rows no longer all fall off
  the card on machines with many fans.
- **Settings modal no longer panics when the update-check mutex is
  poisoned.** Every `update_state.lock().unwrap()` in the modal, banner
  and update modal recovers via `into_inner()` so a background-thread
  panic degrades to a "never" timestamp rather than crashing the UI.
- **Rapid theme switching no longer accumulates style allocations.**
  `Theme::apply` now calls `Context::set_theme` before `set_visuals` so
  the new palette lands in the correct style slot instead of cloning
  the `Arc<Style>` on every switch.

### Added

- **Tooltip coverage** for MEM USE, DIMM, DIMM MAX, IFACE, REMAINING,
  BOARD and VRM labels so every visible stat row has hover text.

## [0.9.0] — 2026-05-27

### Fixed

- **Dashboard crash on close.** `eframe::App::on_exit` now sends a
  `{"shutdown":true}` control message over the pipe before the dashboard
  process tears down. `Drop` alone was unreliable during shutdown; the
  explicit close path runs while the egui frame is still alive.
- **Service won't restart after closing.** The worker now exits on client
  disconnect and the dashboard re-launches it on the next start. Every
  launch follows the same path regardless of session history; the silent
  failure observed in 0.8.1 (where the sc-grant ACL was inconsistently
  applied) is gone.
- **Installer cannot be blocked by a running PerfWindow instance.**
  `ForceCloseApplications=yes` terminates stubborn processes during
  upgrade. The uninstaller also taskkills any orphaned `sensord.exe`.
- **Display resolution and refresh rate match the Windows Display panel.**
  The dashboard manifest is now PerMonitorV2 DPI-aware, so Win32 no
  longer DPI-virtualises the display modes it reports to the process.
- **Hardware topology changes mid-session** (e.g. a dGPU power-gating
  on/off battery, a USB drive being unplugged) are detected every five
  poll ticks. The sensor monitor is recreated transparently; the
  dashboard sees the new device on the next snapshot.
- **Per-sensor and per-hardware exceptions in the snapshot builder are
  isolated.** A single failing reading drops only that section to `null`
  instead of aborting the whole snapshot.

### Changed

- **Service lifecycle: per-launch elevated start.** The custom SDDL grant
  added in 0.8.1 is removed; the service reverts to default Windows
  permissions (admin-only start). Each dashboard launch issues one UAC
  prompt to start the service, surfaced via a phase-aware loading
  screen ("Connecting to sensor service" / "Windows will ask for
  permission" / "Starting sensor service" / "Loading sensors"). The
  installer no longer starts the service post-install.
- **No background sensord.** The 60-second post-disconnect idle window
  is removed; the worker exits immediately when the dashboard
  disconnects. `sensord.exe` is never resident while the app is closed.
- **Window minimum size lowered to 720x500** (was 960x600).
- **Panel rendering: priority-ranked progressive disclosure.** The
  binary "Compact" mode is replaced by a numeric capacity model. Each
  panel publishes a ranked list of stat candidates; the layout selects
  as many as the card's allocated width affords. Smaller windows drop
  low-priority readings (HOTSPOT, JUNCTION, V, PCIE, etc.) first;
  primary readings (TEMP, VRAM used/total, USED/FREE) are preserved
  at every tier.
- **Multi-monitor display info.** Sensord now enumerates all attached
  monitors via `EnumDisplayMonitors`; the snapshot carries the full
  list (primary first) in addition to the existing single-display
  field for backward compatibility.

### Removed

- The "Sensor service is not running" modal. Replaced by the loading
  screen.

## [0.8.1] — 2026-05-27

A **hotfix** release.

### Fixed

- **Hang on close** when shutting down the dashboard. The pipe client's
  `Drop` impl was joining the reader thread, which was blocked reading
  the pipe with its own file handle keeping the pipe open — the join
  would never return. Process exit hung; Windows would force-kill it
  after a few seconds and the user saw a "Not Responding" / crash. The
  reader thread is now detached on drop; the OS reaps it on process
  exit and the server detects the disconnect from its own side.

### Changed

- **Sensor service no longer runs in the background when PerfWindow is
  closed.** v0.8.0 left `PerfWindowSensor` running 24/7 as
  LocalSystem (idle, ~15 MB RAM, 0 % CPU). v0.8.1 switches the service
  to demand-start with three small changes:
  - Installer grants `Authenticated Users` the `SERVICE_START` and
    `SERVICE_STOP` permissions via SDDL, so the non-elevated dashboard
    can manage the service lifecycle without UAC.
  - Dashboard silently runs `sc start PerfWindowSensor` on launch
    (idempotent; if the service is already running, sc returns 1056
    and we proceed straight to the pipe open).
  - The worker auto-stops 60 s after the last client disconnects.
  Net effect: opening the dashboard brings the service up tiny-amount
  faster than v0.7.0 felt; closing the dashboard reliably tears it
  down. No more sensord process visible in Task Manager when PerfWindow
  is closed.

## [0.8.0] — 2026-05-27

A **service architecture** release: `sensord` moves out of a child process
into a Windows Service (`PerfWindowSensor`, runs as `LocalSystem`). The
dashboard drops the `requireAdministrator` manifest and runs as the
current user, fixing `ERROR_ELEVATION_REQUIRED` (Win32 740) on
environments that silently deny UAC elevation. Hardware-reading
capability is unchanged; the install/launch UX gains one UAC prompt at
install time and zero per launch from then on.

### Architecture

- **Sensor backend now a Windows Service** (`PerfWindowSensor`) running
  as `LocalSystem`. Dashboard runs as the current user (`asInvoker`
  manifest), so every subsequent launch is UAC-free.
- IPC moved from child-process stdin/stdout to a named pipe
  (`\\.\pipe\PerfWindowSensor`, full-duplex NDJSON, single-client,
  ACL'd to Authenticated Users).

### Added

- Service-side **`health` payload** in every snapshot
  (`health.pawnio = ok | missing | denied`).
- Dashboard **health banner** when the service reports degraded mode,
  with **Install PawnIO** (opens https://pawnio.eu) and **Dismiss**
  actions, styled to match the existing update banner.
- Dashboard **"Sensor service is not running" modal** with a **Start**
  button that calls `sc start PerfWindowSensor` via UAC elevation. One
  UAC prompt instead of one per launch.
- Installer **creates and starts** `PerfWindowSensor` automatically
  post-install (`sc create … obj= LocalSystem start= auto`).
- Uninstaller **stops and deletes** the service, then **asks** whether
  to remove the PawnIO driver too. If yes, runs PawnIO's own uninstaller
  via the registry `QuietUninstallString` (fallback: `UninstallString +
  /SILENT`).
- Dev flag `--dev` on `PerfWindow.exe` falls back to the v0.7.0
  child-spawn IPC, so `cargo run -p perfwindow` works without
  installing the service.
- Integration test (`dashboard/tests/pipe_integration.rs`) that
  spawns the real service, opens the pipe, and asserts a snapshot
  with the new `health` field arrives.

### Changed

- `PerfWindow.exe` manifest: `requireAdministrator` → `asInvoker`. No
  more UAC prompts in normal use.
- `sensord.exe` now has three modes: `--probe` (diagnostic, unchanged),
  `--service` (Worker host for SCM), default (console child for dev).

### Fixed

- **`ERROR_ELEVATION_REQUIRED` (Win32 740)** on environments that
  silently denied UAC elevation for the dashboard binary. The dashboard
  no longer requests elevation; the service it talks to was already
  elevated by SCM.

## [0.7.0] — 2026-05-25

A **chrome + sensors** release: window controls, two new data sources
(ASUS ATK fans, active display info), a richer GPU LOAD tooltip with
per-engine DXGI breakdown, and a responsive panel layout that drops
non-essential rows when the window is shrunk so the dashboard always
fits without scrolling.

### Added
- **F11 fullscreen** toggle. Hides the OS window chrome (title bar,
  minimise / close / maximise) while fullscreen so the dashboard fills
  the screen edge-to-edge; F11 again restores the windowed state.
- **ASUS ATK fan readings** in the footer. `sensord` reads the
  `AsusAtkWmi_WMNB` WMI bridge that the ATK Package driver installs on
  every ASUS gaming laptop and most ASUS desktops; CPU and GPU fan
  values surface as `"CPU FAN 89  GPU FAN 104"`. Silently skipped on
  non-ASUS hardware — the WMI call swallows every failure path and
  returns no data.
- **Active display info** in the footer. `sensord` reads the primary
  monitor's resolution and refresh rate via Win32 `EnumDisplaySettings`
  and surfaces `"1920x1080 @ 60Hz"` next to the other footer figures.
- **GPU LOAD tooltip — per-engine DXGI breakdown.** Hovering the GPU
  load donut now shows a per-engine table (3D, Copy, Video Decode,
  Video Encode, Optical Flow, Overlay, Security, VR) sorted by
  utilisation. Engines below 0.5 % are hidden so the tooltip is
  readable on idle GPUs. The discrete-GPU panel and the iGPU panel
  share this.

### Changed
- **Panels are now responsive.** Below ~1100 px window width the
  dashboard enters `Compact` mode and hides secondary stat rows
  (CPU's TJMAX / VCORE, GPU's HOTSPOT / JUNCTION / PCIE / V, RAM's
  CACHED / DIMM) so each card fits without an internal scroll bar.
  The default launch size (1180×600) and any larger window stay in
  `Full` mode and show every reading. Heat-map, sparklines and donut
  remain in both modes.

### Known limitations
- Intel iGPU temperature is still absent on hardware where LHM's IGCL
  path does not return it (the FX507VI dev laptop is one such). The
  iGPU panel surfaces every reading IGCL does expose (Clock, Power,
  Voltage, shared memory) and hides the TEMP row when no sensor is
  present. Bundling an Intel IGCL DLL would unlock it but requires a
  redistribution review and is deferred.

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

[Unreleased]: https://github.com/lukastojiljkovic/PerfWindow/compare/v0.9.5...HEAD
[0.9.5]: https://github.com/lukastojiljkovic/PerfWindow/releases/tag/v0.9.5
[0.9.4]: https://github.com/lukastojiljkovic/PerfWindow/releases/tag/v0.9.4
[0.9.3]: https://github.com/lukastojiljkovic/PerfWindow/releases/tag/v0.9.3
[0.9.2]: https://github.com/lukastojiljkovic/PerfWindow/releases/tag/v0.9.2
[0.9.1]: https://github.com/lukastojiljkovic/PerfWindow/releases/tag/v0.9.1
[0.9.0]: https://github.com/lukastojiljkovic/PerfWindow/releases/tag/v0.9.0
[0.8.1]: https://github.com/lukastojiljkovic/PerfWindow/releases/tag/v0.8.1
[0.8.0]: https://github.com/lukastojiljkovic/PerfWindow/releases/tag/v0.8.0
[0.7.0]: https://github.com/lukastojiljkovic/PerfWindow/releases/tag/v0.7.0
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
