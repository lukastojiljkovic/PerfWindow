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

/// Lowercase hex; the form the downloader and the sidecar parser use.
pub(crate) fn hex_lower(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut out, b| {
        let _ = write!(out, "{b:02x}");
        out
    })
}

/// Re-read the downloaded installer and confirm it still matches the hash
/// recorded when the download verified it. This closes the window between the
/// checksum check and the launch: a file swapped, truncated or replaced in
/// between is refused.
pub fn verify(path: &Path, expected_hash: &str) -> bool {
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    let digest = ring::digest::digest(&ring::digest::SHA256, &bytes);
    hex_lower(digest.as_ref()) == expected_hash.to_ascii_lowercase()
}

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

    #[test]
    fn verify_accepts_a_matching_hash_and_rejects_a_changed_file() {
        let path = std::env::temp_dir().join("pw-install-verify.exe");
        std::fs::write(&path, b"installer bytes").expect("test file writes");
        let digest = ring::digest::digest(&ring::digest::SHA256, b"installer bytes");
        let hash = hex_lower(digest.as_ref());

        assert!(verify(&path, &hash));
        assert!(verify(&path, &hash.to_uppercase()), "hash case is ignored");

        std::fs::write(&path, b"tampered bytes").expect("test file rewrites");
        assert!(!verify(&path, &hash));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn verify_rejects_a_missing_file() {
        let missing = std::env::temp_dir().join("pw-install-verify-missing.exe");
        let _ = std::fs::remove_file(&missing);
        assert!(!verify(&missing, &"0".repeat(64)));
    }
}
