//! Manual check of the release contract against a captured GitHub payload.
//!
//! This test fetches nothing itself: point `PERFWINDOW_RELEASE_JSON` at a file
//! captured from
//! `GET https://api.github.com/repos/lukastojiljkovic/PerfWindow/releases/latest`
//! and run:
//!
//! ```text
//! cargo test --release --test live_release -- --ignored --nocapture
//! ```

use perfwindow::update::release::{is_acceptable_asset_url, parse_release, parse_sidecar};

#[test]
#[ignore = "reads a captured GitHub payload from PERFWINDOW_RELEASE_JSON"]
fn the_live_release_matches_the_contract() {
    let path = std::env::var("PERFWINDOW_RELEASE_JSON")
        .expect("set PERFWINDOW_RELEASE_JSON to the captured payload");
    let json = std::fs::read_to_string(&path).expect("the captured payload is readable");
    let release = parse_release(&json).expect("the live payload parses");

    println!("tag: {}", release.tag_name);
    match release.installer_asset() {
        Some(asset) => println!(
            "installer: {} ({} bytes, url accepted: {})",
            asset.name,
            asset.size,
            is_acceptable_asset_url(&asset.browser_download_url)
        ),
        None => println!("installer: missing"),
    }
    match release.sidecar_asset() {
        Some(asset) => println!(
            "sidecar: {} (url accepted: {})",
            asset.name,
            is_acceptable_asset_url(&asset.browser_download_url)
        ),
        None => println!("sidecar: missing"),
    }

    if let Ok(sidecar_path) = std::env::var("PERFWINDOW_SIDECAR_PATH") {
        let body = std::fs::read_to_string(sidecar_path).expect("the captured sidecar is readable");
        match parse_sidecar(&body) {
            Some(hash) => println!("sidecar parses to: {hash}"),
            None => println!("sidecar does NOT parse: {body:?}"),
        }
    }

    assert!(
        release.installer_asset().is_some(),
        "the release contract requires the installer asset"
    );
}
