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
            .with_inner_size([1180.0, 760.0])
            .with_min_inner_size([900.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native(
        "PerfWindow",
        options,
        Box::new(|cc| Ok(Box::new(PerfApp::new(cc)))),
    )
}
