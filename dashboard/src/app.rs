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
#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    /// The connect state machine is running. `Sensord` is `None` until
    /// the machine produces a `Ready(PipeSensord)` event.
    Connecting(crate::ipc::connect::ConnectPhase),
    /// `sensord` is producing snapshots.
    Running,
    /// `sensord` was alive and then died (the reader thread saw EOF).
    SensordDown,
}

/// What a finished download produced.
pub enum DownloadOutcome {
    /// The installer was downloaded successfully and is at `path`, with the
    /// SHA-256 it had when the download verified it. The UI thread re-reads
    /// and re-checks the file, then launches it and sets `want_quit` — but only
    /// while the modal is still open on its Downloading screen.
    Ready(std::path::PathBuf, String),
    /// The user cancelled; the modal has already routed itself back to
    /// Confirm, so this is consumed silently.
    Cancelled,
    /// The download failed; the modal transitions to its Failed state.
    Failed(String),
}

/// Outcome slot shared with the download worker. The `u64` is the attempt
/// generation stamped by `start_update_download`; results from abandoned
/// attempts are recognised and discarded by [`PerfApp::poll_download_outcome`].
pub type SharedDownloadOutcome = std::sync::Arc<std::sync::Mutex<Option<(u64, DownloadOutcome)>>>;

/// The full window's minimum inner size. `main.rs` sets it at startup; leaving
/// the mini strip puts it back after the strip lowered it to fit its 28 px
/// overlay.
pub const MIN_INNER_SIZE: [f32; 2] = [720.0, 500.0];

/// The parts of the window's rectangle mini-strip mode has to put back when it
/// expands into the full window again.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RestoreRect {
    /// Outer (decorated) top-left corner, in monitor space.
    pub outer_pos: Option<egui::Pos2>,
    /// Client-area size.
    pub inner_size: Option<egui::Vec2>,
}

/// The PerfWindow application.
pub struct PerfApp {
    pub config: Config,
    pub theme: Theme,
    pub history: History,
    pub sensord: Option<SensordKind>,
    pub latest: Option<Snapshot>,
    /// `latest` was taken while sensord was still enabling sensor categories;
    /// see [`crate::ipc::SensorState::latest_partial`]. The grid waits for the
    /// first full snapshot so it is laid out once, for every card.
    pub latest_partial: bool,
    /// Active monitors, read in this process. `sensord` cannot see the
    /// interactive desktop from session 0, so the footer gets its display
    /// info from here instead of the snapshot.
    pub display_cache: crate::displays::DisplayCache,
    pub status: Status,
    /// Receiver for events emitted by the [`crate::ipc::connect`] state
    /// machine. `Some` while connecting, `None` once `Ready` has been
    /// consumed (or on the dev-mode path that bypasses the state machine).
    pub connect_rx: Option<std::sync::mpsc::Receiver<crate::ipc::connect::ConnectEvent>>,
    /// `true` when launched with `--dev` (spawn child sensord); `false` for the
    /// production path that connects to the installed service over the named pipe.
    pub dev_mode: bool,
    pub settings_open: bool,
    pub update_state: SharedUpdateState,
    pub update_banner_dismissed: bool,
    pub update_modal_open: bool,
    pub update_modal_phase: crate::ui::update_modal::ModalPhase,
    /// Cancel flag of the CURRENT download attempt. Replaced wholesale on
    /// every attempt: clearing a shared flag would un-cancel an orphaned
    /// worker from an earlier attempt.
    pub update_download_cancel: Arc<std::sync::atomic::AtomicBool>,
    /// Progress of the CURRENT download attempt; replaced per attempt so an
    /// orphaned worker cannot write into the live attempt's counter.
    pub update_download_progress: crate::update::download::SharedProgress,
    pub update_download_outcome: SharedDownloadOutcome,
    /// Attempt counter; outcomes carrying an older generation are stale.
    pub update_download_generation: u64,
    pub want_quit: bool,
    pub show_changelog: bool,
    /// Version whose section the changelog viewer opens on when it was opened
    /// by the post-update "what's new" flow; `None` shows the whole changelog.
    /// Cleared when the footer's version label opens the full log.
    pub changelog_version: Option<String>,
    /// True once the post-update viewer was expanded with "Show all versions".
    pub changelog_show_all: bool,
    /// Last `WindowLevel` value pushed to the viewport. Used to detect
    /// `config.always_on_top` toggles so we send `ViewportCommand::WindowLevel`
    /// only on transitions, not every frame.
    pub applied_on_top: bool,
    /// Cached zoom-to-fit decision; see [`crate::ui::fit`]. `None` until the
    /// first frame computes it. Holds the input [`crate::ui::fit::FitKey`] the
    /// decision was taken from, so it is rebuilt only when the physical window
    /// size, the OS scale factor or the card set changes.
    pub(crate) grid_fit: Option<(crate::ui::fit::FitKey, crate::ui::fit::Plan)>,
    /// True while the window is in F11 fullscreen mode. Transient — not
    /// persisted to config. Toggled by the F11 keypress handler.
    pub fullscreen: bool,
    /// Raw `HWND` of the dashboard window, captured from the eframe creation
    /// context. `None` in headless tests (and on platforms where the handle is
    /// unavailable), where strip mode runs without an overlay placement.
    pub window_hwnd: Option<isize>,
    /// The live overlay: `Some` while the strip window is placed. Its `Drop`
    /// puts back the extended-style bits the strip took.
    pub strip: Option<crate::strip_window::StripWindow>,
    /// The window rectangle captured when strip mode was entered.
    pub strip_restore: Option<RestoreRect>,
    /// Runtime guard for the viewport/Win32 side of strip mode. The mode
    /// itself is `config.mini_strip`; this says whether that state has already
    /// been applied to the window this launch, which is what lets a launch
    /// straight into strip mode save the window rectangle before the strip is
    /// placed over it.
    strip_applied: bool,
    /// The overlay is placed one frame after strip mode is entered: the
    /// viewport commands that drop the decorations and the minimum size are
    /// applied at the end of the frame that sends them, and placing before
    /// that would have winit clamp the 28 px strip to the window's minimum.
    strip_place_after_pass: Option<u64>,
    /// `ctx.input(|i| i.time)` of the last overlay refresh, so the window is
    /// re-checked about once a second and not every frame.
    strip_refreshed_at: Option<f64>,
    /// Set to true when the user clicks Dismiss on the sensord health banner,
    /// hiding it for the rest of the session even if the degraded condition
    /// persists. Reset on next launch (not persisted).
    pub health_banner_dismissed: bool,
    /// Timestamp of the most recent transition into [`Status::Running`]. Used
    /// by [`Self::ingest`] to detect a "running but never got a snapshot"
    /// stall: if a sensord client connects but dies before the first NDJSON
    /// line arrives, the alive flag may not flip cleanly before the OS reaps
    /// the reader thread, leaving the dashboard stuck on the loading screen.
    /// Demotion to `SensordDown` is silence-based (see [`startup_stalled`]):
    /// progress lines from a slow staged init keep the session alive, so a
    /// long hardware enumeration is never mistaken for a dead service.
    pub running_since: Option<std::time::Instant>,
    update_source: Arc<GitHubReleaseSource>,
    os_is_light: bool,
}

/// How long a `Running` session may go without its first snapshot before the
/// silence watchdog is allowed to fire at all.
const STARTUP_WATCHDOG: std::time::Duration = std::time::Duration::from_secs(120);
/// How long the pipe must have carried no line at all (snapshot or progress)
/// before the watchdog treats the feed as dead.
const LINE_SILENCE: std::time::Duration = std::time::Duration::from_secs(20);

/// How often strip mode re-checks its monitor and re-asserts the topmost band.
/// Cheap enough to be idempotent, slow enough that a steady desktop costs one
/// `GetWindowRect` a second.
const STRIP_REFRESH_SECS: f64 = 1.0;

/// Decide whether a first-snapshot-pending session has truly stalled. Pure so
/// the timing matrix is unit-testable with fabricated instants.
fn startup_stalled(
    running_since: Option<std::time::Instant>,
    last_line_at: Option<std::time::Instant>,
    now: std::time::Instant,
) -> bool {
    let Some(started) = running_since else {
        return false;
    };
    if now.saturating_duration_since(started) <= STARTUP_WATCHDOG {
        return false;
    }
    match last_line_at {
        Some(line) => now.saturating_duration_since(line) > LINE_SILENCE,
        None => true,
    }
}

/// The dashboard window's raw `HWND`, taken from the eframe creation context
/// through `raw-window-handle` (which is also how winit hands the handle to
/// eframe). `None` when the handle is unavailable — headless tests, or a
/// platform whose handle is not Win32.
#[cfg(windows)]
fn creation_window_hwnd(cc: &eframe::CreationContext<'_>) -> Option<isize> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match cc.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(handle) => Some(handle.hwnd.get()),
        _ => None,
    }
}

#[cfg(not(windows))]
fn creation_window_hwnd(_cc: &eframe::CreationContext<'_>) -> Option<isize> {
    None
}

/// The window rectangle egui last saw, captured so the strip can put the full
/// window back where it was. Either half is `None` until egui has seen a
/// frame; the strip then restores only the half it knows.
fn capture_window_rect(ctx: &egui::Context) -> RestoreRect {
    ctx.input(|i| {
        let viewport = i.viewport();
        RestoreRect {
            outer_pos: viewport.outer_rect.map(|rect| rect.min),
            inner_size: viewport.inner_rect.map(|rect| rect.size()),
        }
    })
}

/// `true` when this launch follows an upgrade from `previous`: `None` means the
/// config predates `last_run_version`, a semver-older value counts, and an
/// unparseable value is treated as "different" so the notes still show.
fn upgraded_from(previous: Option<&str>) -> bool {
    let current = env!("CARGO_PKG_VERSION");
    match previous {
        None => true,
        Some(prev) => match (
            semver::Version::parse(prev),
            semver::Version::parse(current),
        ) {
            (Ok(prev), Ok(cur)) => prev < cur,
            _ => prev != current,
        },
    }
}

/// Debug-only escape hatch: `PERFWINDOW_SHOW_UPDATED_NOTES=1` forces the
/// post-update notes even on a first run. Compiled out entirely in release, so
/// the variable name never reaches the shipped binary.
#[cfg(debug_assertions)]
fn force_updated_notes() -> bool {
    std::env::var("PERFWINDOW_SHOW_UPDATED_NOTES").is_ok_and(|value| value == "1")
}

#[cfg(not(debug_assertions))]
fn force_updated_notes() -> bool {
    false
}

impl PerfApp {
    pub fn new(cc: &eframe::CreationContext<'_>, dev_mode: bool) -> Self {
        theme::install_fonts(&cc.egui_ctx);
        // A config file that already exists predates `last_run_version`, so its
        // presence marks a launch after an update even when the field is None.
        let config_file_existed = Config::path().is_some_and(|p| p.exists());
        let config = Config::load();
        let previous_version = config.last_run_version.clone();
        let os_is_light = system::windows_is_light();
        let theme = Theme::for_id(system::effective_theme_id(&config, os_is_light));
        theme.apply(&cc.egui_ctx);

        let ctx_clone = cc.egui_ctx.clone();
        let (status, sensord, connect_rx): (Status, Option<crate::ipc::SensordKind>, _) =
            if dev_mode {
                let s = crate::ipc::process::Sensord::spawn(move || ctx_clone.request_repaint())
                    .inspect_err(|e| eprintln!("PerfWindow: failed to spawn sensord child: {e}"))
                    .ok()
                    .map(crate::ipc::SensordKind::Child);
                let st = if s.is_some() {
                    Status::Running
                } else {
                    Status::SensordDown
                };
                (st, s, None)
            } else {
                let (_phase, rx) =
                    crate::ipc::connect::spawn_connect(move || ctx_clone.request_repaint());
                (
                    Status::Connecting(crate::ipc::connect::ConnectPhase::OpeningPipe),
                    None,
                    Some(rx),
                )
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

        // Dev mode reaches Running synchronously inside `new`; record the
        // start time so the no-first-snapshot deadline applies there too.
        let running_since = matches!(status, Status::Running).then(std::time::Instant::now);

        // The overlay placement needs the window's raw Win32 handle.
        let window_hwnd = creation_window_hwnd(cc);

        let mut app = Self {
            config,
            theme,
            history: History::default(),
            sensord,
            latest: None,
            latest_partial: false,
            display_cache: crate::displays::DisplayCache::new(),
            status,
            connect_rx,
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
            update_download_generation: 0,
            want_quit: false,
            show_changelog: false,
            changelog_version: None,
            changelog_show_all: false,
            applied_on_top: false,
            grid_fit: None,
            fullscreen: false,
            window_hwnd,
            strip: None,
            strip_restore: None,
            strip_applied: false,
            strip_place_after_pass: None,
            strip_refreshed_at: None,
            health_banner_dismissed: false,
            running_since,
            update_source,
            os_is_light,
        };
        if let Some(s) = &mut app.sensord {
            s.set_interval(app.config.refresh.as_millis());
        }
        app.record_run(config_file_existed, previous_version.as_deref());
        app
    }

    /// First-launch-after-an-update hook. When the user upgraded from an older
    /// version (or a build that predates `last_run_version`) and the embedded
    /// changelog has a section for the running version, open the viewer on that
    /// section. `last_run_version` is written and saved on every launch, even
    /// when nothing was shown.
    fn record_run(&mut self, ran_before: bool, previous: Option<&str>) {
        let current = env!("CARGO_PKG_VERSION");
        if crate::ui::changelog_modal::version_section(
            crate::ui::changelog_modal::CHANGELOG_TEXT,
            current,
        )
        .is_some()
            && (force_updated_notes() || (ran_before && upgraded_from(previous)))
        {
            self.changelog_version = Some(current.to_string());
            self.show_changelog = true;
            self.changelog_show_all = false;
        }
        self.config.last_run_version = Some(current.to_string());
        self.config.save();
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
    ///
    /// Acting on a result requires the modal to still be open on its
    /// Downloading screen AND the outcome to carry the current attempt
    /// generation: a worker orphaned by an X-close, a cancel or a retry must
    /// neither surprise-launch the installer under a closed modal nor paint
    /// a stale FAILED screen over a newer attempt.
    fn poll_download_outcome(&mut self) {
        use crate::ui::update_modal::ModalPhase;
        let outcome = self
            .update_download_outcome
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        let Some((generation, outcome)) = outcome else {
            return;
        };
        let current = generation == self.update_download_generation;
        let downloading = self.update_modal_open
            && matches!(self.update_modal_phase, ModalPhase::Downloading { .. });
        match outcome {
            DownloadOutcome::Ready(path, hash) => {
                if !(current && downloading) {
                    let _ = std::fs::remove_file(&path);
                    return;
                }
                // Re-verify immediately before the hand-off: the file lives in
                // a user-writable directory and may have been replaced since
                // the download's own check.
                if !crate::update::install::verify(&path, &hash) {
                    let _ = std::fs::remove_file(&path);
                    self.update_modal_phase = ModalPhase::Failed {
                        message: "the installer changed after it was verified".into(),
                    };
                    return;
                }
                match crate::update::install::launch(&path) {
                    Ok(()) => self.want_quit = true,
                    Err(e) => {
                        self.update_modal_phase = ModalPhase::LaunchFailed { message: e };
                    }
                }
            }
            // A cancel was the user's own action (or a close); never surface
            // it as a failure.
            DownloadOutcome::Cancelled => {}
            DownloadOutcome::Failed(message) => {
                if current && downloading {
                    self.update_modal_phase = ModalPhase::Failed { message };
                }
            }
        }
    }

    /// Dev-mode only: relaunch the sibling `sensord.exe` child after it has
    /// died. The production path never calls this — the error overlay routes
    /// non-dev RESPAWN through [`Self::restart_connect`] so the connect state
    /// machine (with its UAC/service-start step) owns recovery.
    ///
    /// The old child is dropped *first* (`self.sensord = None`): its [`Drop`]
    /// queues the shutdown message and reaps the process before a fresh
    /// stdout pipe is handed to the replacement. `history` is deliberately
    /// kept, so the sparklines carry on uninterrupted across the gap.
    /// `latest` is cleared (its readings are now stale) and the configured
    /// refresh interval is re-sent to the fresh feed. `status` ends as
    /// [`Status::Running`] only if the respawn succeeded; a failed spawn is
    /// logged and leaves it [`Status::SensordDown`].
    pub fn respawn_sensord(&mut self, ctx: &egui::Context) {
        self.sensord = None;

        let repaint_ctx = ctx.clone();
        self.sensord = crate::ipc::process::Sensord::spawn(move || repaint_ctx.request_repaint())
            .inspect_err(|e| eprintln!("PerfWindow: failed to restart sensord child: {e}"))
            .ok()
            .map(crate::ipc::SensordKind::Child);

        self.status = if self.sensord.is_some() {
            Status::Running
        } else {
            Status::SensordDown
        };
        self.latest = None;
        self.running_since = matches!(self.status, Status::Running).then(std::time::Instant::now);
        if let Some(sensord) = &mut self.sensord {
            sensord.set_interval(self.config.refresh.as_millis());
        }
    }

    /// Re-apply the current `config` after the user changes a visual setting.
    ///
    /// Recomputes the effective theme (honouring `follow_windows`) and pushes
    /// it into egui's visuals. The caller is responsible for persisting
    /// `config` via [`Config::save`].
    pub fn apply_config_change(&mut self, ctx: &egui::Context) {
        self.theme = Theme::for_id(system::effective_theme_id(&self.config, self.os_is_light));
        self.theme.apply(ctx);
    }

    /// Forward the configured refresh interval to `sensord`.
    ///
    /// Deliberately separate from [`Self::apply_config_change`]: unrelated
    /// theme/unit/display clicks have no business touching the control
    /// channel. The send itself is a non-blocking queue onto the sensord
    /// writer thread.
    pub fn apply_refresh_change(&mut self) {
        if let Some(sensord) = &mut self.sensord {
            sensord.set_interval(self.config.refresh.as_millis());
        }
    }

    /// Enter mini-strip mode: borderless and resized to a thin bar, which
    /// [`Self::place_strip`] then parks on the top edge of the strip's monitor
    /// as a topmost overlay. Nothing is reserved, so maximized windows and
    /// games still see the monitor's full size.
    ///
    /// The window's current rectangle is captured first, so leaving strip mode
    /// puts the full window back where it was. The mode itself is
    /// `config.mini_strip`, which is persisted here: the strip is a window
    /// mode, not a session state.
    pub fn enter_strip(&mut self, ctx: &egui::Context) {
        if self.strip_applied {
            return;
        }
        if self.strip_restore.is_none() {
            self.strip_restore = Some(capture_window_rect(ctx));
        }
        // The settings modal has no home in a 28 px strip.
        self.settings_open = false;
        if self.fullscreen {
            // A fullscreen window has no chrome to hide, and its pinned
            // top-left corner would fight the strip's placement; return it to
            // windowed sizing before the strip moves it.
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
            self.fullscreen = false;
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Resizable(false));
        // winit enforces the minimum size on every `SetWindowPos`, so the
        // strip could never be 28 px tall with the full window's minimum.
        ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(egui::Vec2::splat(1.0)));
        ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
            egui::WindowLevel::AlwaysOnTop,
        ));
        // Strip mode owns the Z-order from here — it re-asserts `HWND_TOPMOST`
        // every refresh — so `sync_window_level` stays out of the way until the
        // full window is back.
        self.applied_on_top = true;
        self.strip_place_after_pass = Some(ctx.cumulative_pass_nr());
        self.strip_refreshed_at = None;
        self.strip_applied = true;
        self.config.mini_strip = true;
        self.config.save();
        ctx.request_repaint();
    }

    /// Take over the window and place it on the strip's band, on the frame
    /// after [`Self::enter_strip`] sent its viewport commands.
    fn place_strip(&mut self) {
        self.strip_place_after_pass = None;
        self.strip = crate::strip_window::StripWindow::attach(
            self.window_hwnd.unwrap_or(0),
            self.config.mini_strip_monitor.as_deref(),
        );
        // Persist the monitor the strip actually landed on: the configured name
        // may have been stale, and the next launch should use the real one.
        if let Some(device) = self.strip.as_ref().and_then(|strip| strip.device_name()) {
            if self.config.mini_strip_monitor.as_deref() != Some(device) {
                self.config.mini_strip_monitor = Some(device.to_owned());
                self.config.save();
            }
        }
    }

    /// Leave mini-strip mode: drop the overlay (restoring the extended style),
    /// restore the window chrome and the saved rectangle, re-apply the user's
    /// always-on-top preference and bring the full window back in front.
    pub fn leave_strip(&mut self, ctx: &egui::Context) {
        // Restore the extended style before the decorations and the size come
        // back: winit rewrites the whole style word when it re-applies its own
        // flags.
        if let Some(mut session) = self.strip.take() {
            session.release();
        }
        self.strip_place_after_pass = None;
        self.strip_refreshed_at = None;
        let level = if self.config.always_on_top {
            egui::WindowLevel::AlwaysOnTop
        } else {
            egui::WindowLevel::Normal
        };
        ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Resizable(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(egui::Vec2::from(
            MIN_INNER_SIZE,
        )));
        ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(level));
        self.applied_on_top = self.config.always_on_top;
        if let Some(saved) = self.strip_restore.take() {
            if let Some(pos) = saved.outer_pos {
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(pos));
            }
            if let Some(size) = saved.inner_size {
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size));
            }
        }
        // The restored window can be behind whatever covered the strip; asking
        // for focus brings it back to the front.
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        self.strip_applied = false;
        self.config.mini_strip = false;
        self.config.save();
    }

    /// `Ctrl+M` in the full window enters strip mode. The strip's own `Ctrl+M`
    /// is handled by [`Self::strip_frame`], which has different modals to
    /// close and a different window to restore.
    fn handle_strip_hotkey(&mut self, ctx: &egui::Context) {
        let pressed = ctx.input(|i| i.focused && i.modifiers.ctrl && i.key_pressed(egui::Key::M));
        if pressed {
            self.enter_strip(ctx);
        }
    }

    /// One frame of strip mode: keep the overlay on its monitor and on top,
    /// draw the strip row and act on the control the user pressed.
    fn strip_frame(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        // `Ctrl+M` with the strip focused is the keyboard route back to the
        // full window.
        let expand = ctx.input(|i| i.focused && i.modifiers.ctrl && i.key_pressed(egui::Key::M));
        if expand {
            self.leave_strip(ctx);
            return;
        }
        if self
            .strip_place_after_pass
            .is_some_and(|sent| ctx.cumulative_pass_nr() > sent)
        {
            self.place_strip();
        } else if self.strip_place_after_pass.is_some() {
            ctx.request_repaint();
        }

        // Recompute the band and re-assert topmost about once a second: that
        // keeps up with a monitor that went away, a resolution or DPI change,
        // and another topmost window that came up later. The repaint request
        // keeps the check running while nothing else asks for frames.
        let now = ctx.input(|i| i.time);
        let due = self
            .strip_refreshed_at
            .is_none_or(|last| now - last >= STRIP_REFRESH_SECS);
        if due {
            if let Some(session) = &mut self.strip {
                session.refresh();
            }
            self.strip_refreshed_at = Some(now);
        }
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(STRIP_REFRESH_SECS));

        let action = egui::CentralPanel::default()
            // The strip row paints its own dimmed background once; an outer
            // fill here would stack a second translucent layer under it.
            .frame(egui::Frame::NONE)
            .show_inside(ui, |ui| {
                crate::ui::mini_strip::strip_row(
                    ui,
                    &self.theme,
                    self.latest.as_ref(),
                    &self.config,
                )
            })
            .inner;

        match action {
            crate::ui::mini_strip::StripAction::None => {}
            crate::ui::mini_strip::StripAction::Expand => self.leave_strip(ctx),
            crate::ui::mini_strip::StripAction::Close => self.want_quit = true,
            crate::ui::mini_strip::StripAction::Opacity(percent) => {
                self.config.background_opacity = percent;
                self.apply_config_change(ctx);
                self.config.save();
            }
        }
    }

    /// The zoom-to-fit plan for this frame, reusing the cached decision unless
    /// the physical window size, the OS scale factor or the card set changed.
    ///
    /// `avail_w` / `window_h` are the window's content size in zoom-1.0 points
    /// and `chrome` the chrome height in points; see [`crate::ui::fit::plan`].
    /// Caching here rather than in the renderer keeps the planner from running
    /// on every repaint while the window's geometry is unchanged.
    pub(crate) fn plan_for(
        &mut self,
        key: crate::ui::fit::FitKey,
        metrics: &[crate::ui::fit::CardMetrics],
        avail_w: f32,
        window_h: f32,
        chrome: f32,
    ) -> crate::ui::fit::Plan {
        if let Some((cached, plan)) = self.grid_fit {
            if cached == key {
                return plan;
            }
        }
        let plan = crate::ui::fit::plan(metrics, avail_w, window_h, chrome);
        self.grid_fit = Some((key, plan));
        plan
    }

    /// Pull the newest snapshot out of the shared state and update history.
    fn ingest(&mut self) {
        // While a connect machine runs it owns `status`. A stale (or absent)
        // sensord must not clobber Connecting with SensordDown every frame,
        // and a leftover dead feed must not be drained into `latest`.
        if self.connect_rx.is_some() {
            return;
        }
        let Some(sensord) = &self.sensord else {
            // Don't clobber Status::Connecting — it can outlive the receiver
            // (a `Failed` phase stays on screen after the worker has exited).
            if !matches!(self.status, Status::Connecting(_)) {
                self.status = Status::SensordDown;
                self.running_since = None;
            }
            return;
        };
        let mut last_line_at = None;
        if let Ok(mut state) = sensord.state().lock() {
            if !state.alive {
                self.status = Status::SensordDown;
                self.running_since = None;
            }
            last_line_at = state.last_line_at;
            // The reader thread is the sole writer of `latest`, so presence
            // means unconsumed: `take()` moves the snapshot out without a
            // clone, and two snapshots sharing one seconds-resolution `ts`
            // (sub-second refresh rates) both ingest.
            if let Some(snap) = state.latest.take() {
                if state.alive {
                    // A snapshot from a live feed proves sensord works —
                    // promote out of SensordDown so a late first snapshot
                    // (or a recovered feed) clears the error overlay.
                    self.status = Status::Running;
                    self.running_since = None;
                }
                self.history.record(&snap);
                self.latest = Some(snap);
                self.latest_partial = state.latest_partial;
            }
        }
        // Silence watchdog: a session that never produced a snapshot is
        // demoted only when the pipe has carried no line at all for a long
        // time — progress lines during staged sensor init keep it alive.
        if matches!(self.status, Status::Running)
            && self.latest.is_none()
            && startup_stalled(self.running_since, last_line_at, std::time::Instant::now())
        {
            eprintln!(
                "PerfWindow: no snapshot and a silent pipe past the watchdog; \
                 demoting to SensordDown"
            );
            self.status = Status::SensordDown;
            self.running_since = None;
        }
    }

    /// Latest staged-init progress reported by sensord, if any. The loading
    /// screen shows it until the first full snapshot is in; `None` against an
    /// old sensord that never emits progress lines.
    pub fn sensor_progress(&self) -> Option<crate::ipc::ProgressInfo> {
        self.sensord.as_ref()?.state().lock().ok()?.progress.clone()
    }

    /// Re-arm the connect state machine after the user clicks RETRY on the
    /// loading screen's failure card or RESPAWN on the error overlay (the
    /// production route). Ignored while a machine is already running:
    /// repeated clicks would spawn racing workers whose orphaned `Ready`
    /// would shut down the freshly started service.
    pub fn restart_connect(&mut self, ctx: &egui::Context) {
        if self.connect_rx.is_some() {
            return;
        }
        self.prepare_reconnect();
        let repaint = ctx.clone();
        let (_phase, rx) = crate::ipc::connect::spawn_connect(move || repaint.request_repaint());
        self.connect_rx = Some(rx);
    }

    /// Tear down the dead feed BEFORE a fresh connect machine spawns: a stale
    /// `PipeSensord` keeps handles on the old pipe instance open and would
    /// feed `ingest` a dead `alive` flag once the machine hands over; stale
    /// `latest` readings must not be painted under the new session either.
    fn prepare_reconnect(&mut self) {
        self.sensord = None;
        self.latest = None;
        self.status = Status::Connecting(crate::ipc::connect::ConnectPhase::OpeningPipe);
        self.running_since = None;
    }

    /// Drain pending connect events from the worker thread, advancing
    /// `status` and adopting the `PipeSensord` once it becomes available.
    /// Called once per frame from `eframe::App::ui`. Cheap when there are
    /// no events.
    pub fn poll_connect_events(&mut self) {
        use crate::ipc::connect::ConnectEvent;
        let Some(rx) = self.connect_rx.as_mut() else {
            return;
        };
        loop {
            match rx.try_recv() {
                Ok(ConnectEvent::Phase(p)) => {
                    self.status = Status::Connecting(p);
                }
                Ok(ConnectEvent::Ready(boxed)) => {
                    let mut s = crate::ipc::SensordKind::Pipe(*boxed);
                    s.set_interval(self.config.refresh.as_millis());
                    self.sensord = Some(s);
                    self.status = Status::Running;
                    self.running_since = Some(std::time::Instant::now());
                    self.connect_rx = None;
                    break;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.connect_rx = None;
                    break;
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
            latest_partial: false,
            display_cache: crate::displays::DisplayCache::empty(),
            status: Status::Running,
            connect_rx: None,
            dev_mode: false,
            settings_open: false,
            update_state: new_shared(),
            update_banner_dismissed: false,
            update_modal_open: false,
            update_modal_phase: crate::ui::update_modal::ModalPhase::default(),
            update_download_cancel: Arc::new(AtomicBool::new(false)),
            update_download_progress: Arc::new(Mutex::new(DownloadProgress::default())),
            update_download_outcome: Arc::new(Mutex::new(None)),
            update_download_generation: 0,
            want_quit: false,
            show_changelog: false,
            changelog_version: None,
            changelog_show_all: false,
            applied_on_top: false,
            grid_fit: None,
            fullscreen: false,
            window_hwnd: None,
            strip: None,
            strip_restore: None,
            strip_applied: false,
            strip_place_after_pass: None,
            strip_refreshed_at: None,
            health_banner_dismissed: false,
            running_since: None,
            update_source: Arc::new(GitHubReleaseSource::new(OWNER, REPO)),
            os_is_light: false,
            config,
        }
    }
}

impl eframe::App for PerfApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        // Follow-Windows must track a live OS light/dark switch; the poll is
        // registry-backed and throttled inside theme::system (~1 read / 2 s).
        if let Some(light) = system::os_light_flipped(self.os_is_light) {
            self.os_is_light = light;
            self.apply_config_change(&ctx);
        }
        self.ingest();
        self.display_cache.maybe_refresh(std::time::Instant::now());
        self.poll_connect_events();
        self.poll_download_outcome();

        // A launch whose config asked for the strip enters it here, on the
        // first frame: by now the window exists and its rectangle is known, so
        // it can be saved before the strip shrinks the window to its bar.
        if self.config.mini_strip && !self.strip_applied {
            self.enter_strip(&ctx);
        }

        if self.config.mini_strip {
            // Sensor polling, history and the update machinery above all keep
            // running; only the chrome is replaced by the strip row.
            self.strip_frame(&ctx, ui);
            ctx.request_repaint_after(std::time::Duration::from_millis(
                self.config.refresh.as_millis() as u64 + 500,
            ));
            if self.want_quit {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            return;
        }

        self.sync_window_level(&ctx);
        self.handle_fullscreen_keypress(&ctx);
        self.handle_strip_hotkey(&ctx);

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
            .frame(
                egui::Frame::NONE.fill(
                    self.theme
                        .surface(self.theme.bg, self.config.background_opacity),
                ),
            )
            .show_inside(ui, |ui| {
                // The faint grid sits on the body background, behind the cards.
                crate::ui::effects::paint_grid(ui, &self.theme, self.config.background_opacity);
                // No scroll area: the grid zooms itself to fit, and sheds
                // content when even the smallest zoom cannot (see `ui::fit`).
                let central = ui.max_rect();
                crate::ui::card_grid(ui, self);
                crate::ui::recorder::central(central);
            });

        // The settings modal floats above the panels; it is a free-floating
        // `egui::Window` and so takes the `Context`, not a nested `Ui`.
        crate::ui::settings::settings_modal(&ctx, self);
        crate::ui::update_modal::update_modal(&ctx, self);
        crate::ui::changelog_modal::changelog_modal(&ctx, self);

        // The scanline + vignette overlay paints last so it sits on top of
        // every panel and the modal. (The grid is drawn earlier, inside the
        // central panel, so the opaque cards cover it.)
        crate::ui::effects::paint_effects(&ctx, &self.theme, self.config.background_opacity);

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

    /// The OS window is created transparent (see `main.rs`) so the dimmed
    /// surface fills can show the desktop behind them. egui's own clear colour
    /// must therefore contribute nothing: any opaque clear would fill the
    /// transparent regions right back in. Both the glow and the wgpu backends
    /// hand these four floats straight to their clear call.
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        // Tell the worker to exit (over the pipe) before the dashboard
        // process tears down. Drop alone is unreliable during shutdown —
        // explicit shutdown() while the egui frame is still alive is
        // the canonical close path.
        if let Some(mut s) = self.sensord.take() {
            s.shutdown();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, RefreshRate, ThemeId};
    use crate::format::TempUnit;

    #[test]
    fn config_theme_field_is_writable() {
        let cfg = Config {
            theme: ThemeId::Amber,
            ..Config::default()
        };
        let mut app = PerfApp::for_tests(cfg);
        app.config.theme = ThemeId::Slate;
        assert_eq!(app.config.theme, ThemeId::Slate);
    }

    #[test]
    fn config_unit_field_is_writable() {
        let mut app = PerfApp::for_tests(Config::default());
        app.config.unit = TempUnit::Fahrenheit;
        assert_eq!(app.config.unit, TempUnit::Fahrenheit);
    }

    #[test]
    fn config_refresh_field_is_writable() {
        let mut app = PerfApp::for_tests(Config::default());
        app.config.refresh = RefreshRate::S5;
        assert_eq!(app.config.refresh, RefreshRate::S5);
    }

    #[test]
    fn default_status_is_running() {
        // `for_tests` initialises `status` to `Running` — the "happy path"
        // status. Production starts in `Connecting(OpeningPipe)` (named-pipe
        // path) or directly in `Running` (`--dev` child-spawn path).
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

    #[test]
    fn status_can_be_set_to_connecting() {
        use crate::ipc::connect::ConnectPhase;
        let mut app = PerfApp::for_tests(Config::default());
        app.status = Status::Connecting(ConnectPhase::OpeningPipe);
        assert!(matches!(app.status, Status::Connecting(_)));
    }

    #[test]
    fn ingest_preserves_connecting_when_sensord_is_none() {
        use crate::ipc::connect::ConnectPhase;
        let mut app = PerfApp::for_tests(Config::default());
        app.sensord = None;
        app.status = Status::Connecting(ConnectPhase::StartingService);
        app.ingest();
        assert!(matches!(app.status, Status::Connecting(_)));
    }

    #[test]
    fn ingest_demotes_running_to_sensord_down_when_sensord_is_none() {
        let mut app = PerfApp::for_tests(Config::default());
        app.sensord = None;
        app.status = Status::Running;
        app.ingest();
        assert!(matches!(app.status, Status::SensordDown));
    }

    use crate::ipc::{pipe::PipeSensord, SensorState, SensordKind, SharedState, Snapshot};
    use std::sync::{Arc, Mutex};

    fn test_snapshot(ts: i64, load: f64) -> Snapshot {
        crate::ipc::parse_snapshot(&format!(
            r#"{{"v":1,"ts":{ts},"cpu":{{"name":"X","load":{load}}}}}"#
        ))
        .expect("test snapshot must parse")
    }

    /// A `PerfApp` wired to a detached pipe client; tests drive the feed by
    /// mutating the returned shared state directly.
    fn app_with_feed(alive: bool) -> (PerfApp, SharedState) {
        let state: SharedState = Arc::new(Mutex::new(SensorState {
            alive,
            ..SensorState::default()
        }));
        let mut app = PerfApp::for_tests(Config::default());
        app.sensord = Some(SensordKind::Pipe(PipeSensord::detached(Arc::clone(&state))));
        (app, state)
    }

    #[test]
    fn two_snapshots_with_identical_ts_both_ingest() {
        let (mut app, state) = app_with_feed(true);
        state.lock().unwrap().latest = Some(test_snapshot(7, 10.0));
        app.ingest();
        assert_eq!(
            app.latest.as_ref().unwrap().cpu.as_ref().unwrap().load,
            Some(10.0)
        );

        // Same seconds-resolution `ts`: the old dedup dropped this one,
        // silently capping sub-second refresh rates at 1 Hz.
        state.lock().unwrap().latest = Some(test_snapshot(7, 20.0));
        app.ingest();
        assert_eq!(
            app.latest.as_ref().unwrap().cpu.as_ref().unwrap().load,
            Some(20.0)
        );
    }

    #[test]
    fn ingest_consumes_the_shared_snapshot_via_take() {
        let (mut app, state) = app_with_feed(true);
        state.lock().unwrap().latest = Some(test_snapshot(1, 1.0));
        app.ingest();
        assert!(state.lock().unwrap().latest.is_none());
        assert!(app.latest.is_some());
    }

    #[test]
    fn late_snapshot_promotes_sensord_down_to_running() {
        let (mut app, state) = app_with_feed(true);
        app.status = Status::SensordDown;
        state.lock().unwrap().latest = Some(test_snapshot(1, 1.0));
        app.ingest();
        assert!(matches!(app.status, Status::Running));
    }

    #[test]
    fn dead_feed_snapshot_is_ingested_but_does_not_promote() {
        let (mut app, state) = app_with_feed(false);
        app.status = Status::Running;
        state.lock().unwrap().latest = Some(test_snapshot(1, 1.0));
        app.ingest();
        assert!(matches!(app.status, Status::SensordDown));
        assert!(app.latest.is_some());
    }

    #[test]
    fn watchdog_does_not_fire_before_the_deadline() {
        let base = std::time::Instant::now();
        let now = base + std::time::Duration::from_secs(60);
        assert!(!startup_stalled(Some(base), None, now));
    }

    #[test]
    fn line_traffic_prevents_demotion() {
        let base = std::time::Instant::now();
        let now = base + std::time::Duration::from_secs(200);
        // Last progress line 5 s ago: the pipe is talking, keep waiting.
        let last_line = Some(base + std::time::Duration::from_secs(195));
        assert!(!startup_stalled(Some(base), last_line, now));
    }

    #[test]
    fn true_silence_demotes() {
        let base = std::time::Instant::now();
        let now = base + std::time::Duration::from_secs(200);
        // No line for 50 s, session 200 s old: the feed is dead.
        let last_line = Some(base + std::time::Duration::from_secs(150));
        assert!(startup_stalled(Some(base), last_line, now));
        // Never saw any line at all: equally dead.
        assert!(startup_stalled(Some(base), None, now));
    }

    #[test]
    fn watchdog_is_disarmed_without_a_running_since() {
        let now = std::time::Instant::now() + std::time::Duration::from_secs(500);
        assert!(!startup_stalled(None, None, now));
    }

    #[test]
    fn a_missing_previous_version_counts_as_an_upgrade() {
        assert!(upgraded_from(None));
    }

    #[test]
    fn only_a_strictly_older_previous_version_shows_the_notes() {
        assert!(upgraded_from(Some("0.11.0")));
        assert!(!upgraded_from(Some(env!("CARGO_PKG_VERSION"))));
        assert!(!upgraded_from(Some("99.0.0")));
    }

    #[test]
    fn an_unparseable_previous_version_is_treated_as_different() {
        assert!(upgraded_from(Some("old-build")));
        assert!(!upgraded_from(Some(env!("CARGO_PKG_VERSION"))));
    }

    #[test]
    fn ingest_is_inert_while_the_connect_machine_runs() {
        use crate::ipc::connect::ConnectPhase;
        let (mut app, state) = app_with_feed(false);
        let (_tx, rx) = std::sync::mpsc::channel();
        app.connect_rx = Some(rx);
        app.status = Status::Connecting(ConnectPhase::OpeningPipe);
        state.lock().unwrap().latest = Some(test_snapshot(1, 1.0));
        app.ingest();
        // The machine owns `status`; the dead feed neither demotes it nor
        // leaks its stale snapshot into the app.
        assert!(matches!(app.status, Status::Connecting(_)));
        assert!(app.latest.is_none());
        assert!(state.lock().unwrap().latest.is_some());
    }

    #[test]
    fn restart_connect_is_ignored_while_a_machine_is_running() {
        let (mut app, state) = app_with_feed(true);
        state.lock().unwrap().latest = Some(test_snapshot(1, 1.0));
        app.ingest();
        let (_tx, rx) = std::sync::mpsc::channel();
        app.connect_rx = Some(rx);
        app.restart_connect(&egui::Context::default());
        // Neither torn down nor respawned: the running machine keeps both
        // the feed and the receiver it will replace on `Ready`.
        assert!(app.sensord.is_some());
        assert!(app.latest.is_some());
        assert!(app.connect_rx.is_some());
    }

    #[test]
    fn prepare_reconnect_clears_the_stale_feed_first() {
        let (mut app, state) = app_with_feed(true);
        state.lock().unwrap().latest = Some(test_snapshot(1, 1.0));
        app.ingest();
        assert!(app.sensord.is_some() && app.latest.is_some());
        app.prepare_reconnect();
        assert!(app.sensord.is_none());
        assert!(app.latest.is_none());
        assert!(matches!(app.status, Status::Connecting(_)));
        assert!(app.running_since.is_none());
    }

    use crate::ui::update_modal::ModalPhase;

    fn temp_installer(name: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(name);
        std::fs::write(&path, b"not really an installer").expect("test file writes");
        path
    }

    fn put_outcome(app: &PerfApp, generation: u64, outcome: DownloadOutcome) {
        *app.update_download_outcome.lock().unwrap() = Some((generation, outcome));
    }

    #[test]
    fn ready_outcome_with_modal_closed_is_discarded_and_file_deleted() {
        let mut app = PerfApp::for_tests(Config::default());
        let path = temp_installer("pw-poll-closed-modal.exe");
        app.update_modal_open = false;
        put_outcome(
            &app,
            app.update_download_generation,
            DownloadOutcome::Ready(path.clone(), String::new()),
        );
        app.poll_download_outcome();
        assert!(!app.want_quit, "a closed modal must never launch + quit");
        assert!(!path.exists(), "the unwanted installer must be deleted");
        assert!(matches!(app.update_modal_phase, ModalPhase::Confirm));
    }

    #[test]
    fn ready_outcome_outside_downloading_phase_is_discarded() {
        let mut app = PerfApp::for_tests(Config::default());
        let path = temp_installer("pw-poll-confirm-phase.exe");
        // Modal open but back on Confirm (user cancelled a beat earlier).
        app.update_modal_open = true;
        app.update_modal_phase = ModalPhase::Confirm;
        put_outcome(
            &app,
            app.update_download_generation,
            DownloadOutcome::Ready(path.clone(), String::new()),
        );
        app.poll_download_outcome();
        assert!(!app.want_quit);
        assert!(!path.exists());
        assert!(matches!(app.update_modal_phase, ModalPhase::Confirm));
    }

    #[test]
    fn stale_generation_ready_outcome_is_ignored_and_file_deleted() {
        let mut app = PerfApp::for_tests(Config::default());
        let path = temp_installer("pw-poll-stale-gen.exe");
        app.update_modal_open = true;
        app.update_modal_phase = ModalPhase::Downloading {
            progress: 0.5,
            bytes: 1,
            total: 2,
        };
        app.update_download_generation = 3;
        put_outcome(&app, 2, DownloadOutcome::Ready(path.clone(), String::new()));
        app.poll_download_outcome();
        assert!(!app.want_quit);
        assert!(!path.exists());
        assert!(
            matches!(app.update_modal_phase, ModalPhase::Downloading { .. }),
            "the live attempt's screen must survive a stale outcome"
        );
    }

    #[test]
    fn stale_generation_failure_does_not_touch_the_modal() {
        let mut app = PerfApp::for_tests(Config::default());
        app.update_modal_open = true;
        app.update_modal_phase = ModalPhase::Downloading {
            progress: 0.5,
            bytes: 1,
            total: 2,
        };
        app.update_download_generation = 3;
        put_outcome(&app, 2, DownloadOutcome::Failed("orphan died".into()));
        app.poll_download_outcome();
        assert!(matches!(
            app.update_modal_phase,
            ModalPhase::Downloading { .. }
        ));
    }

    #[test]
    fn cancelled_outcome_never_reaches_the_failed_screen() {
        let mut app = PerfApp::for_tests(Config::default());
        // The cancel click already routed the modal back to Confirm.
        app.update_modal_open = true;
        app.update_modal_phase = ModalPhase::Confirm;
        put_outcome(
            &app,
            app.update_download_generation,
            DownloadOutcome::Cancelled,
        );
        app.poll_download_outcome();
        assert!(matches!(app.update_modal_phase, ModalPhase::Confirm));
        assert!(!app.want_quit);
    }

    #[test]
    fn late_failure_with_modal_closed_leaves_no_stale_failed_screen() {
        let mut app = PerfApp::for_tests(Config::default());
        app.update_modal_open = false;
        put_outcome(
            &app,
            app.update_download_generation,
            DownloadOutcome::Failed("network reset".into()),
        );
        app.poll_download_outcome();
        assert!(
            matches!(app.update_modal_phase, ModalPhase::Confirm),
            "reopening the modal must land on Confirm, not a stale FAILED screen"
        );
    }

    #[test]
    fn launch_failure_routes_to_the_launch_failed_screen() {
        let mut app = PerfApp::for_tests(Config::default());
        // A real file whose hash matches, but whose bytes are not a Windows
        // executable: verification passes and the launch itself fails.
        let path = std::env::temp_dir().join("pw-poll-unlaunchable-installer.exe");
        std::fs::write(&path, b"not really an installer").expect("test file writes");
        let digest = ring::digest::digest(&ring::digest::SHA256, b"not really an installer");
        let hash = digest
            .as_ref()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        app.update_modal_open = true;
        app.update_modal_phase = ModalPhase::Downloading {
            progress: 1.0,
            bytes: 2,
            total: 2,
        };
        put_outcome(
            &app,
            app.update_download_generation,
            DownloadOutcome::Ready(path, hash),
        );
        app.poll_download_outcome();
        assert!(!app.want_quit);
        assert!(matches!(
            app.update_modal_phase,
            ModalPhase::LaunchFailed { .. }
        ));
    }

    #[test]
    fn a_file_that_changed_after_verification_is_refused() {
        let mut app = PerfApp::for_tests(Config::default());
        let path = temp_installer("pw-poll-tampered-installer.exe");
        app.update_modal_open = true;
        app.update_modal_phase = ModalPhase::Downloading {
            progress: 1.0,
            bytes: 2,
            total: 2,
        };
        put_outcome(
            &app,
            app.update_download_generation,
            DownloadOutcome::Ready(path.clone(), "0".repeat(64)),
        );
        app.poll_download_outcome();
        assert!(!app.want_quit);
        assert!(!path.exists(), "a mismatched installer must be deleted");
        assert!(matches!(app.update_modal_phase, ModalPhase::Failed { .. }));
    }
}
