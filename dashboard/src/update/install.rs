//! Installer hand-off.
//!
//! [`launch`] starts the supplied installer executable and returns. The
//! caller is expected to immediately set
//! [`crate::app::PerfApp::want_quit`] so the next UI frame closes the
//! application's window. The Inno installer carries
//! `CloseApplications=force`, so even an ill-timed exit will not corrupt
//! the install; the explicit close exists to remove the race.
//!
//! The launch goes through `ShellExecuteExW`, not `std::process::Command`:
//! the installer is built with `PrivilegesRequired=admin`, and
//! `CreateProcessW` refuses to start a binary whose manifest requests
//! elevation (ERROR_ELEVATION_REQUIRED, os error 740). The `open` verb is
//! sufficient — the shell honours the manifest and raises the UAC prompt
//! itself.

use std::path::Path;

/// Spawn the installer at `path` as an independent process. The current
/// process must exit shortly after the call so the installer can overwrite
/// its files.
pub fn launch(path: &Path) -> Result<(), String> {
    crate::ui::shell::shell_exec(
        "open",
        &path.display().to_string(),
        "",
        crate::ui::shell::SW_SHOWNORMAL,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launching_a_missing_installer_returns_err() {
        let missing = std::env::temp_dir().join("pw-missing-installer-b2-test.exe");
        let _ = std::fs::remove_file(&missing);
        assert!(launch(&missing).is_err());
    }
}
