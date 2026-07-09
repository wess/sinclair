use super::*;

/// A trimmed GitHub `releases/latest` response.
const BODY: &str = r#"{
    "tag_name": "v1.26.0",
    "html_url": "https://github.com/wess/prompt/releases/tag/v1.26.0",
    "assets": [
        {"name": "Prompt.dmg", "browser_download_url": "https://d/Prompt.dmg"},
        {"name": "prompt_1.26.0_amd64.deb", "browser_download_url": "https://d/deb"},
        {"name": "Prompt-1.26.0-x86_64.AppImage", "browser_download_url": "https://d/img"}
    ]
}"#;

#[test]
fn github_release_json_parses() {
    let r = parse(BODY.as_bytes()).unwrap();
    assert_eq!(r.version, "1.26.0");
    assert_eq!(r.url, "https://github.com/wess/prompt/releases/tag/v1.26.0");
    assert_eq!(r.assets.len(), 3);
}

#[test]
fn release_asset_lookup() {
    let r = parse(BODY.as_bytes()).unwrap();
    assert_eq!(r.asset(".dmg"), Some("https://d/Prompt.dmg"));
    assert_eq!(r.asset(".AppImage"), Some("https://d/img"));
    assert_eq!(r.asset(".exe"), None);
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
