//! Zoom-to-fit acceptance tests.
//!
//! Renders the whole dashboard (title bar, footer, grid and every panel) with
//! the worst-case fixture through the kittest harness and inspects the
//! rectangles the renderer produced. `tests/ui_snapshot.rs` covers the pixels;
//! this file covers the geometry that must hold at every size — no scrolling,
//! no card content outside its cell, no stat label/value overlap, and a stable
//! zoom.
//!
//! The four grid scenarios are snapshotted for the slate and light themes only
//! (the image budget matters); the geometry matrix deliberately runs without
//! rendering to keep it fast.

use egui_kittest::{Harness, SnapshotResults};
use perfwindow::app::{PerfApp, Status};
use perfwindow::config::{Config, ThemeId};
use perfwindow::theme::{self, Theme};
use perfwindow::ui::recorder::{self, Record};

/// Build a `PerfApp` showing the worst-case fixture with a little history, so
/// the sparklines render at their natural size.
fn fixture_app(theme_id: ThemeId) -> PerfApp {
    let mut app = PerfApp::for_tests(Config {
        theme: theme_id,
        ..Config::default()
    });
    let snap = perfwindow::test_support::worst_case_snapshot();
    app.history.record(&snap);
    app.history.record(&snap);
    app.latest = Some(snap);
    app.status = Status::Running;
    app
}

/// Render the dashboard chrome and grid into `ui`, mirroring `PerfApp::ui`
/// (the real entry point needs an `eframe::Frame` the harness cannot supply).
fn render_dashboard(ui: &mut egui::Ui, app: &mut PerfApp) {
    egui::Panel::top("pw_title_bar")
        .frame(egui::Frame::NONE)
        .show_inside(ui, |ui| perfwindow::ui::title_bar(ui, app));
    egui::Panel::bottom("pw_footer")
        .frame(egui::Frame::NONE)
        .show_inside(ui, |ui| perfwindow::ui::footer(ui, app));
    let bg = app.theme.bg;
    egui::CentralPanel::default()
        .frame(egui::Frame::NONE.fill(bg))
        .show_inside(ui, |ui| {
            let central = ui.max_rect();
            perfwindow::ui::card_grid(ui, app);
            recorder::central(central);
        });
}

/// Run `frames` steps of a harness sized `size` logical points at `ppp`
/// physical pixels per point, with `theme_id` installed on the first frame,
/// and return the last frame's geometry plus the final zoom.
fn run_harness(theme_id: ThemeId, size: egui::Vec2, ppp: f32, frames: usize) -> (Vec<Record>, f32) {
    let theme = Theme::for_id(theme_id);
    let mut app = fixture_app(theme_id);
    let mut frame = 0usize;
    let mut harness = Harness::builder()
        .with_size(size)
        .with_pixels_per_point(ppp)
        .build_ui(move |ui| {
            let ctx = ui.ctx().clone();
            if frame == 0 {
                theme::install_fonts(&ctx);
                theme.apply(&ctx);
            }
            frame += 1;
            if frame >= 2 {
                render_dashboard(ui, &mut app);
            }
        });
    for _ in 0..frames {
        harness.step();
    }
    let zoom = harness.ctx.zoom_factor();
    (recorder::take(), zoom)
}

/// The geometry matrix: every logical size at 1.0, plus the DPI spot checks.
const SIZES: &[(f32, f32, f32)] = &[
    (720.0, 500.0, 1.0),
    (1024.0, 640.0, 1.0),
    (1180.0, 600.0, 1.0),
    (1280.0, 720.0, 1.0),
    (1366.0, 768.0, 1.0),
    (1600.0, 900.0, 1.0),
    (1920.0, 1080.0, 1.0),
    (2560.0, 1440.0, 1.0),
    (3840.0, 2160.0, 1.0),
    (1920.0, 1080.0, 1.25),
    (1920.0, 1080.0, 1.5),
    (1920.0, 1080.0, 2.0),
    (1280.0, 800.0, 1.5),
];

const EPSILON: f32 = 0.75;

#[test]
fn grid_fits_every_tested_size() {
    let mut failures: Vec<String> = Vec::new();
    for &(w, h, ppp) in SIZES {
        let (records, zoom) = run_harness(ThemeId::Slate, egui::vec2(w, h), ppp, 8);
        let central = records.iter().find_map(|r| match r {
            Record::Central(rect) => Some(*rect),
            _ => None,
        });
        let Some(central) = central else {
            failures.push(format!("{w}x{h}@{ppp}: no central panel rectangle"));
            continue;
        };
        let cards: Vec<(&str, egui::Rect, egui::Rect)> = records
            .iter()
            .filter_map(|r| match r {
                Record::Card { name, slot, rect } => Some((name.as_str(), *slot, *rect)),
                _ => None,
            })
            .collect();
        if cards.is_empty() {
            failures.push(format!("{w}x{h}@{ppp}: no cards rendered"));
            continue;
        }

        // Nothing scrolls: the grid's used rectangle sits inside the central
        // panel (a `ScrollArea` would clip or scroll it), and so does every
        // card.
        if let Some(grid) = records.iter().find_map(|r| match r {
            Record::Grid(rect) => Some(*rect),
            _ => None,
        }) {
            if grid.max.y > central.max.y + EPSILON || grid.min.y < central.min.y - EPSILON {
                failures.push(format!(
                    "{w}x{h}@{ppp}: grid {grid:?} escapes the central panel {central:?}"
                ));
            }
        }
        for (name, slot, rect) in &cards {
            if rect.max.y > central.max.y + EPSILON || slot.max.y > central.max.y + EPSILON {
                failures.push(format!(
                    "{w}x{h}@{ppp}: {name} bottom {:.1} below central panel {:.1}",
                    rect.max.y, central.max.y
                ));
            }
            // Content stays inside the cell the layout allocated.
            if rect.max.y > slot.max.y + EPSILON
                || rect.min.y < slot.min.y - EPSILON
                || rect.max.x > slot.max.x + EPSILON
                || rect.min.x < slot.min.x - EPSILON
            {
                failures.push(format!(
                    "{w}x{h}@{ppp}: {name} content {rect:?} escapes cell {slot:?}"
                ));
            }
        }

        // Stat rows: label and value never overlap.
        for r in &records {
            if let Record::StatRow {
                label,
                value,
                elided,
                text,
            } = r
            {
                if label.intersects(*value) {
                    failures.push(format!(
                        "{w}x{h}@{ppp}: label {label:?} overlaps value {value:?} ({text})"
                    ));
                }
                if *elided && w >= 1180.0 && h >= 600.0 {
                    failures.push(format!(
                        "{w}x{h}@{ppp}: value {text:?} elided at a comfortable size"
                    ));
                }
            }
        }

        if !(0.75..=2.5).contains(&zoom) {
            failures.push(format!("{w}x{h}@{ppp}: zoom {zoom} out of band"));
        }
        if (w, h, ppp) == (1920.0, 1080.0, 1.0) && zoom <= 1.0 {
            failures.push(format!("1920x1080@1.0: zoom {zoom} not above 1.0"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} geometry failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn zoom_does_not_oscillate() {
    let theme = Theme::for_id(ThemeId::Slate);
    let mut app = fixture_app(ThemeId::Slate);
    let mut frame = 0usize;
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1600.0, 900.0))
        .build_ui(move |ui| {
            let ctx = ui.ctx().clone();
            if frame == 0 {
                theme::install_fonts(&ctx);
                theme.apply(&ctx);
            }
            frame += 1;
            if frame >= 2 {
                render_dashboard(ui, &mut app);
            }
        });
    let mut zooms = Vec::new();
    for _ in 0..10 {
        harness.step();
        zooms.push(harness.ctx.zoom_factor());
    }
    // The first frame computes the zoom; it is applied at the start of the
    // second, after which it must not move again.
    let settled = zooms[2..].to_vec();
    assert!(
        settled.iter().all(|z| (z - settled[0]).abs() < 1e-4),
        "zoom drifted across frames: {zooms:?}"
    );
}

// ---- Grid snapshot scenarios (slate + light only) ---------------------

/// Subdirectory under `tests/snapshots/` for `theme`.
fn theme_dir(id: ThemeId) -> &'static str {
    match id {
        ThemeId::Light => "light",
        ThemeId::Slate => "slate",
        ThemeId::Amber => "amber",
        ThemeId::Phosphor => "phosphor",
        ThemeId::Synthwave => "synthwave",
        ThemeId::Crimson => "crimson",
    }
}

/// Snapshot `scenario` at `size` for the slate and light themes.
fn snapshot_grid(scenario: &str, size: egui::Vec2) {
    let mut results = SnapshotResults::new();
    for id in [ThemeId::Slate, ThemeId::Light] {
        let theme = Theme::for_id(id);
        let mut app = fixture_app(id);
        let mut frame = 0usize;
        let mut harness = Harness::builder().with_size(size).build_ui(move |ui| {
            let ctx = ui.ctx().clone();
            if frame == 0 {
                theme::install_fonts(&ctx);
                theme.apply(&ctx);
            }
            frame += 1;
            if frame >= 2 {
                render_dashboard(ui, &mut app);
            }
        });
        // Fixed step count keeps the zoom and the blinking cursor
        // deterministic.
        for _ in 0..4 {
            harness.step();
        }
        results.add(harness.try_snapshot(format!("{}/{}", theme_dir(id), scenario)));
    }
}

#[test]
fn grid_1280x720_matches_baseline() {
    snapshot_grid("grid_1280x720", egui::vec2(1280.0, 720.0));
}

#[test]
fn grid_1920x1080_matches_baseline() {
    snapshot_grid("grid_1920x1080", egui::vec2(1920.0, 1080.0));
}

#[test]
fn grid_720x500_matches_baseline() {
    snapshot_grid("grid_720x500", egui::vec2(720.0, 500.0));
}

#[test]
fn grid_2560x1440_matches_baseline() {
    snapshot_grid("grid_2560x1440", egui::vec2(2560.0, 1440.0));
}
