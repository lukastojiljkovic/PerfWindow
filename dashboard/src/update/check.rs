//! Update orchestration: version comparison and the background check.
//!
//! The orchestrator ([`spawn_check`]) is wired up alongside [`crate::app`]
//! in a separate change; this module provides the pure [`is_newer`]
//! comparison and the building blocks for that orchestrator.

use semver::Version;

/// Compare a current version (raw, e.g. from `CARGO_PKG_VERSION`) against a
/// release tag (`v`-prefixed) and return `true` if the release is newer
/// under SemVer ordering.
///
/// Both inputs must parse as SemVer; the release tag must carry the `v`
/// prefix (the leading `v` is stripped before parsing). Anything else is an
/// `Err`, propagated so the UI can surface "check failed" rather than
/// silently treating a malformed release as "no update".
pub fn is_newer(current: &str, release_tag: &str) -> Result<bool, String> {
    let current = Version::parse(current).map_err(|e| e.to_string())?;
    let release_str = release_tag
        .strip_prefix('v')
        .ok_or_else(|| format!("release tag missing 'v' prefix: {release_tag}"))?;
    let release = Version::parse(release_str).map_err(|e| e.to_string())?;
    Ok(release > current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_versions_are_not_newer() {
        assert!(!is_newer("0.1.0", "v0.1.0").unwrap());
    }

    #[test]
    fn patch_bump_is_newer() {
        assert!(is_newer("0.1.0", "v0.1.1").unwrap());
    }

    #[test]
    fn minor_bump_is_newer() {
        assert!(is_newer("0.1.0", "v0.2.0").unwrap());
    }

    #[test]
    fn major_bump_is_newer() {
        assert!(is_newer("0.1.0", "v1.0.0").unwrap());
    }

    #[test]
    fn older_release_is_not_newer() {
        assert!(!is_newer("0.2.0", "v0.1.9").unwrap());
    }

    #[test]
    fn release_candidate_precedes_release() {
        assert!(is_newer("1.0.0-rc.1", "v1.0.0").unwrap());
        assert!(!is_newer("1.0.0", "v1.0.0-rc.1").unwrap());
    }

    #[test]
    fn malformed_inputs_return_err() {
        assert!(is_newer("not a version", "v0.1.0").is_err());
        assert!(is_newer("0.1.0", "totally-bogus").is_err());
        assert!(is_newer("0.1.0", "0.1.0").is_err(), "missing leading v");
    }
}
