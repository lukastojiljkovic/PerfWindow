//! Installer hand-off.
//!
//! [`launch`] starts the supplied installer executable and returns. The
//! caller is expected to immediately set
//! [`crate::app::PerfApp::want_quit`] so the next UI frame closes the
//! application's window. The Inno installer carries
//! `CloseApplications=force`, so even an ill-timed exit will not corrupt
//! the install; the explicit close exists to remove the race.

use std::path::Path;
use std::process::Command;

/// Build the [`Command`] used to spawn the installer at `path`, without
/// running it. Split out from [`launch`] so unit tests can inspect the
/// program path and arguments without actually starting a child process.
pub(crate) fn installer_command(path: &Path) -> Command {
    Command::new(path)
}

/// Spawn the installer at `path` as an independent process. The current
/// process must exit shortly after the call so the installer can overwrite
/// its files.
pub fn launch(path: &Path) -> std::io::Result<()> {
    installer_command(path).spawn().map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn command_targets_the_supplied_installer_path() {
        let cmd = installer_command(Path::new("C:\\tmp\\Setup.exe"));
        let program = cmd.get_program().to_owned();
        assert_eq!(program, "C:\\tmp\\Setup.exe");
    }

    #[test]
    fn command_inherits_environment() {
        let cmd = installer_command(Path::new("Setup.exe"));
        let envs: Vec<_> = cmd.get_envs().collect();
        assert!(envs.is_empty(), "unexpected env overrides: {:?}", envs);
    }
}
