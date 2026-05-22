//! Update orchestration: version comparison and the background check.

use crate::update::cache::{Cache, TTL};
use crate::update::release::Release;
use crate::update::source::{FetchError, ReleaseSource};
use crate::update::state::{SharedUpdateState, UpdateState};
use semver::Version;
use std::sync::Arc;
use std::time::SystemTime;

/// How long to wait after process start before firing the first check. Keeps
/// the first frame paint clean on slow DNS resolution.
pub const STARTUP_DELAY_MS: u64 = 500;

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

/// Run a check on a background thread, writing results into `state` and
/// invoking `repaint` after every transition.
///
/// `source` is the production [`crate::update::source::GitHubReleaseSource`]
/// in normal builds and a [`crate::update::source::MockReleaseSource`] in
/// tests. `ignore_cache` is true for the manual "Check for updates now"
/// button and false for the on-start automatic check.
pub fn spawn_check<S: ReleaseSource>(
    source: Arc<S>,
    state: SharedUpdateState,
    ignore_cache: bool,
    repaint: impl Fn() + Send + 'static,
) {
    std::thread::spawn(move || {
        run_check(source.as_ref(), &state, ignore_cache, &repaint);
    });
}

pub(crate) fn run_check<S: ReleaseSource + ?Sized>(
    source: &S,
    state: &SharedUpdateState,
    ignore_cache: bool,
    repaint: &(impl Fn() + ?Sized),
) {
    set_state(state, UpdateState::Checking, repaint);

    if !ignore_cache {
        if let Some(cache) = Cache::load() {
            if !cache.is_stale(SystemTime::now(), TTL) {
                publish_cached(state, &cache, repaint);
                return;
            }
        }
    }

    let now = SystemTime::now();
    match source.fetch_latest() {
        Ok(release) => match evaluate(&release) {
            Some(true) => {
                Cache {
                    checked_at: now,
                    latest_tag: release.tag_name.clone(),
                    latest_name: release.name.clone(),
                    latest_body: release.body.clone(),
                    latest_asset_url: release
                        .installer_asset()
                        .map(|a| a.browser_download_url.clone())
                        .unwrap_or_default(),
                    latest_asset_size: release
                        .installer_asset()
                        .map(|a| a.size)
                        .unwrap_or_default(),
                }
                .save();
                set_state(
                    state,
                    UpdateState::Available {
                        release,
                        checked_at: now,
                    },
                    repaint,
                );
            }
            Some(false) => set_state(state, UpdateState::NoUpdate { checked_at: now }, repaint),
            None => set_state(
                state,
                UpdateState::Failed {
                    reason: "malformed release tag or missing installer asset".into(),
                    checked_at: now,
                },
                repaint,
            ),
        },
        Err(e) => set_state(
            state,
            UpdateState::Failed {
                reason: failure_reason(&e),
                checked_at: now,
            },
            repaint,
        ),
    }
}

/// Translate a recently-loaded cache into either `Available` or `NoUpdate`
/// without touching the network.
fn publish_cached(state: &SharedUpdateState, cache: &Cache, repaint: &(impl Fn() + ?Sized)) {
    match is_newer(env!("CARGO_PKG_VERSION"), &cache.latest_tag) {
        Ok(true) => {
            let release = Release {
                tag_name: cache.latest_tag.clone(),
                name: cache.latest_name.clone(),
                body: cache.latest_body.clone(),
                html_url: format!(
                    "https://github.com/{}/{}/releases/tag/{}",
                    crate::update::OWNER,
                    crate::update::REPO,
                    cache.latest_tag,
                ),
                assets: vec![crate::update::release::Asset {
                    name: crate::update::release::INSTALLER_ASSET_NAME.into(),
                    browser_download_url: cache.latest_asset_url.clone(),
                    size: cache.latest_asset_size,
                }],
            };
            set_state(
                state,
                UpdateState::Available {
                    release,
                    checked_at: cache.checked_at,
                },
                repaint,
            );
        }
        Ok(false) => set_state(
            state,
            UpdateState::NoUpdate {
                checked_at: cache.checked_at,
            },
            repaint,
        ),
        Err(_) => set_state(
            state,
            UpdateState::Failed {
                reason: "cached release tag is unparseable".into(),
                checked_at: cache.checked_at,
            },
            repaint,
        ),
    }
}

/// `Some(true)` if `release` is newer, `Some(false)` if not, `None` if the
/// release is structurally invalid (missing installer or bad tag).
fn evaluate(release: &Release) -> Option<bool> {
    release.installer_asset()?;
    match is_newer(env!("CARGO_PKG_VERSION"), &release.tag_name) {
        Ok(b) => Some(b),
        Err(_) => None,
    }
}

fn failure_reason(e: &FetchError) -> String {
    e.to_string()
}

fn set_state(state: &SharedUpdateState, next: UpdateState, repaint: &(impl Fn() + ?Sized)) {
    if let Ok(mut guard) = state.lock() {
        *guard = next;
    }
    repaint();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::update::source::MockReleaseSource;
    use crate::update::state::new_shared;

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

    fn release_json(tag: &str) -> String {
        format!(
            r#"{{
                "tag_name": "{tag}",
                "name": "x",
                "body": "",
                "html_url": "https://example.com",
                "assets": [{{
                    "name": "PerfWindow-Setup.exe",
                    "browser_download_url": "https://example.com/i.exe",
                    "size": 1
                }}]
            }}"#
        )
    }

    fn run_sync(source: impl ReleaseSource, ignore_cache: bool) -> UpdateState {
        let state = new_shared();
        run_check(&source, &state, ignore_cache, &|| {});
        let g = state.lock().unwrap();
        g.clone()
    }

    #[test]
    fn newer_release_yields_available() {
        let s = MockReleaseSource::with_release(&release_json("v999.0.0"));
        match run_sync(s, true) {
            UpdateState::Available { release, .. } => {
                assert_eq!(release.tag_name, "v999.0.0");
            }
            other => panic!("expected Available, got {other:?}"),
        }
    }

    #[test]
    fn equal_release_yields_no_update() {
        let tag = format!("v{}", env!("CARGO_PKG_VERSION"));
        let s = MockReleaseSource::with_release(&release_json(&tag));
        assert!(matches!(run_sync(s, true), UpdateState::NoUpdate { .. }));
    }

    #[test]
    fn network_error_yields_failed() {
        let s = MockReleaseSource::failing("network unreachable");
        assert!(matches!(run_sync(s, true), UpdateState::Failed { .. }));
    }
}
