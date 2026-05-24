//! UI snapshot suite. Each scenario is a single `#[test]` that iterates the
//! six themes and snapshots into `tests/snapshots/<theme>/<scenario>.png`.
//!
//! Re-capture after an intentional UI change:
//!
//! ```powershell
//! $env:UPDATE_SNAPSHOTS = "1"
//! cargo test --release --test ui_snapshot
//! Remove-Item Env:UPDATE_SNAPSHOTS
//! ```

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
    let mut harness = Harness::builder()
        .with_size(size)
        .build_ui(move |ui| {
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
    harness.run();
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
        tag_name: "v0.3.0".to_string(),
        name: "PerfWindow 0.3.0".to_string(),
        html_url: "https://example/v0.3.0".to_string(),
        body: "* added thing\n* fixed thing".to_string(),
        assets: vec![perfwindow::update::release::Asset {
            name: "PerfWindow-Setup.exe".to_string(),
            browser_download_url: "https://example/v0.3.0/PerfWindow-Setup.exe".to_string(),
            size: 60_000_000,
        }],
    }
}

/// Build a `PerfApp` in `theme_id` with the update modal opened in `phase`.
///
/// The Downloading phase is special: `update_modal` rebuilds the phase each
/// frame by reading `update_download_progress`, so the `bytes`/`total` baked
/// into the `ModalPhase::Downloading { .. }` we pass in here is overwritten
/// before paint. To keep the snapshot deterministic at 42 %, we also seed the
/// shared progress mutex with matching bytes/total values.
fn app_with_update_modal(theme_id: ThemeId, phase: ModalPhase) -> PerfApp {
    let cfg = Config {
        theme: theme_id,
        ..Config::default()
    };
    let mut app = PerfApp::for_tests(cfg);
    if let Ok(mut g) = app.update_state.lock() {
        *g = UpdateState::Available {
            release: fake_release(),
            checked_at: std::time::SystemTime::UNIX_EPOCH,
        };
    }
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
