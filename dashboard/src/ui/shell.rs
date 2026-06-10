//! Windows shell helpers: a `ShellExecuteExW` wrapper and default-browser
//! URL opening. `ShellExecuteExW` is required wherever `CreateProcessW`
//! cannot work — elevating a single `sc.exe` invocation via UAC, or starting
//! a binary whose manifest requests elevation (`CreateProcessW` fails with
//! ERROR_ELEVATION_REQUIRED, os error 740).

/// Hide the launched process's window — used for console children like
/// `sc.exe` where a flashing console adds nothing.
pub const SW_HIDE: i32 = 0;
/// Show the launched process's window normally.
pub const SW_SHOWNORMAL: i32 = 1;

/// Suppress the shell's own error message boxes; failures surface only
/// through the returned `Err`, so the app controls all error UI.
const SEE_MASK_FLAG_NO_UI: u32 = 0x0000_0400;

/// Invoke `ShellExecuteExW` with the `runas` verb so Windows prompts for
/// elevation for just this one process launch. Returns `Ok(())` when the
/// OS launched the process (UAC accepted) and `Err(...)` on UAC denial
/// or any other Win32 failure.
pub fn shell_exec_runas(exe: &str, args: &str) -> Result<(), String> {
    shell_exec("runas", exe, args, SW_HIDE)
}

/// Invoke `ShellExecuteExW` with an arbitrary verb. The `open` verb is
/// enough to start an elevation-manifested executable: the shell reads the
/// manifest and raises the UAC prompt itself.
pub fn shell_exec(verb: &str, exe: &str, args: &str, n_show: i32) -> Result<(), String> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    let to_wide = |s: &str| -> Vec<u16> {
        OsStr::new(s)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    };
    let verb = to_wide(verb);
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
        f_mask: SEE_MASK_FLAG_NO_UI,
        hwnd: 0,
        lp_verb: verb.as_ptr(),
        lp_file: file.as_ptr(),
        lp_parameters: params.as_ptr(),
        lp_directory: std::ptr::null(),
        n_show,
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

/// Open `url` in the default browser. Errors are logged and otherwise
/// ignored — there is no UI surface for an "open failed" path and the user
/// can always copy the URL manually.
pub fn open_url(url: &str) {
    if let Err(e) = webbrowser::open(url) {
        eprintln!("PerfWindow: failed to open {url} in browser: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_exec_open_on_a_missing_file_returns_err() {
        let missing = std::env::temp_dir().join("pw-no-such-binary-b2-test.exe");
        let _ = std::fs::remove_file(&missing);
        let result = shell_exec("open", &missing.display().to_string(), "", SW_HIDE);
        assert!(result.is_err());
    }
}
