#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(dead_code)]

mod config;
mod format;
mod history;
mod ipc;
mod theme;

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
        Box::new(|cc| Ok(Box::new(App::new(cc.egui_ctx.clone())))),
    )
}

struct App {
    sensord: Option<ipc::Sensord>,
}

impl App {
    fn new(ctx: egui::Context) -> Self {
        let sensord = ipc::Sensord::spawn(move || ctx.request_repaint()).ok();
        Self { sensord }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        match &self.sensord {
            None => {
                ui.label("sensord failed to start");
            }
            Some(s) => {
                let st = s.state.lock().unwrap();
                ui.label(format!("alive: {}", st.alive));
                if let Some(snap) = &st.latest {
                    if let Some(cpu) = &snap.cpu {
                        ui.label(format!(
                            "{} — load {:?} temp {:?}",
                            cpu.name, cpu.load, cpu.temp
                        ));
                    }
                } else {
                    ui.label("waiting for first snapshot\u{2026}");
                }
            }
        }
    }
}
