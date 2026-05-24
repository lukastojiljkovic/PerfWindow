//! UI snapshot smoke test. The first iteration covers only the
//! settings modal in the Amber theme; later iterations expand to the
//! full theme x scenario matrix once the harness is proven on this
//! Windows machine.

use egui_kittest::Harness;
use perfwindow::app::PerfApp;
use perfwindow::config::{Config, ThemeId};
use perfwindow::theme::{self, Theme};

#[test]
fn settings_modal_full_amber_matches_baseline() {
    // Build a fresh PerfApp in the Amber theme, with the settings modal open.
    // `for_tests` skips the live `sensord` spawn and font installation that
    // production `PerfApp::new` does from a `CreationContext`.
    let cfg = Config {
        theme: ThemeId::Amber,
        ..Config::default()
    };
    let mut app = PerfApp::for_tests(cfg);
    app.settings_open = true;

    let theme = Theme::for_id(ThemeId::Amber);

    // `build_ui` gives us a `&mut Ui` closure; we paint nothing into it
    // (the settings modal is an `egui::Window`, which registers itself at the
    // Context level, not inside the Ui). Fonts and palette are installed on
    // the first invocation only via a guard since the closure runs per step.
    //
    // `set_fonts` is queued to apply on the *next* frame, so the modal must
    // not render until the second frame at the earliest. We render an empty
    // placeholder on the first call and the modal from the second onward.
    let mut frame = 0u32;
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1180.0, 800.0))
        .build_ui(move |ui| {
            let ctx = ui.ctx().clone();
            if frame == 0 {
                theme::install_fonts(&ctx);
                theme.apply(&ctx);
            }
            frame += 1;
            if frame >= 2 {
                perfwindow::ui::settings::settings_modal(&ctx, &mut app);
            }
        });

    // Force at least a couple of explicit frames so the font registration
    // completes and the modal lays out at its final size before snapshot.
    harness.run();
    harness.step();
    harness.step();

    // `snapshot` writes to `tests/snapshots/{name}.png` relative to the crate
    // root by default (see SnapshotOptions::output_path).
    harness.snapshot("amber/settings_modal_full");
}
