#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(dead_code)]

use perfwindow::app::PerfApp;
use perfwindow::config::Config;

fn main() -> eframe::Result {
    // `--dev` swaps the production named-pipe client for the legacy
    // child-spawn path (no installed Windows service required) — useful for
    // `cargo run` against a freshly built sensord. Default = pipe.
    let dev_mode = std::env::args().any(|a| a == "--dev");

    // Read the persisted `always_on_top` preference before the viewport opens
    // so the window starts at the correct Z-level and doesn't briefly flash
    // behind other windows on launch. `PerfApp::new` re-reads the config and
    // is the canonical owner; this peek is purely a cosmetic-startup fix.
    let initially_on_top = Config::load().always_on_top;
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("PerfWindow")
        // Default size matches the grid's natural footprint so the window
        // opens with no empty band. Min height equals the default height
        // so the user cannot shrink the window into the cards; min width
        // sits just below the 4-col breakpoint so the grid can collapse
        // to 3 cols when the user shrinks the window deliberately.
        .with_inner_size([1180.0, 600.0])
        .with_min_inner_size([720.0, 500.0]);
    if initially_on_top {
        viewport = viewport.with_always_on_top();
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "PerfWindow",
        options,
        Box::new(move |cc| Ok(Box::new(PerfApp::new(cc, dev_mode)))),
    )
}
