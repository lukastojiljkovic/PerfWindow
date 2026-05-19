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

    /// Restart `sensord` after it has died, re-arming the live feed.
    ///
    /// The old [`Sensord`] is dropped *first* (`self.sensord = None`): its
    /// [`Drop`] kills the child, joins the reader and removes the temp exe.
    /// This ordering is mandatory — [`Sensord::spawn`] re-extracts the embedded
    /// executable to `%TEMP%\PerfWindow-sensord-{pid}.exe`, the *same* path the
    /// old child still occupies, and Windows refuses to overwrite a running
    /// `.exe`. Only once the old process is gone do we spawn the replacement.
    ///
    /// `history` is deliberately kept, so the sparklines carry on uninterrupted
    /// across the gap. `latest` is cleared (its readings are now stale) and the
    /// configured refresh interval is re-sent to the fresh child. `status` ends
    /// as [`Status::Running`] only if the respawn succeeded; a failed spawn is
    /// logged and leaves it [`Status::SensordDown`].
    pub fn respawn_sensord(&mut self, ctx: &egui::Context) {
        // Drop the old child *before* spawning: its `Drop` removes the temp
        // exe that `Sensord::spawn` is about to rewrite (see doc comment).
        self.sensord = None;

        let repaint_ctx = ctx.clone();
        self.sensord = Sensord::spawn(move || repaint_ctx.request_repaint())
            .inspect_err(|e| eprintln!("PerfWindow: failed to restart sensord: {e}"))
            .ok();

        self.status = if self.sensord.is_some() {
            Status::Running
        } else {
            Status::SensordDown
        };
        self.latest = None;
        if let Some(sensord) = &mut self.sensord {
            sensord.set_interval(self.config.refresh.as_millis());
        }
    }

    /// Re-apply the current `config` after the user changes a setting.
    ///
    /// Recomputes the effective theme (honouring `follow_windows`), pushes it
    /// into egui's visuals and forwards the refresh interval to `sensord`. The
    /// caller is responsible for persisting `config` via [`Config::save`].
    pub fn apply_config_change(&mut self, ctx: &egui::Context) {
        self.theme = Theme::for_id(system::effective_theme_id(&self.config, self.os_is_light));
        self.theme.apply(ctx);
        if let Some(sensord) = &mut self.sensord {
            sensord.set_interval(self.config.refresh.as_millis());
        }
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

        // Title bar on top, footer on the bottom, the card grid filling the
        // scrollable centre. Panels are nested with `show_inside` (we render
        // into a `Ui`, not a fresh `Context`) and given `Frame::NONE`: each
        // strip paints its own chrome, and egui's default panel frame would
        // otherwise add a `panel_fill` background and an inset margin under it.
        // egui still draws the 1 px separator line between the panels.
        egui::Panel::top("pw_title_bar")
            .frame(egui::Frame::NONE)
            .show_inside(ui, |ui| {
                crate::ui::title_bar(ui, self);
            });
        egui::Panel::bottom("pw_footer")
            .frame(egui::Frame::NONE)
            .show_inside(ui, |ui| {
                crate::ui::footer(ui, self);
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(self.theme.bg))
            .show_inside(ui, |ui| {
                // The faint grid sits on the body background, behind the cards.
                crate::ui::effects::paint_grid(ui, &self.theme);
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        crate::ui::card_grid(ui, self);
                    });
            });

        // The settings modal floats above the panels; it is a free-floating
        // `egui::Window` and so takes the `Context`, not a nested `Ui`.
        crate::ui::settings::settings_modal(&ctx, self);

        // The scanline + vignette overlay paints last so it sits on top of
        // every panel and the modal. (The grid is drawn earlier, inside the
        // central panel, so the opaque cards cover it.)
        crate::ui::effects::paint_effects(&ctx, &self.theme);

        // Watchdog repaint a little past the refresh interval; new snapshots
        // already wake the UI via request_repaint from the reader thread, and
        // the blinking cursor needs steady frames regardless.
        ctx.request_repaint_after(std::time::Duration::from_millis(
            self.config.refresh.as_millis() as u64 + 500,
        ));
    }
}
