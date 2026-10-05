//! Update orchestration: version comparison and the background check.

use crate::update::cache::{Cache, TTL};
use crate::update::release::Release;
use crate::update::source::{FetchError, ReleaseSource};
use crate::update::state::{SharedUpdateState, UpdateState};
use semver::Version;
use std::sync::{Arc, PoisonError};
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

/// Persistence seam for the check's cache: production hits
/// `%APPDATA%\PerfWindow\update-cache.json`, tests stay in memory so they
/// can assert what a check persisted without touching the real file.
pub(crate) trait CacheStore {
    fn load(&self) -> Option<Cache>;
    fn save(&self, cache: &Cache);
}

pub(crate) struct DiskCacheStore;

impl CacheStore for DiskCacheStore {
    fn load(&self) -> Option<Cache> {
        Cache::load()
    }
    fn save(&self, cache: &Cache) {
        cache.save();
    }
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
        run_check(
            source.as_ref(),
            &state,
            ignore_cache,
            &DiskCacheStore,
            &repaint,
        );
    });
}

pub(crate) fn run_check<S: ReleaseSource + ?Sized>(
    source: &S,
    state: &SharedUpdateState,
    ignore_cache: bool,
    cache_store: &dyn CacheStore,
    repaint: &(impl Fn() + ?Sized),
) {
    set_state(state, UpdateState::Checking, repaint);

    if !ignore_cache {
        if let Some(cache) = cache_store.load() {
            if !cache.is_stale(SystemTime::now(), TTL) {
                publish_cached(state, &cache, repaint);
                return;
            }
        }
    }

    let now = SystemTime::now();
    match source.fetch_latest() {
        Ok(release) => match evaluate(&release) {
            Some(newer) => {
                // NoUpdate results are persisted too: without them the TTL
                // never suppresses the launch check and every start polls
                // GitHub.
                cache_store.save(&cache_from(&release, now));
                let next = if newer {
                    UpdateState::Available {
                        release,
                        checked_at: now,
                    }
                } else {
                    UpdateState::NoUpdate { checked_at: now }
                };
                set_state(state, next, repaint);
            }
            None => set_state(
                state,
                UpdateState::Failed {
                    reason: "malformed release tag or unusable installer asset".into(),
                    checked_at: now,
                },
                repaint,
            ),
        },
        // A rate limit on the automatic check is not worth a banner: keep
        // whatever the last good answer was, and stay quiet when there is no
        // cached answer at all. A manual check reports the limit plainly.
        Err(FetchError::RateLimited) if !ignore_cache => match cache_store.load() {
            Some(cache) => publish_cached(state, &cache, repaint),
            None => set_state(state, UpdateState::Idle, repaint),
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

fn cache_from(release: &Release, checked_at: SystemTime) -> Cache {
    Cache {
        checked_at,
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
        latest_sidecar_url: release
            .sidecar_asset()
            .map(|a| a.browser_download_url.clone())
            .unwrap_or_default(),
    }
}

/// Translate a recently-loaded cache into either `Available` or `NoUpdate`
/// without touching the network.
fn publish_cached(state: &SharedUpdateState, cache: &Cache, repaint: &(impl Fn() + ?Sized)) {
    match is_newer(env!("CARGO_PKG_VERSION"), &cache.latest_tag) {
        Ok(true) => {
            let mut assets = vec![crate::update::release::Asset {
                name: crate::update::release::INSTALLER_ASSET_NAME.into(),
                browser_download_url: cache.latest_asset_url.clone(),
                size: cache.latest_asset_size,
            }];
            if !cache.latest_sidecar_url.is_empty() {
                assets.push(crate::update::release::Asset {
                    name: crate::update::release::SIDECAR_ASSET_NAME.into(),
                    browser_download_url: cache.latest_sidecar_url.clone(),
                    size: 0,
                });
            }
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
                assets,
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
    let asset = release.installer_asset()?;
    // A release whose installer URL is not ours to download is unusable: the
    // downloader must never follow an arbitrary URL.
    if !crate::update::release::is_acceptable_asset_url(&asset.browser_download_url) {
        return None;
    }
    is_newer(env!("CARGO_PKG_VERSION"), &release.tag_name).ok()
}

fn failure_reason(e: &FetchError) -> String {
    e.to_string()
}

fn set_state(state: &SharedUpdateState, next: UpdateState, repaint: &(impl Fn() + ?Sized)) {
    // Recover from poisoning like the UI readers do: a panicked reader must
    // not silently freeze the update state machine.
    *state.lock().unwrap_or_else(PoisonError::into_inner) = next;
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
                    "browser_download_url": "https://github.com/lukastojiljkovic/PerfWindow/releases/download/{tag}/PerfWindow-Setup.exe",
                    "size": 1
                }}]
            }}"#
        )
    }

    /// In-memory store so tests never touch the real
    /// `%APPDATA%\PerfWindow\update-cache.json`.
    #[derive(Default)]
    struct MemCacheStore(std::cell::RefCell<Option<Cache>>);

    impl CacheStore for MemCacheStore {
        fn load(&self) -> Option<Cache> {
            self.0.borrow().clone()
        }
        fn save(&self, cache: &Cache) {
            *self.0.borrow_mut() = Some(cache.clone());
        }
    }

    fn run_sync(source: impl ReleaseSource, ignore_cache: bool) -> UpdateState {
        let state = new_shared();
        run_check(
            &source,
            &state,
            ignore_cache,
            &MemCacheStore::default(),
            &|| {},
        );
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

    #[test]
    fn no_update_result_is_persisted_to_cache() {
        let tag = format!("v{}", env!("CARGO_PKG_VERSION"));
        let s = MockReleaseSource::with_release(&release_json(&tag));
        let state = new_shared();
        let store = MemCacheStore::default();
        run_check(&s, &state, true, &store, &|| {});
        assert!(matches!(
            *state.lock().unwrap(),
            UpdateState::NoUpdate { .. }
        ));
        let cached = store.load().expect("NoUpdate result must be cached");
        assert_eq!(cached.latest_tag, tag);
    }

    #[test]
    fn fresh_no_update_cache_suppresses_the_network() {
        let tag = format!("v{}", env!("CARGO_PKG_VERSION"));
        let store = MemCacheStore::default();
        store.save(&Cache {
            checked_at: SystemTime::now(),
            latest_tag: tag,
            latest_name: "x".into(),
            latest_body: String::new(),
            latest_asset_url:
                "https://github.com/lukastojiljkovic/PerfWindow/releases/download/v0.0.1/PerfWindow-Setup.exe"
                    .into(),
            latest_asset_size: 1,
            latest_sidecar_url: String::new(),
        });
        // A failing source proves the cache served the result: any network
        // attempt would have landed in Failed.
        let s = MockReleaseSource::failing("must not be called");
        let state = new_shared();
        run_check(&s, &state, false, &store, &|| {});
        assert!(matches!(
            *state.lock().unwrap(),
            UpdateState::NoUpdate { .. }
        ));
    }

    #[test]
    fn an_unusable_installer_url_is_a_failed_check() {
        let json = r#"{
            "tag_name": "v999.0.0",
            "name": "x",
            "body": "",
            "html_url": "https://example.com",
            "assets": [{
                "name": "PerfWindow-Setup.exe",
                "browser_download_url": "https://evil.example/PerfWindow-Setup.exe",
                "size": 1
            }]
        }"#;
        let s = MockReleaseSource::with_release(json);
        assert!(matches!(run_sync(s, true), UpdateState::Failed { .. }));
    }

    #[test]
    fn automatic_rate_limit_republishes_the_cache() {
        let tag = format!("v{}", env!("CARGO_PKG_VERSION"));
        let store = MemCacheStore::default();
        store.save(&Cache {
            checked_at: SystemTime::now() - std::time::Duration::from_secs(48 * 3600),
            latest_tag: tag.clone(),
            latest_name: "x".into(),
            latest_body: String::new(),
            latest_asset_url:
                "https://github.com/lukastojiljkovic/PerfWindow/releases/download/x/PerfWindow-Setup.exe"
                    .into(),
            latest_asset_size: 1,
            latest_sidecar_url: String::new(),
        });

        let state = new_shared();
        run_check(
            &MockReleaseSource::rate_limited(),
            &state,
            false,
            &store,
            &|| {},
        );

        // The stale cached answer survives, and the failure is silent.
        assert!(matches!(
            *state.lock().unwrap(),
            UpdateState::NoUpdate { .. }
        ));
    }

    #[test]
    fn automatic_rate_limit_without_a_cache_stays_quiet() {
        let state = new_shared();
        run_check(
            &MockReleaseSource::rate_limited(),
            &state,
            false,
            &MemCacheStore::default(),
            &|| {},
        );
        assert!(matches!(*state.lock().unwrap(), UpdateState::Idle));
    }

    #[test]
    fn manual_rate_limit_is_reported() {
        let state = new_shared();
        run_check(
            &MockReleaseSource::rate_limited(),
            &state,
            true,
            &MemCacheStore::default(),
            &|| {},
        );
        let guard = state.lock().unwrap();
        match &*guard {
            UpdateState::Failed { reason, .. } => assert!(reason.contains("rate")),
            other => panic!("expected a reported failure, got {other:?}"),
        }
    }
}
