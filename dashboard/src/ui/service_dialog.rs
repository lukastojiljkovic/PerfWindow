//! The "Sensor service is not running" modal.
//!
//! Shown on startup when the production-mode pipe connect failed. Offers a
//! single Start button that calls `ShellExecuteExW("runas", "sc.exe",
//! "start PerfWindowSensor")`, letting Windows handle the elevation prompt
//! for just the `sc.exe` invocation. The dashboard itself keeps running as
//! the regular user; after the service comes up, [`crate::app::PerfApp::
//! try_reconnect_if_starting`] reconnects the pipe and the dialog dismisses
//! itself.

use crate::app::PerfApp;

/// Render the service-start dialog over the dashboard when
/// `app.service_dialog_open` is set. No-op otherwise, so callers can invoke
/// this every frame unconditionally.
pub fn service_dialog(ctx: &egui::Context, app: &mut PerfApp) {
    if !app.service_dialog_open {
        return;
    }
    let mut close = false;
    egui::Window::new("Sensor service")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            ui.set_max_width(420.0);
            ui.label(&app.service_dialog_message);
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                let start_btn = ui.add_enabled(
                    !app.service_starting,
                    egui::Button::new(if app.service_starting {
                        "Starting…"
                    } else {
                        "Start"
                    }),
                );
                if start_btn.clicked() {
                    app.service_starting = true;
                    let ctx_clone = ctx.clone();
                    let outcome_msg = match shell_exec_runas("sc.exe", "start PerfWindowSensor") {
                        Ok(()) => "Service start requested. Reconnecting…".to_string(),
                        Err(e) => format!("Could not request elevation: {e}"),
                    };
                    app.service_dialog_message = outcome_msg;
                    ctx_clone.request_repaint_after(std::time::Duration::from_millis(500));
                }
                if ui.button("Cancel").clicked() {
                    close = true;
                }
            });
        });
    if close {
        app.service_dialog_open = false;
    }
}

/// Invoke `ShellExecuteExW` with the `runas` verb so Windows prompts for
/// elevation for just this one `sc.exe start ...` call. Returns `Ok(())`
/// when the process was launched (whether or not it ultimately succeeded —
/// success of the *service* start is observed via the pipe reconnect).
pub fn shell_exec_runas(exe: &str, args: &str) -> Result<(), String> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    let to_wide = |s: &str| -> Vec<u16> {
        OsStr::new(s)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    };
    let verb = to_wide("runas");
    let file = to_wide(exe);
    let params = to_wide(args);
    #[repr(C)]
    struct ShellExecuteInfoW {
        cb_size: u32,
        f_mask: u32,
        hwnd: usize,
        lp_verb: *const u16,
        lp_file: *const u16,
        lp_parameters: *const u16,
        lp_directory: *const u16,
        n_show: i32,
        h_inst_app: usize,
        lp_id_list: usize,
        lp_class: *const u16,
        hkey_class: usize,
        dw_hot_key: u32,
        h_icon_or_monitor: usize,
        h_process: usize,
    }
    extern "system" {
        fn ShellExecuteExW(info: *mut ShellExecuteInfoW) -> i32;
    }
    let mut info = ShellExecuteInfoW {
        cb_size: std::mem::size_of::<ShellExecuteInfoW>() as u32,
        f_mask: 0,
        hwnd: 0,
        lp_verb: verb.as_ptr(),
        lp_file: file.as_ptr(),
        lp_parameters: params.as_ptr(),
        lp_directory: std::ptr::null(),
        n_show: 0,
        h_inst_app: 0,
        lp_id_list: 0,
        lp_class: std::ptr::null(),
        hkey_class: 0,
        dw_hot_key: 0,
        h_icon_or_monitor: 0,
        h_process: 0,
    };
    let ok = unsafe { ShellExecuteExW(&mut info as *mut _) };
    if ok == 0 {
        Err(format!(
            "ShellExecuteExW failed: {}",
            std::io::Error::last_os_error()
        ))
    } else {
        Ok(())
    }
}
