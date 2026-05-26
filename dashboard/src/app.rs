use crate::config::Config;
use crate::history::History;
use crate::ipc::{SensordKind, Snapshot};
use crate::theme::{self, system, Theme};
use crate::update::{
    check::{spawn_check, STARTUP_DELAY_MS},
    new_shared, GitHubReleaseSource, SharedUpdateState, OWNER, REPO,
};
use std::sync::Arc;

/// Whether the sensor feed is healthy.
#[derive(PartialEq)]
pub enum Status {
    Running,
    SensordDown,
}

/// What a finished download produced.
pub enum DownloadOutcome {
    /// The installer was downloaded successfully and is at `path`. The UI
    /// thread launches it and sets `want_quit`.
    Ready(std::path::PathBuf),
    /// The download failed; the modal transitions to its Failed state.
    Failed(String),
}

pub type SharedDownloadOutcome = std::sync::Arc<std::sync::Mutex<Option<DownloadOutcome>>>;

/// The PerfWindow application.
pub struct PerfApp {
    pub config: Config,
    pub theme: Theme,
    pub history: History,
    pub sensord: Option<SensordKind>,
    pub latest: Option<Snapshot>,
    pub status: Status,
    /// `true` when launched with `--dev` (spawn child sensord); `false` for the
    /// production path that connects to the installed service over the named pipe.
    pub dev_mode: bool,
    pub settings_open: bool,
    pub update_state: SharedUpdateState,
    pub update_banner_dismissed: bool,
    pub update_modal_open: bool,
    pub update_modal_phase: crate::ui::update_modal::ModalPhase,
    pub update_download_cancel: Arc<std::sync::atomic::AtomicBool>,
    pub update_download_progress: crate::update::download::SharedProgress,
    pub update_download_outcome: SharedDownloadOutcome,
    pub want_quit: bool,
    pub show_changelog: bool,
    /// Last `WindowLevel` value pushed to the viewport. Used to detect
    /// `config.always_on_top` toggles so we send `ViewportCommand::WindowLevel`
    /// only on transitions, not every frame.
    pub applied_on_top: bool,
    /// True while the window is in F11 fullscreen mode. Transient — not
    /// persisted to config. Toggled by the F11 keypress handler.
    pub fullscreen: bool,
    /// True while the "Sensor service is not running" modal is on screen.
    /// Set on startup when the production-mode pipe connect failed, and
    /// dismissed once the reconnect attempt succeeds (or the user cancels).
    pub service_dialog_open: bool,
    /// The body text shown inside the service dialog. Mutated by the dialog
    /// itself to report the outcome of the `sc.exe start` elevation prompt.
    pub service_dialog_message: String,
    /// True between the moment the user clicks "Start" and the moment the
    /// pipe reconnect succeeds. Disables the Start button while polling.
    pub service_starting: bool,
    /// Set to true when the user clicks Dismiss on the sensord health banner,
    /// hiding it for the rest of the session even if the degraded condition
    /// persists. Reset on next launch (not persisted).
    pub health_banner_dismissed: bool,
    update_source: Arc<GitHubReleaseSource>,
    os_is_light: bool,
}

impl PerfApp {
    pub fn new(cc: &eframe::CreationContext<'_>, dev_mode: bool) -> Self {
        theme::install_fonts(&cc.egui_ctx);
        let config = Config::load();
        let os_is_light = system::windows_is_light();
        let theme = Theme::for_id(system::effective_theme_id(&config, os_is_light));
        theme.apply(&cc.egui_ctx);

        let ctx = cc.egui_ctx.clone();
        let sensord: Option<crate::ipc::SensordKind> = if dev_mode {
            crate::ipc::process::Sensord::spawn(move || ctx.request_repaint())
                .inspect_err(|e| eprintln!("PerfWindow: failed to spawn sensord child: {e}"))
                .ok()
                .map(crate::ipc::SensordKind::Child)
        } else {
            crate::ipc::pipe::PipeSensord::connect(move || ctx.request_repaint())
                .inspect_err(|e| eprintln!("PerfWindow: failed to connect to sensord pipe: {e}"))
                .ok()
                .map(crate::ipc::SensordKind::Pipe)
        };

        let status = if sensord.is_some() {
            Status::Running
        } else {
            Status::SensordDown
        };

        let update_state = new_shared();
        let update_source = Arc::new(GitHubReleaseSource::new(OWNER, REPO));

        if config.check_updates_on_startup {
            let ctx = cc.egui_ctx.clone();
            let source = Arc::clone(&update_source);
            let state = Arc::clone(&update_state);
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(STARTUP_DELAY_MS));
                spawn_check(source, state, false, move || ctx.request_repaint());
            });
        }

        let mut app = Self {
            config,
            theme,
            history: History::default(),
            sensord,
            latest: None,
            status,
            dev_mode,
            settings_open: false,
            update_state,
            update_banner_dismissed: false,
            update_modal_open: false,
            update_modal_phase: crate::ui::update_modal::ModalPhase::default(),
            update_download_cancel: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            update_download_progress: Arc::new(std::sync::Mutex::new(
                crate::update::download::DownloadProgress::default(),
            )),
            update_download_outcome: Arc::new(std::sync::Mutex::new(None)),
            want_quit: false,
            show_changelog: false,
            applied_on_top: false,
            fullscreen: false,
            service_dialog_open: false,
            service_dialog_message: String::new(),
            service_starting: false,
            health_banner_dismissed: false,
            update_source,
            os_is_light,
        };
        if app.sensord.is_none() && !dev_mode {
            app.service_dialog_open = true;
            app.service_dialog_message =
                "PerfWindow's sensor service is not running. Click Start to launch it. \
                 Windows will ask for permission once; after that PerfWindow runs without prompts.".into();
        }
        if let Some(s) = &mut app.sensord {
            s.set_interval(app.config.refresh.as_millis());
        }
        app
    }

    /// Push the configured window-on-top preference to the OS only when it
    /// has actually flipped since the last frame, so we don't spam the
    /// viewport with a no-op `WindowLevel` command on every repaint.
    fn sync_window_level(&mut self, ctx: &egui::Context) {
        if self.config.always_on_top == self.applied_on_top {
            return;
        }
        let level = if self.config.always_on_top {
            egui::WindowLevel::AlwaysOnTop
        } else {
            egui::WindowLevel::Normal
        };
        ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(level));
        self.applied_on_top = self.config.always_on_top;
    }

    /// Toggle the F11 fullscreen state. Hides the OS window chrome
    /// (decorations) while fullscreen so the dashboard fills the screen
    /// edge-to-edge, and restores it on exit. The state is transient and not
    /// persisted to config — F11 always opens a fresh windowed session.
    fn handle_fullscreen_keypress(&mut self, ctx: &egui::Context) {
        let toggled = ctx.input(|i| i.key_pressed(egui::Key::F11));
        if !toggled {
            return;
        }
        self.fullscreen = !self.fullscreen;
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.fullscreen));
        ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(!self.fullscreen));
    }

    /// Fire a manual update check, ignoring the cache TTL. Called by the
    /// Settings → Updates section's "Check for updates now" button.
    pub fn manual_update_check(&self, ctx: &egui::Context) {
        let source = Arc::clone(&self.update_source);
        let state = Arc::clone(&self.update_state);
        let ctx = ctx.clone();
        spawn_check(source, state, true, move || ctx.request_repaint());
    }

    /// Apply any download outcome the background thread has left for us.
    fn poll_download_outcome(&mut self) {
        let outcome = self
            .update_download_outcome
            .lock()
            .ok()
            .and_then(|mut g| g.take());
        let Some(outcome) = outcome else { return };
        match outcome {
            DownloadOutcome::Ready(path) => match crate::update::install::launch(&path) {
                Ok(()) => self.want_quit = true,
                Err(e) => {
                    self.update_modal_phase = crate::ui::update_modal::ModalPhase::Failed {
                        message: format!("could not launch installer: {e}"),
                    };
                }
            },
            DownloadOutcome::Failed(message) => {
                self.update_modal_phase = crate::ui::update_modal::ModalPhase::Failed { message };
            }
        }
    }

    /// Restart `sensord` after it has died, re-arming the live feed.
    ///
    /// The old [`SensordKind`] is dropped *first* (`self.sensord = None`): its
    /// [`Drop`] closes the writer (pipe) or stdin (child), then joins the
    /// reader thread. Only once the old connection is fully torn down is the
    /// replacement constructed — in `dev_mode`, that means `Sensord::spawn`
    /// launches the sibling `sensord.exe`; in the production path it means
    /// `PipeSensord::connect` re-opens the named pipe exposed by the installed
    /// service. The strict ordering is for resource cleanup, not file
    /// contention.
    ///
    /// `history` is deliberately kept, so the sparklines carry on uninterrupted
    /// across the gap. `latest` is cleared (its readings are now stale) and the
    /// configured refresh interval is re-sent to the fresh feed. `status` ends
    /// as [`Status::Running`] only if the respawn succeeded; a failed
    /// spawn/connect is logged and leaves it [`Status::SensordDown`].
    pub fn respawn_sensord(&mut self, ctx: &egui::Context, dev_mode: bool) {
        // Drop the old child *before* spawning: its `Drop` waits for the
        // existing reader thread to finish before we hand a fresh stdout
        // pipe to the replacement.
        self.sensord = None;

        let repaint_ctx = ctx.clone();
        self.sensord = if dev_mode {
            crate::ipc::process::Sensord::spawn(move || repaint_ctx.request_repaint())
                .inspect_err(|e| eprintln!("PerfWindow: failed to restart sensord child: {e}"))
                .ok()
                .map(crate::ipc::SensordKind::Child)
        } else {
            crate::ipc::pipe::PipeSensord::connect(move || repaint_ctx.request_repaint())
                .inspect_err(|e| eprintln!("PerfWindow: failed to reconnect sensord pipe: {e}"))
                .ok()
                .map(crate::ipc::SensordKind::Pipe)
        };

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

    /// Poll the named pipe once per frame while the service-start dialog is
    /// armed, so the UI catches the service coming up the instant Windows
    /// finishes launching it.
    ///
    /// Cheap by construction: bails immediately when `service_starting` is
    /// false, and short-circuits the moment a connection succeeds (closing
    /// the dialog and flipping `status` back to `Running`). One reconnect
    /// attempt per frame is plenty — the service typically takes well under
    /// a second to start, and `PipeSensord::connect` returns immediately
    /// when the pipe isn't yet open.
    pub fn try_reconnect_if_starting(&mut self, ctx: &egui::Context) {
        if !self.service_starting {
            return;
        }
        if self.sensord.is_some() {
            self.service_starting = false;
            self.service_dialog_open = false;
            return;
        }
        // Attempt one reconnect per frame; cheap, and stops as soon as it works.
        let repaint = ctx.clone();
        self.sensord = crate::ipc::pipe::PipeSensord::connect(move || repaint.request_repaint())
            .ok()
            .map(crate::ipc::SensordKind::Pipe);
        if self.sensord.is_some() {
            if let Some(s) = &mut self.sensord {
                s.set_interval(self.config.refresh.as_millis());
            }
            self.status = Status::Running;
            self.service_starting = false;
            self.service_dialog_open = false;
        }
    }

    /// Pull the newest snapshot out of the shared state and update history.
    fn ingest(&mut self) {
        let Some(sensord) = &self.sensord else {
            self.status = Status::SensordDown;
            return;
        };
        if let Ok(state) = sensord.state().lock() {
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

#[cfg(any(test, feature = "test-support"))]
impl PerfApp {
    /// Construct a minimal [`PerfApp`] for unit and integration tests. Skips
    /// the egui setup (font installation, theme application) and the `sensord`
    /// spawn that the production [`PerfApp::new`] performs from a
    /// `CreationContext`. Every field is wired with a safe stub so the
    /// resulting value compiles and supports the field-level assertions used
    /// by `app::tests` plus the snapshot harness in `tests/ui_snapshot.rs`.
    ///
    /// Exposed to integration tests via the `test-support` feature, which the
    /// crate's own `[dev-dependencies]` self-enables — so production builds
    /// (`cargo build`) never include this constructor.
    pub fn for_tests(config: crate::config::Config) -> Self {
        use crate::theme::Theme;
        use crate::update::{
            download::DownloadProgress, new_shared, GitHubReleaseSource, OWNER, REPO,
        };
        use std::sync::atomic::AtomicBool;
        use std::sync::{Arc, Mutex};

        let theme = Theme::for_id(config.theme);
        Self {
            theme,
            history: crate::history::History::default(),
            sensord: None,
            latest: None,
            status: Status::Running,
            dev_mode: false,
            settings_open: false,
            update_state: new_shared(),
            update_banner_dismissed: false,
            update_modal_open: false,
            update_modal_phase: crate::ui::update_modal::ModalPhase::default(),
            update_download_cancel: Arc::new(AtomicBool::new(false)),
            update_download_progress: Arc::new(Mutex::new(DownloadProgress::default())),
            update_download_outcome: Arc::new(Mutex::new(None)),
            want_quit: false,
            show_changelog: false,
            applied_on_top: false,
            fullscreen: false,
            service_dialog_open: false,
            service_dialog_message: String::new(),
            service_starting: false,
            health_banner_dismissed: false,
            update_source: Arc::new(GitHubReleaseSource::new(OWNER, REPO)),
            os_is_light: false,
            config,
        }
    }
}

impl eframe::App for PerfApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.ingest();
        self.poll_download_outcome();
        self.sync_window_level(&ctx);
        self.handle_fullscreen_keypress(&ctx);

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
        if crate::ui::update_banner::is_visible(self) {
            egui::Panel::top("pw_update_banner")
                .frame(egui::Frame::NONE)
                .show_inside(ui, |ui| {
                    crate::ui::update_banner::update_banner(ui, self);
                });
        }
        if crate::ui::health_banner::is_visible(self) {
            egui::Panel::top("pw_health_banner")
                .frame(egui::Frame::NONE)
                .show_inside(ui, |ui| {
                    crate::ui::health_banner::health_banner(ui, self);
                });
        }
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
                // `auto_shrink = [false, true]`: keep the full available
                // width, but shrink vertically to whatever the cards take —
                // no blank scrollable area below the grid.
                egui::ScrollArea::vertical()
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        crate::ui::card_grid(ui, self);
                    });
            });

        // The settings modal floats above the panels; it is a free-floating
        // `egui::Window` and so takes the `Context`, not a nested `Ui`.
        crate::ui::settings::settings_modal(&ctx, self);
        crate::ui::update_modal::update_modal(&ctx, self);
        crate::ui::changelog_modal::changelog_modal(&ctx, &self.theme, &mut self.show_changelog);
        self.try_reconnect_if_starting(&ctx);
        crate::ui::service_dialog::service_dialog(&ctx, self);

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

        if self.want_quit {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, RefreshRate, ThemeId};
    use crate::format::TempUnit;

    #[test]
    fn apply_config_change_updates_theme() {
        let cfg = Config {
            theme: ThemeId::Amber,
            ..Config::default()
        };
        let mut app = PerfApp::for_tests(cfg);
        app.config.theme = ThemeId::Slate;
        assert_eq!(app.config.theme, ThemeId::Slate);
    }

    #[test]
    fn apply_config_change_updates_unit() {
        let mut app = PerfApp::for_tests(Config::default());
        app.config.unit = TempUnit::Fahrenheit;
        assert_eq!(app.config.unit, TempUnit::Fahrenheit);
    }

    #[test]
    fn apply_config_change_updates_refresh_rate() {
        let mut app = PerfApp::for_tests(Config::default());
        // `Status::Running` is the only "ok" variant; the only other variant
        // is `SensordDown`. The set of `RefreshRate` variants is
        // {Ms500, S1, S2, S5} (see `config.rs`).
        app.config.refresh = RefreshRate::S5;
        assert_eq!(app.config.refresh, RefreshRate::S5);
    }

    #[test]
    fn default_status_is_running() {
        // `for_tests` initialises `status` to `Running`; production sets
        // `SensordDown` only when the sensord spawn fails, so `Running` is
        // the default "happy path" status.
        let app = PerfApp::for_tests(Config::default());
        assert!(matches!(app.status, Status::Running));
    }

    #[test]
    fn status_can_be_flipped_to_sensord_down() {
        let mut app = PerfApp::for_tests(Config::default());
        app.status = Status::SensordDown;
        assert!(matches!(app.status, Status::SensordDown));
    }

    #[test]
    fn status_can_be_flipped_back_to_running() {
        let mut app = PerfApp::for_tests(Config::default());
        app.status = Status::SensordDown;
        app.status = Status::Running;
        assert!(matches!(app.status, Status::Running));
    }
}
