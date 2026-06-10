//! The header-strip banner announcing a degraded sensord (typically missing
//! PawnIO).
//!
//! Rendered between the title bar / update banner and the central panel by
//! [`crate::app`]. Visible only when the latest snapshot carries a
//! [`crate::ipc::HealthInfo`] with `degraded == true` and the user has not
//! pressed `Dismiss` in the current session. Styled like the title bar's
//! chrome strip but tinted with the theme's `warn` colour so it reads as an
//! advisory rather than a critical error.

use crate::app::PerfApp;
use crate::theme::Theme;
use egui::{Align, FontId, Layout, Margin, RichText, Sense, Stroke, StrokeKind, Vec2};

const STRIP_PADDING_X: i8 = 13;
const STRIP_PADDING_Y: i8 = 9;
const GAP: f32 = 8.0;

/// Returns true if a banner should be drawn this frame.
pub fn is_visible(app: &PerfApp) -> bool {
    if app.health_banner_dismissed {
        return false;
    }
    matches!(
        app.latest.as_ref().and_then(|s| s.health.as_ref()),
        Some(h) if h.degraded
    )
}

/// Paint the banner. Caller hosts this inside a `Panel::top` so the layout
/// math is identical to the update banner's chrome strip.
pub fn health_banner(ui: &mut egui::Ui, app: &mut PerfApp) {
    // Pull the health payload out of the snapshot. `is_visible` has already
    // guaranteed `degraded == true`, but we re-check defensively so this is
    // safe to call independently.
    let Some(snap) = app.latest.as_ref() else {
        return;
    };
    let Some(health) = snap.health.as_ref() else {
        return;
    };
    if !health.degraded {
        return;
    }

    let theme = app.theme.clone();
    let msg = match health.pawnio.as_str() {
        "missing" => "PawnIO driver not detected — CPU thermal data unavailable.",
        "denied" => "PawnIO access denied — CPU thermal data unavailable.",
        _ => "Sensor service reports a degraded state.",
    };
    let notes = health.notes.clone();

    let frame = egui::Frame::NONE
        .fill(theme.chrome)
        .inner_margin(Margin::symmetric(STRIP_PADDING_X, STRIP_PADDING_Y));

    let mut install_clicked = false;
    let mut dismiss_clicked = false;

    frame.show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = GAP;

            ui.label(
                RichText::new(crate::format::letter_spaced("\u{26a0} HEALTH"))
                    .family(theme.font_display.egui())
                    .size(10.0)
                    .color(theme.warn),
            );

            ui.label(
                RichText::new(msg)
                    .family(theme.font_data.egui())
                    .size(11.0)
                    .color(theme.ink),
            );

            if let Some(notes) = notes {
                ui.label(
                    RichText::new(format!("({notes})"))
                        .family(theme.font_data.egui())
                        .size(11.0)
                        .color(theme.dim),
                );
            }

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if banner_chip(ui, &theme, "Dismiss", false).clicked() {
                    dismiss_clicked = true;
                }
                if banner_chip(ui, &theme, "Install PawnIO", true).clicked() {
                    install_clicked = true;
                }
            });
        });
    });

    // 1 px bottom rule so the banner reads as a discrete strip, matching the
    // update banner's separator treatment.
    let rect = ui.min_rect();
    ui.painter().add(egui::Shape::line_segment(
        [rect.left_bottom(), rect.right_bottom()],
        Stroke::new(1.0, theme.border),
    ));

    if install_clicked {
        crate::ui::shell::open_url("https://pawnio.eu");
    }
    if dismiss_clicked {
        app.health_banner_dismissed = true;
    }
}

/// A chip in the banner. Same visual idiom as [`crate::ui::update_banner`]'s
/// chip so the two banners share a coherent action bar.
fn banner_chip(ui: &mut egui::Ui, theme: &Theme, label: &str, primary: bool) -> egui::Response {
    let font = FontId::new(11.0, theme.font_data.egui());
    let text_color = if primary { theme.bg } else { theme.dim };
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font, text_color);

    let pad = Vec2::new(10.0, 6.0);
    let size = galley.size() + pad * 2.0;
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
        painter.galley(rect.min + pad, galley, text_color);
    }
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::ipc::{HealthInfo, Snapshot};

    fn make_snapshot(health: Option<HealthInfo>) -> Snapshot {
        Snapshot {
            v: 1,
            ts: 0,
            cpu: None,
            gpu: None,
            igpu: None,
            ram: None,
            storage: None,
            board: None,
            fans: None,
            voltages: None,
            net: None,
            battery: None,
            uptime_sec: None,
            atk_fans: None,
            display: None,
            displays: None,
            health,
            ts_ms: None,
        }
    }

    #[test]
    fn invisible_when_no_snapshot() {
        let app = PerfApp::for_tests(Config::default());
        assert!(!is_visible(&app));
    }

    #[test]
    fn invisible_when_snapshot_has_no_health() {
        let mut app = PerfApp::for_tests(Config::default());
        app.latest = Some(make_snapshot(None));
        assert!(!is_visible(&app));
    }

    #[test]
    fn invisible_when_health_not_degraded() {
        let mut app = PerfApp::for_tests(Config::default());
        app.latest = Some(make_snapshot(Some(HealthInfo {
            pawnio: "ok".into(),
            degraded: false,
            notes: None,
        })));
        assert!(!is_visible(&app));
    }

    #[test]
    fn visible_when_degraded() {
        let mut app = PerfApp::for_tests(Config::default());
        app.latest = Some(make_snapshot(Some(HealthInfo {
            pawnio: "missing".into(),
            degraded: true,
            notes: None,
        })));
        assert!(is_visible(&app));
    }

    #[test]
    fn invisible_when_dismissed_even_if_degraded() {
        let mut app = PerfApp::for_tests(Config::default());
        app.latest = Some(make_snapshot(Some(HealthInfo {
            pawnio: "missing".into(),
            degraded: true,
            notes: None,
        })));
        app.health_banner_dismissed = true;
        assert!(!is_visible(&app));
    }
}
