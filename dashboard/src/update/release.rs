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

impl Release {
    /// Return the installer asset for this release, or `None` if it is
    /// missing (which the caller treats as a malformed release).
    pub fn installer_asset(&self) -> Option<&Asset> {
        self.assets.iter().find(|a| a.name == INSTALLER_ASSET_NAME)
    }
}

/// Parse the JSON body of `GET /repos/{owner}/{repo}/releases/latest`.
pub fn parse_release(json: &str) -> Result<Release, serde_json::Error> {
    serde_json::from_str(json)
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
    fn malformed_json_returns_err() {
        assert!(parse_release("not json").is_err());
    }
}
