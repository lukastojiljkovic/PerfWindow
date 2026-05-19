use crate::config::Config;
use crate::history::History;
use crate::ipc::{Sensord, Snapshot};
use crate::theme::{self, system, Theme};

/// Whether the sensor feed is healthy.
#[derive(PartialEq)]
pub enum Status {
    Running,
    SensordDown,
}

/// The PerfWindow application.
pub struct PerfApp {
    pub config: Config,
    pub theme: Theme,
    pub history: History,
    pub sensord: Option<Sensord>,
    pub latest: Option<Snapshot>,
    pub status: Status,
    pub settings_open: bool,
    os_is_light: bool,
}

impl PerfApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::install_fonts(&cc.egui_ctx);
        let config = Config::load();
        let os_is_light = system::windows_is_light();
        let theme = Theme::for_id(system::effective_theme_id(&config, os_is_light));
        theme.apply(&cc.egui_ctx);

        let ctx = cc.egui_ctx.clone();
        let sensord = Sensord::spawn(move || ctx.request_repaint())
            .inspect_err(|e| eprintln!("PerfWindow: failed to start sensord: {e}"))
            .ok();

        let status = if sensord.is_some() {
            Status::Running
        } else {
            Status::SensordDown
        };

        let mut app = Self {
            config,
            theme,
            history: History::default(),
            sensord,
            latest: None,
            status,
            settings_open: false,
            os_is_light,
        };
        if let Some(s) = &mut app.sensord {
            s.set_interval(app.config.refresh.as_millis());
        }
        app
    }

    /// Pull the newest snapshot out of the shared state and update history.
    fn ingest(&mut self) {
        let Some(sensord) = &self.sensord else {
            self.status = Status::SensordDown;
            return;
        };
        if let Ok(state) = sensord.state.lock() {
            if !state.alive {
                self.status = Status::SensordDown;
            }
            if let Some(snap) = &state.latest {
                if self.latest.as_ref().map(|p| p.ts) != Some(snap.ts) {
                    self.history.record(snap);
                    self.latest = Some(snap.clone());
                }
            }
        }
    }
}

impl eframe::App for PerfApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.ingest();

        ui.heading(self.theme.name);
        match self.status {
            Status::Running => {
                if let Some(snap) = &self.latest {
                    ui.label(format!("snapshot ts={}", snap.ts));
                } else {
                    ui.label("waiting for sensord\u{2026}");
                }
            }
            Status::SensordDown => {
                ui.colored_label(self.theme.hot, "sensord stopped");
            }
        }

        // Watchdog repaint a little past the refresh interval; new snapshots
        // already wake the UI via request_repaint from the reader thread.
        ctx.request_repaint_after(std::time::Duration::from_millis(
            self.config.refresh.as_millis() as u64 + 500,
        ));
    }
}
