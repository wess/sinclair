//! The GitHub release check: fetch the latest published release and compare it
//! against the running version.

use crate::{fetch, semver};

/// The GitHub repo releases are published to.
const REPO: &str = "wess/sinclair";

/// A published release.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    /// Semver without the leading `v` (e.g. `1.25.0`).
    pub version: String,
    /// The release page URL.
    pub url: String,
    /// `(asset name, download url)` for each uploaded asset.
    pub assets: Vec<(String, String)>,
}

impl Release {
    /// The download URL of the first asset whose name contains `needle`.
    pub fn asset(&self, needle: &str) -> Option<&str> {
        self.assets
            .iter()
            .find(|(n, _)| n.contains(needle))
            .map(|(_, u)| u.as_str())
    }
}

/// Parse the GitHub `releases/latest` response body into a [`Release`].
fn parse(body: &[u8]) -> Result<Release, String> {
    let v: serde_json::Value =
        serde_json::from_slice(body).map_err(|e| format!("parse release: {e}"))?;
    let tag = v["tag_name"].as_str().ok_or("release has no tag")?;
    let version = tag.trim_start_matches('v').to_string();
    let assets = v["assets"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| {
                    Some((
                        x["name"].as_str()?.to_string(),
                        x["browser_download_url"].as_str()?.to_string(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default();
    let url = v["html_url"].as_str().unwrap_or("").to_string();
    Ok(Release { version, url, assets })
}

/// Fetch the latest published release from GitHub and, if it's newer than
/// `current`, return it. Blocking (spawns `curl`) — run off the UI thread.
pub fn check(current: &str) -> Result<Option<Release>, String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let release = parse(&fetch::bytes(&url)?)?;
    if semver::is_newer(&release.version, current) {
        Ok(Some(release))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
#[path = "../tests/release.rs"]
mod tests;
