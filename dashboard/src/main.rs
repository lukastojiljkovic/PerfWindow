#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(dead_code)]

mod app;
mod config;
mod format;
mod history;
mod ipc;
mod panels;
mod theme;
mod ui;
mod update;
mod widgets;

use app::PerfApp;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("PerfWindow")
            // Default size matches the grid's natural footprint so the window
            // opens with no empty band; min size is just below the 4-col
            // breakpoint so the grid can collapse to 3 cols when the user
            // shrinks the window deliberately.
            .with_inner_size([1180.0, 580.0])
            .with_min_inner_size([960.0, 500.0]),
        ..Default::default()
    };
    eframe::run_native(
        "PerfWindow",
        options,
        Box::new(|cc| Ok(Box::new(PerfApp::new(cc)))),
    )
}
