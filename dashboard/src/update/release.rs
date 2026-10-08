//! Strongly-typed view of one GitHub release.

use serde::Deserialize;

/// The fields of the GitHub Releases API response that the update flow uses.
/// Extra fields in the JSON are tolerated (the deserializer ignores them).
#[derive(Debug, Clone, Deserialize)]
pub struct Release {
    pub tag_name: String,
    pub name: String,
    pub body: String,
    pub html_url: String,
    pub assets: Vec<Asset>,
    /// RFC 3339 publish timestamp from the API. Releases that predate the
    /// update modal's "Released on" line have no value (the field is absent
    /// from the JSON), so it defaults to `None`.
    #[serde(default)]
    pub published_at: Option<String>,
}

/// One published asset on a release. Only the installer is consumed by the
/// flow; other assets are present in the JSON but ignored.
#[derive(Debug, Clone, Deserialize)]
pub struct Asset {
    pub name: String,
    pub browser_download_url: String,
    pub size: u64,
}

/// The asset filename the installer downloader looks for.
pub const INSTALLER_ASSET_NAME: &str = "PerfWindow-Setup.exe";

/// The checksum sidecar `release.yml` publishes next to the installer:
/// `"<UPPERCASE HEX>  PerfWindow-Setup.exe"` in ASCII.
pub const SIDECAR_ASSET_NAME: &str = "PerfWindow-Setup.exe.sha256";

impl Release {
    /// Return the installer asset for this release, or `None` if it is
    /// missing (which the caller treats as a malformed release).
    pub fn installer_asset(&self) -> Option<&Asset> {
        self.assets.iter().find(|a| a.name == INSTALLER_ASSET_NAME)
    }

    /// Return the checksum sidecar for this release, or `None` when the
    /// release predates it (published before the sidecar was added).
    pub fn sidecar_asset(&self) -> Option<&Asset> {
        self.assets.iter().find(|a| a.name == SIDECAR_ASSET_NAME)
    }
}

/// Only `https` asset URLs on `github.com` under this repository's release
/// download path are followed. GitHub redirects those to its asset CDN; the
/// checksum covers what actually arrives.
pub fn is_acceptable_asset_url(url: &str) -> bool {
    url.strip_prefix("https://github.com/")
        .is_some_and(|rest| rest.starts_with("lukastojiljkovic/PerfWindow/releases/download/"))
}

/// Parse a sidecar body: 64 hex digits, whitespace, then the installer file
/// name. Returns the hash lowercased. Anything else — empty, truncated,
/// non-hex, naming another file — is rejected.
pub fn parse_sidecar(body: &str) -> Option<String> {
    let mut fields = body.split_whitespace();
    let hash = fields.next()?;
    let name = fields.next()?;
    if fields.next().is_some() || name != INSTALLER_ASSET_NAME {
        return None;
    }
    if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    Some(hash.to_ascii_lowercase())
}

/// Parse the JSON body of `GET /repos/{owner}/{repo}/releases/latest`.
pub fn parse_release(json: &str) -> Result<Release, serde_json::Error> {
    serde_json::from_str(json)
}

/// Month names for [`format_published_date`], indexed by `month - 1`.
const MONTH_NAMES: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// Render a release's RFC 3339 `YYYY-MM-DDTHH:MM:SSZ` timestamp as the
/// user-facing date `d MMMM yyyy` (`"8 October 2026"`). The time-of-day and
/// timezone are ignored — the UTC calendar date is shown — and `None` is
/// returned when the string is not a recognisable timestamp, so a malformed
/// field simply omits the line.
pub fn format_published_date(rfc3339: &str) -> Option<String> {
    let date = rfc3339.get(..10)?;
    let mut parts = date.split('-');
    let year: i32 = parts.next()?.parse().ok()?;
    let month: usize = parts.next()?.parse().ok()?;
    let day: u32 = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    let name = MONTH_NAMES.get(month.checked_sub(1)?)?;
    if !(1..=31).contains(&day) {
        return None;
    }
    Some(format!("{day} {name} {year}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = r#"{
        "tag_name": "v0.2.0",
        "name": "PerfWindow 0.2.0",
        "body": "* added thing\n* fixed thing",
        "html_url": "https://github.com/example/PerfWindow/releases/tag/v0.2.0",
        "assets": [{
            "name": "PerfWindow-Setup.exe",
            "browser_download_url": "https://github.com/example/PerfWindow/releases/download/v0.2.0/PerfWindow-Setup.exe",
            "size": 35840000
        }],
        "draft": false,
        "prerelease": false
    }"#;

    #[test]
    fn parses_a_well_formed_release() {
        let r = parse_release(VALID).expect("parses");
        assert_eq!(r.tag_name, "v0.2.0");
        assert_eq!(r.name, "PerfWindow 0.2.0");
        assert!(r.body.contains("added thing"));
        assert_eq!(r.installer_asset().unwrap().size, 35_840_000);
        assert!(r
            .installer_asset()
            .unwrap()
            .browser_download_url
            .ends_with("/PerfWindow-Setup.exe"));
    }

    #[test]
    fn missing_installer_asset_returns_none_from_helper() {
        let no_asset = r#"{
            "tag_name": "v0.2.0",
            "name": "x",
            "body": "",
            "html_url": "https://example.com",
            "assets": [{
                "name": "other-file.zip",
                "browser_download_url": "https://example.com/other.zip",
                "size": 1
            }]
        }"#;
        let r = parse_release(no_asset).expect("parses");
        assert!(r.installer_asset().is_none());
    }

    #[test]
    fn unknown_fields_are_ignored() {
        let extra = r#"{
            "tag_name": "v0.2.0",
            "name": "x",
            "body": "",
            "html_url": "https://example.com",
            "assets": [],
            "completely_new_field": "ignored"
        }"#;
        assert!(parse_release(extra).is_ok());
    }

    #[test]
    fn a_release_without_a_publish_date_has_none() {
        let r = parse_release(VALID).expect("parses");
        assert!(r.published_at.is_none());
    }

    #[test]
    fn a_publish_date_is_carried_through() {
        let json = r#"{
            "tag_name": "v0.2.0",
            "name": "x",
            "body": "",
            "html_url": "https://example.com",
            "assets": [],
            "published_at": "2026-10-08T09:30:00Z"
        }"#;
        let r = parse_release(json).expect("parses");
        assert_eq!(r.published_at.as_deref(), Some("2026-10-08T09:30:00Z"));
    }

    #[test]
    fn rfc3339_dates_render_as_spelled_out_dates() {
        assert_eq!(
            format_published_date("2026-10-08T09:30:00Z").as_deref(),
            Some("8 October 2026")
        );
        assert_eq!(
            format_published_date("2026-01-31T00:00:00Z").as_deref(),
            Some("31 January 2026")
        );
        assert_eq!(
            format_published_date("2026-12-25T12:00:00Z").as_deref(),
            Some("25 December 2026")
        );
    }

    #[test]
    fn malformed_publish_dates_render_as_none() {
        assert!(format_published_date("").is_none());
        assert!(format_published_date("not a date").is_none());
        assert!(format_published_date("2026-13-08T00:00:00Z").is_none());
        assert!(format_published_date("2026-10-00T00:00:00Z").is_none());
        assert!(format_published_date("2026-10-32T00:00:00Z").is_none());
    }

    #[test]
    fn malformed_json_returns_err() {
        assert!(parse_release("not json").is_err());
    }

    #[test]
    fn installer_and_sidecar_assets_are_found_by_exact_name() {
        let with_sidecar = r#"{
            "tag_name": "v0.2.0",
            "name": "x",
            "body": "",
            "html_url": "https://example.com",
            "assets": [
                {
                    "name": "PerfWindow-Setup.exe.sha256",
                    "browser_download_url": "https://github.com/lukastojiljkovic/PerfWindow/releases/download/v0.2.0/PerfWindow-Setup.exe.sha256",
                    "size": 96
                },
                {
                    "name": "PerfWindow-Setup.exe",
                    "browser_download_url": "https://github.com/lukastojiljkovic/PerfWindow/releases/download/v0.2.0/PerfWindow-Setup.exe",
                    "size": 35840000
                }
            ]
        }"#;
        let r = parse_release(with_sidecar).expect("parses");
        assert_eq!(r.installer_asset().unwrap().size, 35_840_000);
        assert!(r.sidecar_asset().is_some());
    }

    #[test]
    fn a_release_without_the_sidecar_has_no_sidecar_asset() {
        let r = parse_release(VALID).expect("parses");
        assert!(r.sidecar_asset().is_none());
    }

    #[test]
    fn only_our_https_release_download_urls_are_accepted() {
        assert!(is_acceptable_asset_url(
            "https://github.com/lukastojiljkovic/PerfWindow/releases/download/v0.2.0/PerfWindow-Setup.exe"
        ));
        assert!(!is_acceptable_asset_url(
            "http://github.com/lukastojiljkovic/PerfWindow/releases/download/v0.2.0/PerfWindow-Setup.exe"
        ));
        assert!(!is_acceptable_asset_url(
            "https://github.com/lukastojiljkovic/Other/releases/download/v0.2.0/PerfWindow-Setup.exe"
        ));
        assert!(!is_acceptable_asset_url(
            "https://github.com/lukastojiljkovic/PerfWindow/releases/tag/v0.2.0"
        ));
        assert!(!is_acceptable_asset_url(
            "https://github.com.evil.example/PerfWindow-Setup.exe"
        ));
        assert!(!is_acceptable_asset_url(
            "https://objects.githubusercontent.com/whatever"
        ));
        assert!(!is_acceptable_asset_url("not a url"));
    }

    const HASH: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    #[test]
    fn sidecar_accepts_get_filehash_output() {
        let body = format!("{}  PerfWindow-Setup.exe\r\n", HASH.to_uppercase());
        assert_eq!(parse_sidecar(&body).as_deref(), Some(HASH));
    }

    #[test]
    fn sidecar_accepts_a_single_space_and_no_trailing_newline() {
        let body = format!("{HASH} PerfWindow-Setup.exe");
        assert_eq!(parse_sidecar(&body).as_deref(), Some(HASH));
    }

    #[test]
    fn sidecar_is_rejected_when_malformed() {
        assert!(parse_sidecar("").is_none());
        assert!(parse_sidecar("   \n").is_none());
        assert!(parse_sidecar(&format!("{HASH}  Other.exe")).is_none());
        assert!(parse_sidecar(&format!("{HASH}  PerfWindow-Setup.exe extra")).is_none());
        assert!(
            parse_sidecar("00ff  PerfWindow-Setup.exe").is_none(),
            "too short"
        );
        assert!(
            parse_sidecar(&format!("{}  PerfWindow-Setup.exe", "z".repeat(64))).is_none(),
            "not hex"
        );
    }
}
