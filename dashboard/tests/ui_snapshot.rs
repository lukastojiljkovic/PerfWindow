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
//! Current suite size: 10 scenarios (5 modal + 5 loading screen) x 6 themes
//! = 60 PNGs under `tests/snapshots/<theme>/`, plus the single-theme
//! `loading_sensors_checklist` and the two background-opacity scenarios on the
//! Slate and Light themes: 65 PNGs in total.

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

/// Build a harness pre-sized to `size` that installs `theme_id`'s fonts and
/// palette on the first frame and calls `body` from frame 2 onward — egui
/// defers the font swap to the next frame, so a body invoked earlier would
/// lay out against the default fonts and at the wrong metrics.
///
/// The body receives the root [`egui::Ui`] (for scenarios that nest panels
/// the way `PerfApp::ui` does) and the [`egui::Context`] (for the
/// free-floating modals, which are `Window`s on the context).
fn themed_harness(
    size: egui::Vec2,
    theme_id: ThemeId,
    mut body: impl FnMut(&mut egui::Ui, &egui::Context) + 'static,
) -> Harness<'static> {
    let theme = Theme::for_id(theme_id);
    let mut frame = 0u32;
    Harness::builder().with_size(size).build_ui(move |ui| {
        let ctx = ui.ctx().clone();
        if frame == 0 {
            theme::install_fonts(&ctx);
            theme.apply(&ctx);
        }
        frame += 1;
        if frame >= 2 {
            body(ui, &ctx);
        }
    })
}

/// Run `body` in a harness pre-sized to `size`, with `theme` and the bundled
/// fonts installed on the first frame, and snapshot the result to
/// `tests/snapshots/<theme_dir>/<scenario>.png`. The pass/fail outcome is
/// appended to `results` so a single test can snapshot every theme in one
/// run without tripping the `SnapshotResults` aggregation invariant.
fn snapshot_scenario(
    results: &mut SnapshotResults,
    size: egui::Vec2,
    theme_id: ThemeId,
    scenario: &str,
    body: impl FnMut(&egui::Context) + 'static,
) {
    snapshot_scenario_inspected(results, size, theme_id, scenario, body, |_, _| {});
}

/// [`snapshot_scenario`] plus an `inspect` step that receives the rendered
/// pixels before the baseline is compared. The opacity scenarios use it to
/// assert on the alpha channel of the image they are about to pin.
fn snapshot_scenario_inspected(
    results: &mut SnapshotResults,
    size: egui::Vec2,
    theme_id: ThemeId,
    scenario: &str,
    mut body: impl FnMut(&egui::Context) + 'static,
    inspect: impl FnOnce(&[u8], [usize; 2]),
) {
    let mut harness = themed_harness(size, theme_id, move |_ui, ctx| body(ctx));

    // Two explicit frames so the queued font swap settles before snapshot.
    // `run_ok` tolerates scenarios that keep requesting repaints (e.g. the
    // loading_screen spinner via `request_repaint_after`); the two trailing
    // `step()` calls still guarantee the font swap has settled.
    harness.run_ok();
    harness.step();
    harness.step();

    let image = harness.render().expect("scenario renders offscreen");
    inspect(
        image.as_raw(),
        [image.width() as usize, image.height() as usize],
    );

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

// ---- Background opacity scenarios (v0.12.0) --------------------------

/// A small but complete snapshot for the opacity scenario: CPU, GPU, RAM, the
/// always-present Network card and an uptime for the footer. Inline JSON in
/// the `ipc::parse_snapshot` schema — the same shape the sensord pipe carries.
const OPACITY_FIXTURE: &str = r#"{"v":1,"ts":1747645200,
      "cpu":{"name":"Test CPU","load":34.2,"temp":58.0,"clock_mhz":4400,"power_w":52.0},
      "gpu":[{"name":"RTX 4070","kind":"discrete","load":51.0,"temp":71.0,"vram_used_mb":6348,"vram_total_mb":12288}],
      "ram":{"used_mb":15462,"total_mb":32768,"available_mb":17306,"load":47.2},
      "net":{"adapter":"Ethernet","down_bps":4404019,"up_bps":629145},
      "uptime_sec":93784}"#;

/// Build a `PerfApp` running on a populated grid at `percent` background
/// opacity.
fn app_with_opacity(theme_id: ThemeId, percent: u8) -> PerfApp {
    let cfg = Config {
        theme: theme_id,
        background_opacity: percent,
        ..Config::default()
    };
    let mut app = PerfApp::for_tests(cfg);
    app.latest = Some(perfwindow::ipc::parse_snapshot(OPACITY_FIXTURE).expect("fixture parses"));
    app.status = perfwindow::app::Status::Running;
    app
}

/// Nest the production chrome the way `PerfApp::ui` does: title bar on top,
/// footer at the bottom, the grid body in between. Mirrors `app.rs` so the
/// scenario exercises the same surfaces the Background opacity setting dims.
///
/// The panels are hosted on the `Context` rather than inside the harness's
/// `Ui` because the kittest `build_ui` wrapper paints a `Frame::central_panel`
/// behind the body whose fill is `visuals.panel_fill` — which `Theme::apply`
/// sets to the opaque theme background. That backdrop would hide exactly the
/// translucency this scenario measures, and no such frame exists in
/// production (the OS window itself is the backdrop there). Hosting the panels
/// on the context is what the loading-screen scenarios already do.
#[allow(deprecated)]
fn dashboard_ui(ctx: &egui::Context, app: &mut PerfApp) {
    let theme = app.theme.clone();
    egui::Panel::top("pw_title_bar")
        .frame(egui::Frame::NONE)
        .show(ctx, |ui| perfwindow::ui::title_bar(ui, app));
    egui::Panel::bottom("pw_footer")
        .frame(egui::Frame::NONE)
        .show(ctx, |ui| perfwindow::ui::footer(ui, app));
    egui::CentralPanel::default()
        .frame(egui::Frame::NONE.fill(theme.surface(theme.bg, app.config.background_opacity)))
        .show(ctx, |ui| {
            perfwindow::ui::effects::paint_grid(ui, &theme, app.config.background_opacity);
            egui::ScrollArea::vertical()
                .auto_shrink([false, true])
                .show(ui, |ui| perfwindow::ui::card_grid(ui, app));
        });
}

/// Alpha channel of one RGBA pixel in a rendered image.
fn alpha_at(raw: &[u8], width: usize, x: usize, y: usize) -> u8 {
    raw[(y * width + x) * 4 + 3]
}

/// Sample the pixels of a rendered dashboard at `percent` background opacity
/// and assert both halves of the contract: the body surface is genuinely
/// see-through, and a value drawn on top of it is still fully opaque.
fn assert_opacity_pixels(raw: &[u8], width: usize, height: usize, percent: u8) {
    assert!(width > 0 && height > 0, "rendered image is empty");
    assert!(
        raw.len() >= width * height * 4,
        "rendered image is truncated"
    );

    // The middle half of the window is the grid body: the translucent fill
    // shows between the cards, while the card text and borders sit on top.
    let (mut body_alpha, mut text_alpha) = (u8::MAX, 0u8);
    for y in height / 4..height * 3 / 4 {
        for x in 0..width {
            let alpha = alpha_at(raw, width, x, y);
            body_alpha = body_alpha.min(alpha);
            text_alpha = text_alpha.max(alpha);
        }
    }

    let expected = (u16::from(percent) * 255 / 100) as u8;
    assert!(
        body_alpha < 255 && body_alpha.abs_diff(expected) <= 24,
        "body pixel alpha {body_alpha} should sit near {expected} at {percent} %"
    );
    assert_eq!(
        text_alpha, 255,
        "a value pixel must stay fully opaque at {percent} %"
    );
}

/// The dashboard at 60 % background opacity: chrome, body and cards dim, so
/// the desktop behind the window shows through, while every readout stays
/// solid. Captured for one dark theme and the light theme.
#[test]
fn opacity_60_dashboard_matches_baseline() {
    let mut results = SnapshotResults::new();
    for &id in &[ThemeId::Slate, ThemeId::Light] {
        let mut app = app_with_opacity(id, 60);
        snapshot_scenario_inspected(
            &mut results,
            egui::vec2(1180.0, 600.0),
            id,
            "opacity_60_dashboard",
            move |ctx| dashboard_ui(ctx, &mut app),
            |raw, [width, height]| assert_opacity_pixels(raw, width, height, 60),
        );
    }
}

/// The settings modal at 60 % background opacity. The dashboard behind it is
/// dimmed, but the modal is read, not looked through, so it stays fully
/// opaque — the sampled middle pixel proves it.
///
/// The viewport is taller than `settings_modal_full`'s 800 px on purpose: at
/// the app's default height the modal's scroll fold cuts the DISPLAY section in
/// half, which would hide the new Background opacity slider from this
/// baseline. At 1000 px the whole section (toggle, slider, hint) sits above
/// the fold and the snapshot pins it.
#[test]
fn settings_modal_opacity_matches_baseline() {
    let mut results = SnapshotResults::new();
    for &id in &[ThemeId::Slate, ThemeId::Light] {
        let mut app = app_with_settings_open(id);
        app.config.background_opacity = 60;
        snapshot_scenario_inspected(
            &mut results,
            egui::vec2(1180.0, 1000.0),
            id,
            "settings_modal_opacity",
            move |ctx| perfwindow::ui::settings::settings_modal(ctx, &mut app),
            |raw, [width, height]| {
                // The modal is centred, so the middle pixel is inside it.
                assert_eq!(
                    alpha_at(raw, width, width / 2, height / 2),
                    255,
                    "the settings modal must stay opaque at 60 %"
                );
            },
        );
    }
}
