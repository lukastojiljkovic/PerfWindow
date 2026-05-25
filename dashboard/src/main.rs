#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(dead_code)]

use perfwindow::app::PerfApp;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("PerfWindow")
            // Default size matches the grid's natural footprint so the window
            // opens with no empty band. Min height equals the default height
            // so the user cannot shrink the window into the cards; min width
            // sits just below the 4-col breakpoint so the grid can collapse
            // to 3 cols when the user shrinks the window deliberately.
            .with_inner_size([1180.0, 600.0])
            .with_min_inner_size([960.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native(
        "PerfWindow",
        options,
        Box::new(|cc| Ok(Box::new(PerfApp::new(cc)))),
    )
}
