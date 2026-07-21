use super::*;
use crate::Install;
use std::path::PathBuf;

/// A trimmed GitHub `releases/latest` response, with both AppImage
/// architectures so arch matching is exercised.
const BODY: &str = r#"{
    "tag_name": "v1.26.0",
    "html_url": "https://github.com/wess/sinclair/releases/tag/v1.26.0",
    "assets": [
        {"name": "Sinclair.dmg", "browser_download_url": "https://d/Sinclair.dmg", "size": 87357960},
        {"name": "sinclair_1.26.0_amd64.deb", "browser_download_url": "https://d/deb", "size": 11977580},
        {"name": "Sinclair-1.26.0-x86_64.AppImage", "browser_download_url": "https://d/intel", "size": 4},
        {"name": "Sinclair-1.26.0-aarch64.AppImage", "browser_download_url": "https://d/arm", "size": 3}
    ]
}"#;

fn mac() -> Install {
    Install::MacApp(PathBuf::from("/Applications/Sinclair.app"))
}

fn appimage() -> Install {
    Install::AppImage(PathBuf::from("/opt/Sinclair.AppImage"))
}

#[test]
fn github_release_json_parses() {
    let r = parse(BODY.as_bytes()).unwrap();
    assert_eq!(r.version, "1.26.0");
    assert_eq!(r.url, "https://github.com/wess/sinclair/releases/tag/v1.26.0");
    assert_eq!(r.assets.len(), 4);
    assert_eq!(r.assets[0].size, 87357960);
}

#[test]
fn tagless_body_is_an_error() {
    assert!(parse(br#"{"assets": []}"#).is_err());
    assert!(parse(b"not json").is_err());
}

#[test]
fn missing_asset_fields_are_skipped() {
    let r = parse(br#"{"tag_name": "v9.9.9", "assets": [{"name": "x"}]}"#).unwrap();
    assert!(r.assets.is_empty());
    assert!(r.url.is_empty());
}

#[test]
fn absent_size_field_defaults_to_zero() {
    let body = br#"{"tag_name": "v9.9.9", "assets":
        [{"name": "Sinclair.dmg", "browser_download_url": "https://d/x"}]}"#;
    assert_eq!(parse(body).unwrap().assets[0].size, 0);
}

#[test]
fn mac_installs_take_the_universal_dmg() {
    let r = parse(BODY.as_bytes()).unwrap();
    let asset = r.asset_for(&mac()).unwrap();
    assert_eq!(asset.url, "https://d/Sinclair.dmg");
}

#[test]
fn appimage_picks_the_running_architecture() {
    // The fixture deliberately lists x86_64 *before* aarch64. Without arch
    // matching, `pick` returns the first ".AppImage" it sees, so on an aarch64
    // host this assertion is what separates "matched my arch" from "took
    // whatever was listed first" — the bug being guarded is renaming an image
    // built for the other architecture over a working install.
    let r = parse(BODY.as_bytes()).unwrap();
    let asset = r.asset_for(&appimage()).unwrap();
    let want = if std::env::consts::ARCH == "aarch64" { "https://d/arm" } else { "https://d/intel" };
    assert_eq!(asset.url, want);
}

#[test]
fn debian_and_uname_arch_spellings_both_match() {
    assert!(matches_arch("sinclair_1.26.0_arm64.deb", "aarch64"));
    assert!(matches_arch("Sinclair-1.26.0-aarch64.AppImage", "aarch64"));
    assert!(matches_arch("sinclair_1.26.0_amd64.deb", "x86_64"));
    assert!(matches_arch("Sinclair-1.26.0-x86_64.AppImage", "x86_64"));
    assert!(!matches_arch("Sinclair-1.26.0-aarch64.AppImage", "x86_64"));
    assert!(!matches_arch("sinclair_1.26.0_amd64.deb", "aarch64"));
}

#[test]
fn arch_tokens_do_not_match_as_bare_substrings() {
    // 32-bit `ARCH` values are substrings of the 64-bit asset names. Matching
    // loosely would let an i686 or armv7 install download a 64-bit image and
    // rename it over itself.
    assert!(!matches_arch("Sinclair-1.26.0-x86_64.AppImage", "x86"));
    assert!(!matches_arch("sinclair_1.26.0_arm64.deb", "arm"));
    assert!(!matches_arch("Sinclair-1.26.0-aarch64.AppImage", "arm"));
    // A genuine 32-bit artifact still matches its own name.
    assert!(matches_arch("Sinclair-1.26.0-x86.AppImage", "x86"));
    assert!(matches_arch("Sinclair-1.26.0-arm.AppImage", "arm"));
}

#[test]
fn malformed_release_tags_are_refused() {
    // The version lands in the staging directory path, and `semver::parse`
    // reads only the leading fields, so a tag carrying `..` would escape
    // $TMPDIR when `create_dir_all` resolved it.
    assert!(parse(br#"{"tag_name": "v1.28.0-/../../../../pwned", "assets": []}"#).is_err());
    assert!(parse(br#"{"tag_name": "v1.28.0/../x", "assets": []}"#).is_err());
    assert!(parse(br#"{"tag_name": "v1.28.0-beta1", "assets": []}"#).is_err());
    assert!(parse(br#"{"tag_name": "v1.28.0", "assets": []}"#).is_ok());
}

#[test]
fn unknown_installs_have_no_in_place_asset() {
    let r = parse(BODY.as_bytes()).unwrap();
    assert!(r.asset_for(&Install::Unknown).is_none());
}

#[test]
fn a_release_with_our_asset_is_ready() {
    let r = parse(BODY.as_bytes()).unwrap();
    assert!(r.ready_for(&mac()));
    assert!(r.ready_for(&appimage()));
}

#[test]
fn a_release_still_uploading_is_not_ready() {
    // Exactly the v1.27.8 shape: the release went live with only the Linux
    // aarch64 artifacts while macOS notarization was still running. Offering
    // this to a Mac produced an Update button that could only ever fail.
    let body = br#"{"tag_name": "v1.27.8", "assets": [
        {"name": "Sinclair-1.27.8-aarch64.AppImage", "browser_download_url": "https://d/a", "size": 1},
        {"name": "sinclair_1.27.8_arm64.deb", "browser_download_url": "https://d/b", "size": 2}
    ]}"#;
    let r = parse(body).unwrap();
    assert!(!r.ready_for(&mac()));
}

#[test]
fn a_release_with_no_assets_at_all_is_not_ready() {
    let r = parse(br#"{"tag_name": "v9.9.9", "assets": []}"#).unwrap();
    assert!(!r.ready_for(&mac()));
    assert!(!r.ready_for(&appimage()));
    assert!(!r.ready_for(&Install::Unknown));
}

#[test]
fn installs_we_cannot_rewrite_are_never_stranded() {
    // `Unknown` only ever opens the download page, so it must not be gated on
    // an asset for this platform — a machine we publish nothing for would sit
    // on "still building" forever and never reach the page.
    let body = br#"{"tag_name": "v9.9.9", "assets": [
        {"name": "something-for-another-platform.tar.gz", "browser_download_url": "https://d/x", "size": 1}
    ]}"#;
    assert!(parse(body).unwrap().ready_for(&Install::Unknown));
}
