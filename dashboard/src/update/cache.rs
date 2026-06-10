//! Persistence for the most recent successful update check.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// How long a successful check result stays fresh before the next startup
/// will hit the network again.
pub const TTL: Duration = Duration::from_secs(6 * 3600);

/// One persisted update-check result. `checked_at` is serialised as Unix
/// seconds so the file is human-inspectable without timezone surprises.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cache {
    #[serde(with = "unix_seconds")]
    pub checked_at: SystemTime,
    pub latest_tag: String,
    pub latest_name: String,
    pub latest_body: String,
    pub latest_asset_url: String,
    pub latest_asset_size: u64,
}

impl Cache {
    /// `%APPDATA%\PerfWindow\update-cache.json`.
    pub fn path() -> Option<PathBuf> {
        let appdata = std::env::var_os("APPDATA")?;
        Some(
            PathBuf::from(appdata)
                .join("PerfWindow")
                .join("update-cache.json"),
        )
    }

    /// Read a cache from disk; any failure (missing, malformed, IO error)
    /// yields `None`. Callers treat that as "no cache".
    pub fn load() -> Option<Self> {
        let path = Self::path()?;
        let body = std::fs::read_to_string(&path).ok()?;
        Self::from_json(&body).ok()
    }

    /// Atomically write a cache to disk. Errors are logged but not returned;
    /// a write failure must not block the check itself.
    pub fn save(&self) {
        let Some(path) = Self::path() else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        match self.to_json() {
            Ok(json) => {
                if let Err(e) = atomic_write(&path, json.as_bytes()) {
                    eprintln!("PerfWindow: failed to save update cache: {e}");
                }
            }
            Err(e) => eprintln!("PerfWindow: failed to serialise update cache: {e}"),
        }
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }

    /// `true` if the cache is older than `ttl` relative to `now`. A
    /// `checked_at` in the future is also stale: a clock that has stepped
    /// backwards must not freeze the cache as "fresh" indefinitely.
    pub fn is_stale(&self, now: SystemTime, ttl: Duration) -> bool {
        match now.duration_since(self.checked_at) {
            Ok(age) => age > ttl,
            Err(_) => true,
        }
    }
}

/// Write `bytes` to `path` atomically by writing to a sibling `.tmp` file
/// and renaming it into place. A torn write leaves any previous file intact.
fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

/// `serde` shim: persist `SystemTime` as seconds since the Unix epoch.
mod unix_seconds {
    use super::*;
    use serde::{Deserializer, Serializer};

    pub fn serialize<S: Serializer>(t: &SystemTime, s: S) -> Result<S::Ok, S::Error> {
        let secs = t
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        s.serialize_i64(secs)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<SystemTime, D::Error> {
        use serde::Deserialize;
        let secs = i64::deserialize(d)?;
        if secs >= 0 {
            Ok(SystemTime::UNIX_EPOCH + Duration::from_secs(secs as u64))
        } else {
            Ok(SystemTime::UNIX_EPOCH)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime};

    fn sample() -> Cache {
        Cache {
            checked_at: SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000),
            latest_tag: "v0.2.0".into(),
            latest_name: "PerfWindow 0.2.0".into(),
            latest_body: "* x\n* y".into(),
            latest_asset_url: "https://example.com/PerfWindow-Setup.exe".into(),
            latest_asset_size: 1234,
        }
    }

    #[test]
    fn round_trips_through_json() {
        let original = sample();
        let json = original.to_json().expect("serialises");
        let parsed = Cache::from_json(&json).expect("deserialises");
        assert_eq!(parsed.latest_tag, original.latest_tag);
        assert_eq!(parsed.latest_asset_size, original.latest_asset_size);
        assert_eq!(parsed.checked_at, original.checked_at);
    }

    #[test]
    fn ttl_expires_after_six_hours() {
        let c = sample();
        let just_under = c.checked_at + Duration::from_secs(6 * 3600 - 1);
        let just_over = c.checked_at + Duration::from_secs(6 * 3600 + 1);
        assert!(!c.is_stale(just_under, TTL));
        assert!(c.is_stale(just_over, TTL));
    }

    #[test]
    fn future_checked_at_is_stale() {
        let c = sample();
        let before_check = c.checked_at - Duration::from_secs(3600);
        assert!(c.is_stale(before_check, TTL));
    }

    #[test]
    fn malformed_cache_returns_err() {
        assert!(Cache::from_json("garbage").is_err());
    }
}
