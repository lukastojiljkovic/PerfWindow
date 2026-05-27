//! Win32 ShellExecuteExW("runas", ...) helper. Surfaces an interactive UAC
//! prompt for a single child-process launch without elevating the dashboard
//! itself. Used by the connect state machine to start the PerfWindow sensor
//! service per launch.

/// Invoke `ShellExecuteExW` with the `runas` verb so Windows prompts for
/// elevation for just this one process launch. Returns `Ok(())` when the
/// OS launched the process (UAC accepted) and `Err(...)` on UAC denial
/// or any other Win32 failure.
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
