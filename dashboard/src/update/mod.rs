//! Update flow: detection, download and installer hand-off.
//!
//! Submodules:
//! - [`release`] parses the GitHub Releases API response.
//! - [`cache`] persists the most recent check result.
//! - [`state`] defines the shared state observed by the UI.
//! - [`source`] abstracts the upstream so tests can plug in a mock.
//! - [`check`] orchestrates the background check.
//! - [`download`] streams the installer asset with progress.
//! - [`install`] hands off to the downloaded installer.

pub mod cache;
pub mod check;
pub mod download;
pub mod install;
pub mod release;
pub mod source;
pub mod state;
