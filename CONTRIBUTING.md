# Contributing

Thanks for looking. Bug reports and fixes are welcome.

## Before you start

- **For a bug**, open an issue with the steps to reproduce it, your Windows
  version and build, and your CPU and GPU model.
- **For a bigger change**, open an issue first and say what you intend. A PR
  that changes direction is harder to accept than to discuss.
- **One change per pull request.** A reviewable diff is the point.
- For a security problem, follow [SECURITY.md](SECURITY.md) instead of opening
  an issue.

## Getting it running

Prerequisites (from the README's *Building* section):

- The [.NET 8 SDK](https://dotnet.microsoft.com/download), for `sensord`.
- A [Rust](https://www.rust-lang.org/tools/install) toolchain with the MSVC
  target, plus the Visual Studio Build Tools.
- [Inno Setup 6](https://jrsoftware.org/isdl.php) for the installer, and
  [cargo-about](https://github.com/EmbarkStudios/cargo-about) for the bundled
  license texts.

The dashboard embeds `sensord.exe` as a sibling through `dashboard/build.rs`,
so **publish the sensor helper before any `cargo` command**:

```powershell
dotnet publish sensord/src -c Release -r win-x64 --self-contained

cd dashboard
cargo build --release
cargo fmt --check
cargo clippy --release -- -D warnings
cargo test --release -- --test-threads=1
```

The .NET side has its own format check and tests:

```powershell
dotnet format sensord/src/sensord.csproj --verify-no-changes
dotnet format sensord/tests/Sensord.Tests.csproj --verify-no-changes
dotnet test sensord/tests/Sensord.Tests.csproj
```

`.\build\build.ps1` runs all of the above and then compiles the installer to
`dist\PerfWindow-Setup.exe`. CI runs the same commands; see
`.github/workflows/ci.yml`.

## The sensor helper

`sensord/` is a .NET 8 solution published self-contained for `win-x64`. It has
three modes:

- `sensord.exe --service` runs as the `PerfWindowSensor` Windows Service and
  streams NDJSON snapshots over the `\\.\pipe\PerfWindowSensor` named pipe.
- `sensord.exe` with no arguments runs in console mode: NDJSON on stdout,
  control on stdin. The dashboard's `--dev` flag spawns it this way, so you can
  work on the dashboard without installing the service.
- `sensord.exe --probe` dumps the full LibreHardwareMonitor hardware and sensor
  tree, which is how you check what the machine exposes.

Run the dashboard with `cargo run -p perfwindow -- --dev`. As a normal user,
this path cannot read the administrator-only sensors (CPU MSR temperature and
clock, NVMe SMART), so the Storage card stays empty; the installed service
reads them as `LocalSystem`. After touching `sensord`, re-publish it or the
Rust integration tests exercise a stale service.

## Tests

Rust tests live in `dashboard/src` and `dashboard/tests`:

- `ui_snapshot.rs` renders ten scenarios in all six themes and compares each to
  a PNG under `tests/snapshots/<theme>/`. This is the visual test: after an
  intentional UI change, re-capture the baselines and commit them.

  ```powershell
  $env:UPDATE_SNAPSHOTS = "1"
  cargo test --release --test ui_snapshot -- --test-threads=1
  Remove-Item Env:UPDATE_SNAPSHOTS
  ```

- `pipe_integration.rs` starts `sensord.exe` and reads snapshots over the pipe.
  It needs a built `sensord.exe` next to the test binary, so publish first.
- `render_stress.rs` drives the whole dashboard through a raw `egui` context,
  with no GPU involved.
- `displays_live.rs` and `live_release.rs` are `#[ignore]`d manual checks.
  The first reads the live display topology in an interactive session; the
  second reads a captured GitHub release payload pointed at by
  `PERFWINDOW_RELEASE_JSON`.

Run the snapshot and pipe suites single-threaded (`--test-threads=1`): they
share a wgpu context and have been observed to crash with
`STATUS_ACCESS_VIOLATION` when run concurrently.

The .NET tests are under `sensord/tests/`.

## What a change must include

- **A test for new behavior.** A suite that stays green while the feature is
  broken reads as coverage without being it.
- **A line in [CHANGELOG.md](CHANGELOG.md)** under *Unreleased* for anything
  users notice. The changelog is embedded in the dashboard, opened by the
  version number in the footer, and becomes the release notes the update window
  shows, so write each line as a user would describe the change, not as the
  code does: "The dashboard fits the window without scrolling", not "Removed
  the ScrollArea".
- **No invented numbers.** The README and the website claim only what the code
  does.

## Code of conduct

Participation is covered by [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md)
(Contributor Covenant 2.1). Reports go to stojiljkovic.d.luka@gmail.com.
