//! The per-user `PATH` entry that lets `perfwindow-cli` run from any terminal.
//!
//! The list arithmetic is pure and tested; only [`set_on_user_path`] touches
//! the registry.

/// `path` with `dir` appended, or `None` when `dir` is already an entry.
/// Empty entries left by a trailing or doubled `;` are dropped on the way.
pub fn with_dir(path: &str, dir: &str) -> Option<String> {
    if path.split(';').any(|entry| same_dir(entry, dir)) {
        return None;
    }
    let mut entries: Vec<&str> = path.split(';').filter(|e| !e.is_empty()).collect();
    entries.push(dir);
    Some(entries.join(";"))
}

/// `path` without any entry for `dir`, or `None` when there is none to remove.
pub fn without_dir(path: &str, dir: &str) -> Option<String> {
    if !path.split(';').any(|entry| same_dir(entry, dir)) {
        return None;
    }
    let kept: Vec<&str> = path
        .split(';')
        .filter(|e| !e.is_empty() && !same_dir(e, dir))
        .collect();
    Some(kept.join(";"))
}

/// Two PATH entries name the same folder: Windows paths are case-insensitive,
/// and a trailing backslash changes nothing.
fn same_dir(entry: &str, dir: &str) -> bool {
    let entry = normalize(entry);
    !entry.is_empty() && entry == normalize(dir)
}

fn normalize(dir: &str) -> String {
    let dir = dir.trim();
    dir.strip_suffix('\\').unwrap_or(dir).to_ascii_lowercase()
}

/// Add `dir` to (or remove it from) the current user's `PATH`, and tell running
/// programs that the environment changed so new terminals see it.
///
/// The value keeps its registry type: a `REG_EXPAND_SZ` stays one, and a
/// missing `Path` is created as `REG_EXPAND_SZ`. Nothing is written, and no
/// broadcast is sent, unless the list actually changes.
#[cfg(windows)]
pub fn set_on_user_path(dir: &std::path::Path, on: bool) -> std::io::Result<()> {
    use std::borrow::Cow;
    use std::io::ErrorKind;
    use winreg::enums::{RegType, HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};
    use winreg::{RegKey, RegValue};

    let environment = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags("Environment", KEY_READ | KEY_WRITE)?;
    let dir = dir.to_string_lossy();
    let (current, vtype) = match environment.get_raw_value("Path") {
        Ok(value) => (utf16_to_string(&value.bytes), value.vtype),
        Err(e) if e.kind() == ErrorKind::NotFound => (String::new(), RegType::REG_EXPAND_SZ),
        Err(e) => return Err(e),
    };
    let updated = if on {
        with_dir(&current, &dir)
    } else {
        without_dir(&current, &dir)
    };
    if let Some(value) = updated {
        let bytes = Cow::Owned(string_to_utf16_bytes(&value));
        environment.set_raw_value("Path", &RegValue { bytes, vtype })?;
        broadcast_environment_change();
    }
    Ok(())
}

/// Decode a registry string (UTF-16LE, usually NUL-terminated).
#[cfg(windows)]
fn utf16_to_string(bytes: &[u8]) -> String {
    let (pairs, _odd_byte) = bytes.as_chunks::<2>();
    let units: Vec<u16> = pairs.iter().map(|&pair| u16::from_le_bytes(pair)).collect();
    String::from_utf16_lossy(&units)
        .trim_end_matches('\0')
        .to_string()
}

/// Encode a registry string as UTF-16LE with its terminating NUL.
#[cfg(windows)]
fn string_to_utf16_bytes(text: &str) -> Vec<u8> {
    text.encode_utf16()
        .chain(std::iter::once(0))
        .flat_map(u16::to_le_bytes)
        .collect()
}

/// `WM_SETTINGCHANGE` with `"Environment"`, the message Explorer and shells
/// listen for. A hung receiver must not stall the window, so the send times
/// out after five seconds.
#[cfg(windows)]
fn broadcast_environment_change() {
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE,
    };

    let name: Vec<u16> = "Environment\0".encode_utf16().collect();
    // SAFETY: `name` is a NUL-terminated wide string that outlives the call,
    // and the broadcast takes no ownership of it.
    unsafe {
        SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            WPARAM(0),
            LPARAM(name.as_ptr() as isize),
            SMTO_ABORTIFHUNG,
            5000,
            None,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_to_an_empty_path_without_a_stray_separator() {
        assert_eq!(with_dir("", r"C:\Tools"), Some(r"C:\Tools".to_string()));
    }

    #[test]
    fn appends_after_existing_entries() {
        assert_eq!(
            with_dir(r"C:\Windows;C:\Git", r"C:\Tools"),
            Some(r"C:\Windows;C:\Git;C:\Tools".to_string())
        );
    }

    #[test]
    fn a_trailing_separator_is_not_doubled() {
        assert_eq!(
            with_dir(r"C:\Windows;", r"C:\Tools"),
            Some(r"C:\Windows;C:\Tools".to_string())
        );
    }

    #[test]
    fn an_existing_entry_is_not_added_again_in_any_spelling() {
        assert_eq!(with_dir(r"C:\Tools", r"C:\Tools"), None);
        assert_eq!(with_dir(r"c:\tools", r"C:\Tools"), None);
        assert_eq!(with_dir(r"C:\Tools\", r"C:\Tools"), None);
        assert_eq!(with_dir(r"C:\Windows;C:\TOOLS;", r"c:\tools\"), None);
    }

    #[test]
    fn removes_every_matching_entry_and_keeps_the_rest() {
        assert_eq!(
            without_dir(r"C:\Tools;C:\Windows;c:\tools\", r"C:\Tools"),
            Some(r"C:\Windows".to_string())
        );
    }

    #[test]
    fn removing_an_absent_entry_changes_nothing() {
        assert_eq!(without_dir("", r"C:\Tools"), None);
        assert_eq!(without_dir(r"C:\Windows;", r"C:\Tools"), None);
    }

    #[test]
    fn removing_the_only_entry_leaves_an_empty_path() {
        assert_eq!(without_dir(r"C:\Tools;", r"C:\Tools"), Some(String::new()));
    }
}
