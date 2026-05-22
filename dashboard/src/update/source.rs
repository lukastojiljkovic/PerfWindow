//! Abstraction over the upstream release feed.

use crate::update::release::{parse_release, Release};
use std::time::Duration;

/// The upstream the update flow asks for the latest release. Implemented by
/// the production HTTP client and by the in-memory mock used in tests.
pub trait ReleaseSource: Send + Sync + 'static {
    fn fetch_latest(&self) -> Result<Release, FetchError>;
}

/// Why a fetch failed, in user-facing terms.
#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error("no network connection")]
    Network,
    #[error("GitHub returned status {0}")]
    HttpStatus(u16),
    #[error("rate-limited by GitHub")]
    RateLimited,
    #[error("response was not valid release JSON")]
    Malformed,
    #[error("{0}")]
    Other(String),
}

/// Production source. Calls
/// `https://api.github.com/repos/<owner>/<repo>/releases/latest` once per
/// invocation, with a 10-second total timeout and a custom User-Agent.
pub struct GitHubReleaseSource {
    pub owner: &'static str,
    pub repo: &'static str,
    pub user_agent: String,
}

impl GitHubReleaseSource {
    pub fn new(owner: &'static str, repo: &'static str) -> Self {
        Self {
            owner,
            repo,
            user_agent: format!("PerfWindow/{}", env!("CARGO_PKG_VERSION")),
        }
    }
}

impl ReleaseSource for GitHubReleaseSource {
    fn fetch_latest(&self) -> Result<Release, FetchError> {
        let url = format!(
            "https://api.github.com/repos/{}/{}/releases/latest",
            self.owner, self.repo
        );
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(10))
            .user_agent(&self.user_agent)
            .build();

        let response = match agent
            .get(&url)
            .set("Accept", "application/vnd.github+json")
            .call()
        {
            Ok(r) => r,
            Err(ureq::Error::Status(403, ref r))
                if r.header("x-ratelimit-remaining") == Some("0") =>
            {
                return Err(FetchError::RateLimited);
            }
            Err(ureq::Error::Status(code, _)) => return Err(FetchError::HttpStatus(code)),
            Err(ureq::Error::Transport(_)) => return Err(FetchError::Network),
        };

        let body = response.into_string().map_err(|_| FetchError::Malformed)?;
        parse_release(&body).map_err(|_| FetchError::Malformed)
    }
}

/// In-memory source for tests: either returns a pre-baked release or a
/// pre-baked error.
pub struct MockReleaseSource {
    outcome: Result<String, String>,
}

impl MockReleaseSource {
    pub fn with_release(json: &str) -> Self {
        Self {
            outcome: Ok(json.to_owned()),
        }
    }

    pub fn failing(reason: &str) -> Self {
        Self {
            outcome: Err(reason.to_owned()),
        }
    }
}

impl ReleaseSource for MockReleaseSource {
    fn fetch_latest(&self) -> Result<Release, FetchError> {
        match &self.outcome {
            Ok(json) => parse_release(json).map_err(|_| FetchError::Malformed),
            Err(reason) => Err(FetchError::Other(reason.clone())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_returns_the_configured_release() {
        let json = r#"{
            "tag_name": "v0.9.0",
            "name": "x",
            "body": "",
            "html_url": "https://example.com",
            "assets": [{
                "name": "PerfWindow-Setup.exe",
                "browser_download_url": "https://example.com/installer.exe",
                "size": 1
            }]
        }"#;
        let source = MockReleaseSource::with_release(json);
        let release = source.fetch_latest().expect("mock returns the release");
        assert_eq!(release.tag_name, "v0.9.0");
    }

    #[test]
    fn mock_can_simulate_failure() {
        let source = MockReleaseSource::failing("network unreachable");
        let err = source.fetch_latest().unwrap_err();
        assert!(err.to_string().contains("network unreachable"));
    }
}
