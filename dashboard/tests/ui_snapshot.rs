//! UI snapshot suite. Each scenario is a single `#[test]` that iterates the
//! six themes and snapshots into `tests/snapshots/<theme>/<scenario>.png`.
//!
//! Re-capture after an intentional UI change:
//!
//! ```powershell
//! $env:UPDATE_SNAPSHOTS = "1"
//! cargo test --release --test ui_snapshot -- --test-threads=1
//! Remove-Item Env:UPDATE_SNAPSHOTS
//! ```
//!
//! `--test-threads=1` is recommended: every scenario spins up its own wgpu
//! surface, and parallel surface creation has been observed to crash with
//! STATUS_ACCESS_VIOLATION on Windows under load (the v0.9.0 PerMonitorV2
//! manifest change made this more sensitive). Single-threaded runs are
//! deterministic; the parallel form is only marginally faster and not worth
//! the flakiness when capturing baselines.
//!
//! Current suite size: 10 scenarios (5 modal + 5 loading screen) x 6 themes,
//! plus 2 mini-strip scenarios x 2 themes, = 64 PNGs under
//! `tests/snapshots/<theme>/`.

use egui_kittest::{Harness, SnapshotResults};
use perfwindow::app::PerfApp;
use perfwindow::config::{Config, ThemeId};
use perfwindow::theme::{self, Theme};
use perfwindow::ui::update_modal::ModalPhase;
use perfwindow::update::{Release, UpdateState};

/// The full theme matrix, in palette-cycle order. Every scenario test loops
/// over this list and snapshots each theme into its own subdirectory.
const ALL_THEMES: &[ThemeId] = &[
    ThemeId::Light,
    ThemeId::Amber,
    ThemeId::Slate,
    ThemeId::Phosphor,
    ThemeId::Synthwave,
    ThemeId::Crimson,
];

/// Subdirectory under `tests/snapshots/` for `theme`. Kept in sync with
/// `ALL_THEMES` so each iteration writes one PNG per theme per scenario.
fn theme_dir(id: ThemeId) -> &'static str {
    match id {
        ThemeId::Light => "light",
        ThemeId::Amber => "amber",
        ThemeId::Slate => "slate",
        ThemeId::Phosphor => "phosphor",
        ThemeId::Synthwave => "synthwave",
        ThemeId::Crimson => "crimson",
    }
}

/// Run `body` in a harness pre-sized to `size`, with `theme` and the bundled
/// fonts installed on the first frame, and snapshot the result to
/// `tests/snapshots/<theme_dir>/<scenario>.png`. The pass/fail outcome is
/// appended to `results` so a single test can snapshot every theme in one
/// run without tripping the `SnapshotResults` aggregation invariant.
///
/// The first frame installs fonts and the palette — egui defers the font swap
/// to the next frame, so `body` is only invoked from frame 2 onward and the
/// modal lays out at its final size before the snapshot is taken.
fn snapshot_scenario(
    results: &mut SnapshotResults,
    size: egui::Vec2,
    theme_id: ThemeId,
    scenario: &str,
    body: impl FnMut(&egui::Context) + 'static,
) {
    let theme = Theme::for_id(theme_id);
    let mut frame = 0u32;
    let mut body = body;
    let mut harness = Harness::builder().with_size(size).build_ui(move |ui| {
        let ctx = ui.ctx().clone();
        if frame == 0 {
            theme::install_fonts(&ctx);
            theme.apply(&ctx);
        }
        frame += 1;
        if frame >= 2 {
            body(&ctx);
        }
    });

    // Two explicit frames so the queued font swap settles before snapshot.
    // `run_ok` tolerates scenarios that keep requesting repaints (e.g. the
    // loading_screen spinner via `request_repaint_after`); the two trailing
    // `step()` calls still guarantee the font swap has settled.
    harness.run_ok();
    harness.step();
    harness.step();

    results.add(harness.try_snapshot(format!("{}/{}", theme_dir(theme_id), scenario)));
}

/// Build a `PerfApp` in `theme_id` with the settings modal opened.
fn app_with_settings_open(theme_id: ThemeId) -> PerfApp {
    let cfg = Config {
        theme: theme_id,
        ..Config::default()
    };
    let mut app = PerfApp::for_tests(cfg);
    app.settings_open = true;
    app
}

/// A deterministic stand-in for a real GitHub release payload. All fields
/// are static strings so two captures of the same scenario render byte-for-byte
/// identically — no timestamps, no network state, no varying byte counts.
fn fake_release() -> Release {
    Release {
        tag_name: "v0.12.0".to_string(),
        name: "PerfWindow 0.12.0".to_string(),
        html_url: "https://example/v0.12.0".to_string(),
        body: "### Added\n- A clearer update window that lists what changed.\n\n\
               ### Fixed\n- The close button no longer shows an empty square."
            .to_string(),
        published_at: Some("2026-10-08T09:30:00Z".to_string()),
        assets: vec![perfwindow::update::release::Asset {
            name: "PerfWindow-Setup.exe".to_string(),
            browser_download_url: "https://example/v0.12.0/PerfWindow-Setup.exe".to_string(),
            size: 60_000_000,
        }],
    }
}

/// A release body with three sections and a dozen bullets, several wrapping
/// inside the modal. Exercises the notes `ScrollArea` at a short viewport.
fn fake_release_long() -> Release {
    let mut release = fake_release();
    release.body = String::from(
        r#"### Added

- A rewritten update modal that lists what changed instead of showing the raw release notes as typed.
- Release notes are folded one bullet at a time, so a wrapped continuation stays with its bullet.
- A link to the full release notes on GitHub sits under the list.
- The modal keeps each theme's own accent colour in the notes.

### Changed

- The modal hugs its content, so the action row sits directly under the notes.
- The banner's vertical padding is symmetric and its action chips are centred on the text.
- The close button is painted with two strokes so every theme font can draw it.
- The header reads as a sentence instead of two stacked version columns.

### Fixed

- The close button no longer renders as an empty square.
- Wrapped bullet lines hang under the text instead of under the dot.
- The release date is shown whenever GitHub publishes one.
- The empty band below the action row is gone.
"#,
    );
    release
}

/// Put `release` into `app`'s shared update state as an available update.
fn publish_available(app: &mut PerfApp, release: Release) {
    if let Ok(mut g) = app.update_state.lock() {
        *g = UpdateState::Available {
            release,
            checked_at: std::time::SystemTime::UNIX_EPOCH,
        };
    }
}

/// A `PerfApp` in `theme_id` with an update available but no modal open.
fn app_with_update_available(theme_id: ThemeId) -> PerfApp {
    let cfg = Config {
        theme: theme_id,
        ..Config::default()
    };
    let mut app = PerfApp::for_tests(cfg);
    publish_available(&mut app, fake_release());
    app
}

/// Build a `PerfApp` in `theme_id` with the update modal opened in `phase`.
///
/// The Downloading phase is special: `update_modal` rebuilds the phase each
/// frame by reading `update_download_progress`, so the `bytes`/`total` baked
/// into the `ModalPhase::Downloading { .. }` we pass in here is overwritten
/// before paint. To keep the snapshot deterministic at 42 %, we also seed the
/// shared progress mutex with matching bytes/total values.
fn app_with_update_modal(theme_id: ThemeId, phase: ModalPhase) -> PerfApp {
    let mut app = app_with_update_available(theme_id);
    if let ModalPhase::Downloading { bytes, total, .. } = &phase {
        if let Ok(mut p) = app.update_download_progress.lock() {
            *p = perfwindow::update::download::DownloadProgress {
                bytes: *bytes,
                total: *total,
            };
        }
    }
    app.update_modal_open = true;
    app.update_modal_phase = phase;
    app
}

#[test]
fn settings_modal_full_matches_baseline() {
    let mut results = SnapshotResults::new();
    for &id in ALL_THEMES {
        let mut app = app_with_settings_open(id);
        snapshot_scenario(
            &mut results,
            egui::vec2(1180.0, 800.0),
            id,
            "settings_modal_full",
            move |ctx| {
                perfwindow::ui::settings::settings_modal(ctx, &mut app);
            },
        );
    }
}

#[test]
fn settings_modal_short_viewport_matches_baseline() {
    // The 400 px viewport is the v0.2.6 regression guard: the modal must
    // render with a scrollbar and keep the title bar's close ✕ reachable
    // instead of overflowing past the bottom of the window.
    let mut results = SnapshotResults::new();
    for &id in ALL_THEMES {
        let mut app = app_with_settings_open(id);
        snapshot_scenario(
            &mut results,
            egui::vec2(1180.0, 400.0),
            id,
            "settings_modal_short_viewport",
            move |ctx| {
                perfwindow::ui::settings::settings_modal(ctx, &mut app);
            },
        );
    }
}

#[test]
fn update_modal_confirm_matches_baseline() {
    let mut results = SnapshotResults::new();
    for &id in ALL_THEMES {
        let mut app = app_with_update_modal(id, ModalPhase::Confirm);
        snapshot_scenario(
            &mut results,
            egui::vec2(1180.0, 800.0),
            id,
            "update_modal_confirm",
            move |ctx| {
                perfwindow::ui::update_modal::update_modal(ctx, &mut app);
            },
        );
    }
}

#[test]
fn update_modal_downloading_matches_baseline() {
    let mut results = SnapshotResults::new();
    for &id in ALL_THEMES {
        let mut app = app_with_update_modal(
            id,
            ModalPhase::Downloading {
                progress: 0.42,
                bytes: 25_000_000,
                total: 60_000_000,
            },
        );
        snapshot_scenario(
            &mut results,
            egui::vec2(1180.0, 800.0),
            id,
            "update_modal_downloading",
            move |ctx| {
                perfwindow::ui::update_modal::update_modal(ctx, &mut app);
            },
        );
    }
}

#[test]
fn update_modal_failed_matches_baseline() {
    let mut results = SnapshotResults::new();
    for &id in ALL_THEMES {
        let mut app = app_with_update_modal(
            id,
            ModalPhase::Failed {
                message: "connection reset by peer".to_string(),
            },
        );
        snapshot_scenario(
            &mut results,
            egui::vec2(1180.0, 800.0),
            id,
            "update_modal_failed",
            move |ctx| {
                perfwindow::ui::update_modal::update_modal(ctx, &mut app);
            },
        );
    }
}

#[test]
fn update_modal_confirm_long_notes_matches_baseline() {
    // A 1180x600 window with three sections and a dozen bullets: the notes
    // area must scroll while the header, link and action row stay put.
    let mut results = SnapshotResults::new();
    for &id in ALL_THEMES {
        let mut app = app_with_update_modal(id, ModalPhase::Confirm);
        publish_available(&mut app, fake_release_long());
        snapshot_scenario(
            &mut results,
            egui::vec2(1180.0, 600.0),
            id,
            "update_modal_confirm_long_notes",
            move |ctx| {
                perfwindow::ui::update_modal::update_modal(ctx, &mut app);
            },
        );
    }
}

#[test]
fn update_banner_matches_baseline() {
    // The banner as the app hosts it: a title bar above it, both strips
    // spanning the full 1180-wide window.
    let mut results = SnapshotResults::new();
    for &id in ALL_THEMES {
        let mut app = app_with_update_available(id);
        let theme = Theme::for_id(id);
        snapshot_scenario(
            &mut results,
            egui::vec2(1180.0, 600.0),
            id,
            "update_banner",
            move |ctx| {
                #[allow(deprecated)]
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE.fill(theme.bg))
                    .show(ctx, |ui| {
                        egui::Panel::top("pw_title_bar")
                            .frame(egui::Frame::NONE)
                            .show_inside(ui, |ui| perfwindow::ui::title_bar(ui, &mut app));
                        if perfwindow::ui::update_banner::is_visible(&app) {
                            egui::Panel::top("pw_update_banner")
                                .frame(egui::Frame::NONE)
                                .show_inside(ui, |ui| {
                                    perfwindow::ui::update_banner::update_banner(ui, &mut app)
                                });
                        }
                    });
            },
        );
    }
}

#[test]
fn changelog_after_update_matches_baseline() {
    // The post-update viewer: the running version's section under the
    // "PerfWindow was updated to …" heading, with "Show all versions".
    let mut results = SnapshotResults::new();
    for &id in ALL_THEMES {
        let mut app = PerfApp::for_tests(Config {
            theme: id,
            ..Config::default()
        });
        app.show_changelog = true;
        app.changelog_version = Some(env!("CARGO_PKG_VERSION").to_string());
        app.changelog_show_all = false;
        snapshot_scenario(
            &mut results,
            egui::vec2(1180.0, 600.0),
            id,
            "changelog_after_update",
            move |ctx| {
                perfwindow::ui::changelog_modal::changelog_modal(ctx, &mut app);
            },
        );
    }
}

// ---- Loading screen scenarios (v0.9.0) -------------------------------

use perfwindow::ipc::connect::{ConnectPhase, FailedReason};
use perfwindow::ui::loading_screen::loading_screen;

/// Render the loading screen for `phase` inside the standard 1180x600
/// viewport for each theme.
fn snapshot_loading_phase(scenario: &str, phase: ConnectPhase) {
    let mut results = SnapshotResults::new();
    for &id in ALL_THEMES {
        let theme = Theme::for_id(id);
        let phase_clone = phase.clone();
        snapshot_scenario(
            &mut results,
            egui::vec2(1180.0, 600.0),
            id,
            scenario,
            move |ctx| {
                // Loading screen needs a `Ui`; the modal scenarios above use
                // `Window`/`Area` so they paint directly into `ctx`. Wrapping
                // in a CentralPanel matches how `card_grid` hosts it in prod.
                #[allow(deprecated)]
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE.fill(theme.bg))
                    .show(ctx, |ui| {
                        let _ = loading_screen(ui, &theme, &phase_clone, None);
                    });
            },
        );
    }
}

#[test]
fn loading_opening_pipe_matches_baseline() {
    snapshot_loading_phase("loading_opening_pipe", ConnectPhase::OpeningPipe);
}

#[test]
fn loading_requesting_elevation_matches_baseline() {
    snapshot_loading_phase(
        "loading_requesting_elevation",
        ConnectPhase::RequestingElevation,
    );
}

#[test]
fn loading_starting_service_matches_baseline() {
    snapshot_loading_phase("loading_starting_service", ConnectPhase::StartingService);
}

#[test]
fn loading_loading_sensors_matches_baseline() {
    snapshot_loading_phase("loading_loading_sensors", ConnectPhase::LoadingSensors);
}

#[test]
fn loading_failed_uac_cancelled_matches_baseline() {
    snapshot_loading_phase(
        "loading_failed_uac_cancelled",
        ConnectPhase::Failed(FailedReason::UacCancelled),
    );
}

/// Staged-init checklist mid-load: three categories done, one loading, four
/// pending. One theme is enough — the checklist adds no theme-specific
/// geometry beyond what the phase scenarios above already cover per theme.
#[test]
fn loading_sensors_checklist_matches_baseline() {
    use perfwindow::ipc::snapshot::ProgressInfo;

    let mut results = SnapshotResults::new();
    let theme = Theme::for_id(ThemeId::Slate);
    let progress = ProgressInfo {
        loading: Some("gpu".into()),
        done: vec!["cpu".into(), "ram".into(), "motherboard".into()],
        pending: vec![
            "storage".into(),
            "network".into(),
            "controller".into(),
            "battery".into(),
        ],
    };
    snapshot_scenario(
        &mut results,
        egui::vec2(1180.0, 600.0),
        ThemeId::Slate,
        "loading_sensors_checklist",
        move |ctx| {
            #[allow(deprecated)]
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE.fill(theme.bg))
                .show(ctx, |ui| {
                    let _ =
                        loading_screen(ui, &theme, &ConnectPhase::LoadingSensors, Some(&progress));
                });
        },
    );
}

// ---- Mini-strip scenarios --------------------------------------------

/// The themes the mini strip is snapshotted in. Two palettes are enough: the
/// strip is one row of text and two hand-drawn icons, so Slate covers the dark
/// themes and Light covers the pale one.
const STRIP_THEMES: &[ThemeId] = &[ThemeId::Slate, ThemeId::Light];

/// A deterministic snapshot carrying every reading the strip can show, so the
/// `mini_strip_full` baseline exercises the whole row and `mini_strip_narrow`
/// exercises dropping from the right.
fn strip_snapshot() -> perfwindow::ipc::Snapshot {
    perfwindow::ipc::parse_snapshot(
        r#"{"v":1,"ts":1747645200,
           "cpu":{"name":"Test CPU","load":23.4,"temp":58.0},
           "gpu":[{"name":"RTX 4070","kind":"discrete","load":41.2,"temp":62.0,
                   "vram_used_mb":6246.4,"vram_total_mb":8192.0}],
           "ram":{"used_mb":14540.8,"total_mb":32768,"load":47.2},
           "net":{"adapter":"Ethernet","down_bps":1258291.0,"up_bps":122880.0,
                  "link_bps":1000000000},
           "battery":{"charge_pct":87.4,"rate_w":-12.0}}"#,
    )
    .expect("the strip snapshot parses")
}

/// Snapshot the strip row alone, at `width` logical px and the strip's own
/// height, for each theme in [`STRIP_THEMES`]. The strip is rendered through
/// the same central panel production uses, so the chrome fill and the bottom
/// rule land exactly where they do on screen.
fn snapshot_strip(scenario: &str, width: f32) {
    let mut results = SnapshotResults::new();
    for &id in STRIP_THEMES {
        let theme = Theme::for_id(id);
        let config = Config {
            theme: id,
            // Show every reading, battery included, so the scenario covers the
            // longest possible row.
            mini_strip_values: perfwindow::config::MiniStripValues {
                battery: true,
                ..Default::default()
            },
            ..Config::default()
        };
        let snapshot = strip_snapshot();
        snapshot_scenario(
            &mut results,
            egui::vec2(width, perfwindow::ui::mini_strip::STRIP_HEIGHT),
            id,
            scenario,
            move |ctx| {
                #[allow(deprecated)]
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE.fill(theme.chrome))
                    .show(ctx, |ui| {
                        let _ = perfwindow::ui::mini_strip::strip_row(
                            ui,
                            &theme,
                            Some(&snapshot),
                            &config,
                        );
                    });
            },
        );
    }
}

/// Every reading, on a 1920 px monitor: the widest row the strip can draw.
#[test]
fn mini_strip_full_matches_baseline() {
    snapshot_strip("mini_strip_full", 1920.0);
}

/// The same readings on a narrow monitor, where the row does not fit and the
/// rightmost readings are dropped.
///
/// 900 logical px is what a 1350 px monitor shows at 150 % scaling. The strip's
/// whole row is only ~950 px wide at the design's 13 px data font, so a
/// 1280 px strip would still show every reading; 900 is the width that
/// actually exercises dropping from the right.
#[test]
fn mini_strip_narrow_matches_baseline() {
    snapshot_strip("mini_strip_narrow", 900.0);
}
