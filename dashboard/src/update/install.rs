//! Installer hand-off.
//!
//! [`launch`] starts the supplied installer executable and returns. The
//! caller is expected to immediately set
//! [`crate::app::PerfApp::want_quit`] so the next UI frame closes the
//! application's window. The Inno installer carries `CloseApplications=yes`,
//! so even an ill-timed exit will not corrupt the install; the explicit
//! close exists to remove the race.

use std::path::Path;
use std::process::Command;

/// Spawn the installer at `path` as an independent process. The current
/// process must exit shortly after the call so the installer can overwrite
/// its files.
pub fn launch(path: &Path) -> std::io::Result<()> {
    Command::new(path).spawn().map(|_| ())
}
