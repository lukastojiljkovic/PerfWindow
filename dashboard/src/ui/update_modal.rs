//! The update-flow modal: confirm, download progress, failure.
//!
//! Hosted as a free-floating [`egui::Window`] like
//! [`crate::ui::settings::settings_modal`]. Visibility is controlled by
//! [`crate::app::PerfApp::update_modal_open`]. The internal state machine
//! lives in [`ModalPhase`].

use crate::app::PerfApp;
use crate::theme::Theme;
use crate::update::UpdateState;
use egui::{Align2, Color32, FontId, Margin, Response, Sense, Stroke, StrokeKind, Vec2};
use std::sync::atomic::Ordering;

const WINDOW_WIDTH: f32 = 520.0;
const TB_PADDING_X: i8 = 15;
const TB_PADDING_Y: i8 = 12;
const BODY_PADDING_X: i8 = 18;
const BODY_PADDING_Y: i8 = 20;
const BTN_PAD: Vec2 = Vec2::new(14.0, 8.0);
const PROGRESS_BAR_H: f32 = 14.0;
/// Title bar + outer breathing room reserved when computing the body's
/// `ScrollArea` max height — see [`update_modal`].
const CHROME_RESERVE: f32 = 60.0;
/// Floor for the body's `ScrollArea` max height when the viewport is tiny.
const MIN_BODY_HEIGHT: f32 = 180.0;

/// Which screen the modal is currently showing.
#[derive(Debug, Clone, Default)]
pub enum ModalPhase {
    /// "Confirm" screen: changelog + Cancel/Update buttons.
    #[default]
    Confirm,
    /// "Downloading" screen: progress bar.
    Downloading {
        progress: f32,
        bytes: u64,
        total: u64,
    },
    /// "Failed" screen: an error message + browser fallback + retry.
    Failed { message: String },
}

/// Render the modal if `app.update_modal_open` is set.
pub fn update_modal(ctx: &egui::Context, app: &mut PerfApp) {
    if !app.update_modal_open {
        return;
    }

    let release = {
        let guard = match app.update_state.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        match &*guard {
            UpdateState::Available { release, .. } => release.clone(),
            _ => {
                app.update_modal_open = false;
                return;
            }
        }
    };

    // While downloading, mirror the live byte counter into the phase so the
    // bar moves frame-by-frame; the background thread only writes into the
    // progress mutex.
    if matches!(app.update_modal_phase, ModalPhase::Downloading { .. }) {
        if let Ok(p) = app.update_download_progress.lock() {
            app.update_modal_phase = ModalPhase::Downloading {
                progress: p.fraction(),
                bytes: p.bytes,
                total: p.total,
            };
        }
    }

    let theme = app.theme.clone();
    let frame = egui::Frame::NONE
        .fill(theme.bg)
        .stroke(Stroke::new(1.0, theme.border))
        .shadow(egui::epaint::Shadow {
            offset: [0, 18],
            blur: 48,
            spread: 0,
            color: Color32::from_black_alpha(140),
        });

    let mut close = false;
    let phase_snapshot = app.update_modal_phase.clone();

    egui::Window::new("update")
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .fixed_size(Vec2::new(WINDOW_WIDTH, f32::INFINITY))
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .frame(frame)
        .show(ctx, |ui| {
            ui.set_width(WINDOW_WIDTH);
            if modal_title_bar(ui, &theme).clicked() {
                close = true;
            }
            // Cap the body to whatever vertical room is left after the title
            // bar so the close ✕ stays reachable on a short app window. When
            // the body fits, the `ScrollArea` shrinks to its content.
            let body_max_h = (ctx.content_rect().height() - CHROME_RESERVE).max(MIN_BODY_HEIGHT);
            egui::ScrollArea::vertical()
                .max_height(body_max_h)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    egui::Frame::NONE
                        .inner_margin(Margin::symmetric(BODY_PADDING_X, BODY_PADDING_Y))
                        .show(ui, |ui| match &phase_snapshot {
                            ModalPhase::Confirm => {
                                confirm_screen(ui, &theme, app, &release, &mut close)
                            }
                            ModalPhase::Downloading {
                                progress,
                                bytes,
                                total,
                            } => downloading_screen(
                                ui, &theme, app, &release, *progress, *bytes, *total,
                            ),
                            ModalPhase::Failed { message } => {
                                failed_screen(ui, &theme, app, &release, message, &mut close);
                            }
                        });
                });
        });

    if close {
        app.update_modal_open = false;
        app.update_modal_phase = ModalPhase::Confirm;
    }
}

fn modal_title_bar(ui: &mut egui::Ui, theme: &Theme) -> Response {
    let inner = egui::Frame::NONE
        .fill(theme.chrome)
        .inner_margin(Margin::symmetric(TB_PADDING_X, TB_PADDING_Y))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(crate::format::letter_spaced("\u{2913} UPDATE PERFWINDOW"))
                        .family(theme.font_display.egui())
                        .size(13.0)
                        .color(theme.accent),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    close_button(ui, theme)
                })
                .inner
            })
            .inner
        });

    let rect = inner.response.rect;
    ui.painter().add(egui::Shape::line_segment(
        [rect.left_bottom(), rect.right_bottom()],
        Stroke::new(1.0, theme.border),
    ));
    inner.inner
}

fn confirm_screen(
    ui: &mut egui::Ui,
    theme: &Theme,
    app: &mut PerfApp,
    release: &crate::update::Release,
    close: &mut bool,
) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 12.0;

        ui.label(
            egui::RichText::new(format!(
                "Current  {}   New  {}",
                env!("CARGO_PKG_VERSION"),
                release.tag_name.trim_start_matches('v'),
            ))
            .family(theme.font_data.egui())
            .size(12.0)
            .color(theme.ink),
        );

        ui.add(egui::Separator::default().horizontal());

        ui.label(
            egui::RichText::new(crate::format::letter_spaced("WHAT'S NEW"))
                .family(theme.font_display.egui())
                .size(10.0)
                .color(theme.accent),
        );

        egui::ScrollArea::vertical()
            .max_height(220.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new(&release.body)
                        .family(theme.font_data.egui())
                        .size(11.0)
                        .color(theme.dim),
                );
            });

        ui.label(
            egui::RichText::new(
                "The installer will close PerfWindow, install the new version \
                 and offer to launch it again.",
            )
            .family(theme.font_data.egui())
            .size(11.0)
            .color(theme.dim),
        );

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if action_button(ui, theme, "Update now", true).clicked() {
                start_update_download(ui.ctx(), app, release);
            }
            ui.add_space(8.0);
            if action_button(ui, theme, "Cancel", false).clicked() {
                *close = true;
            }
        });
    });
}

fn downloading_screen(
    ui: &mut egui::Ui,
    theme: &Theme,
    app: &mut PerfApp,
    release: &crate::update::Release,
    progress: f32,
    bytes: u64,
    total: u64,
) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 14.0;

        ui.label(
            egui::RichText::new(format!(
                "Downloading PerfWindow {}\u{2026}",
                release.tag_name.trim_start_matches('v'),
            ))
            .family(theme.font_data.egui())
            .size(12.0)
            .color(theme.ink),
        );

        progress_bar(ui, theme, progress);

        ui.label(
            egui::RichText::new(format!(
                "{:.1} MB of {:.1} MB",
                bytes as f64 / 1_000_000.0,
                total as f64 / 1_000_000.0,
            ))
            .family(theme.font_data.egui())
            .size(10.0)
            .color(theme.dim),
        );

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if action_button(ui, theme, "Cancel", false).clicked() {
                app.update_download_cancel.store(true, Ordering::SeqCst);
                app.update_modal_phase = ModalPhase::Confirm;
            }
        });
    });
}

fn progress_bar(ui: &mut egui::Ui, theme: &Theme, fraction: f32) {
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), PROGRESS_BAR_H),
        Sense::hover(),
    );
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, theme.track);
    let mut fill_rect = rect;
    fill_rect.max.x = rect.min.x + rect.width() * fraction.clamp(0.0, 1.0);
    painter.rect_filled(fill_rect, 0.0, theme.accent);
    painter.rect_stroke(
        rect,
        0.0,
        Stroke::new(1.0, theme.border),
        StrokeKind::Inside,
    );
}

fn failed_screen(
    ui: &mut egui::Ui,
    theme: &Theme,
    app: &mut PerfApp,
    release: &crate::update::Release,
    message: &str,
    close: &mut bool,
) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 12.0;

        ui.label(
            egui::RichText::new(crate::format::letter_spaced("\u{2715} DOWNLOAD FAILED"))
                .family(theme.font_display.egui())
                .size(13.0)
                .color(theme.hot),
        );

        ui.label(
            egui::RichText::new(format!("Could not download the installer:\n{message}"))
                .family(theme.font_data.egui())
                .size(11.0)
                .color(theme.dim),
        );

        ui.label(
            egui::RichText::new(
                "You can try again, or download it manually from the releases page.",
            )
            .family(theme.font_data.egui())
            .size(11.0)
            .color(theme.dim),
        );

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if action_button(ui, theme, "Try again", true).clicked() {
                app.update_modal_phase = ModalPhase::Confirm;
            }
            ui.add_space(8.0);
            if action_button(ui, theme, "Open in browser", false).clicked() {
                open_in_browser(&release.html_url);
                *close = true;
            }
        });
    });
}

/// Kick off the background download and transition the modal to its
/// Downloading screen. Outcome is written to `app.update_download_outcome`
/// by the worker thread and applied to the UI by
/// [`PerfApp::poll_download_outcome`] on the next frame.
fn start_update_download(ctx: &egui::Context, app: &mut PerfApp, release: &crate::update::Release) {
    let Some(asset) = release.installer_asset().cloned() else {
        app.update_modal_phase = ModalPhase::Failed {
            message: "release is missing the installer asset".into(),
        };
        return;
    };
    let dest = std::env::temp_dir().join(format!(
        "PerfWindow-Setup-{}.exe",
        release.tag_name.trim_start_matches('v')
    ));

    app.update_download_cancel.store(false, Ordering::SeqCst);
    if let Ok(mut g) = app.update_download_outcome.lock() {
        *g = None;
    }
    app.update_modal_phase = ModalPhase::Downloading {
        progress: 0.0,
        bytes: 0,
        total: asset.size,
    };

    let progress = std::sync::Arc::clone(&app.update_download_progress);
    let cancel = std::sync::Arc::clone(&app.update_download_cancel);
    let outcome = std::sync::Arc::clone(&app.update_download_outcome);
    let repaint_ctx = ctx.clone();
    let finish_ctx = ctx.clone();

    crate::update::download::start_download(
        asset.browser_download_url,
        dest,
        asset.size,
        progress,
        cancel,
        move || repaint_ctx.request_repaint(),
        move |result| {
            if let Ok(mut g) = outcome.lock() {
                *g = Some(match result {
                    Ok(path) => crate::app::DownloadOutcome::Ready(path),
                    Err(e) => crate::app::DownloadOutcome::Failed(e.to_string()),
                });
            }
            finish_ctx.request_repaint();
        },
    );
}

/// Standard primary/secondary button styled like the banner chip.
fn action_button(ui: &mut egui::Ui, theme: &Theme, label: &str, primary: bool) -> egui::Response {
    let font = FontId::new(11.0, theme.font_data.egui());
    let text_color = if primary { theme.bg } else { theme.dim };
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font, text_color);
    let size = galley.size() + BTN_PAD * 2.0;
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());

    if ui.is_rect_visible(rect) {
        let painter = ui.painter_at(rect);
        let stroke_color = if primary { theme.accent } else { theme.border };
        if primary {
            painter.rect_filled(rect, 0.0, theme.accent);
        }
        painter.rect_stroke(
            rect,
            0.0,
            Stroke::new(1.0, stroke_color),
            StrokeKind::Inside,
        );
        painter.galley(rect.min + BTN_PAD, galley, text_color);
    }
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
}

fn close_button(ui: &mut egui::Ui, theme: &Theme) -> Response {
    let font = FontId::new(12.0, theme.font_data.egui());
    let pad = Vec2::new(9.0, 4.0);
    let galley = ui
        .painter()
        .layout_no_wrap("\u{2715}".to_owned(), font, Color32::PLACEHOLDER);
    let size = galley.size() + pad * 2.0;
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());

    if ui.is_rect_visible(rect) {
        let painter = ui.painter_at(rect);
        let hovered = response.hovered();
        let (fill, stroke_color, text_color) = if hovered {
            (theme.accent, theme.accent, theme.bg)
        } else {
            (Color32::TRANSPARENT, theme.border, theme.dim)
        };
        if fill != Color32::TRANSPARENT {
            painter.rect_filled(rect, 0.0, fill);
        }
        painter.rect_stroke(
            rect,
            0.0,
            Stroke::new(1.0, stroke_color),
            StrokeKind::Inside,
        );
        painter.galley(rect.min + pad, galley, text_color);
    }
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
}

/// Shell-execute a URL via the Windows shell so the OS picks the default
/// browser. Errors are logged and otherwise ignored — the user can copy the
/// URL manually if the spawn fails.
fn open_in_browser(url: &str) {
    use std::process::Command;
    if let Err(e) = Command::new("cmd").args(["/C", "start", "", url]).spawn() {
        eprintln!("PerfWindow: failed to open {url} in browser: {e}");
    }
}
