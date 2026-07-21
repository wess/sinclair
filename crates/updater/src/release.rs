//! The GitHub release check: fetch the latest published release, compare it
//! against the running version, and decide whether it is something this machine
//! can actually install.
//!
//! That last part is load-bearing. A GitHub release is created and published
//! *before* CI finishes building and uploading its assets, so for the minutes
//! that the macOS notarization takes, `releases/latest` reports a version whose
//! only uploaded files are the Linux ones. Offering that release to a macOS
//! user produces an Update button whose sole possible outcome is "release has
//! no .dmg asset" — which is exactly what a prompt should never do.

use crate::{fetch, semver, Install};

/// The GitHub repo releases are published to.
const REPO: &str = "wess/sinclair";

/// One uploaded release asset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Asset {
    /// The uploaded file name, e.g. `Sinclair-1.27.8-aarch64.AppImage`.
    pub name: String,
    /// Direct download URL.
    pub url: String,
    /// Byte size as GitHub reports it. Drives the download progress bar and the
    /// truncation check after the download; 0 when the field is absent.
    pub size: u64,
}

/// A published release.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    /// Semver without the leading `v` (e.g. `1.25.0`).
    pub version: String,
    /// The release page URL.
    pub url: String,
    /// Every uploaded asset.
    pub assets: Vec<Asset>,
}

/// The outcome of a check. `Pending` exists so a manual "Check for Updates…"
/// can say "still building" instead of the flat lie that you're up to date.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Check {
    /// Nothing newer is published.
    UpToDate,
    /// A newer release exists, but it hasn't uploaded anything this machine can
    /// use yet. Carries the version so the UI can name it.
    Pending(String),
    /// A newer release with the asset this install needs.
    Ready(Release),
}

/// Whether `name` is built for `arch`. Release artifacts spell architectures
/// inconsistently by design: `cargo-deb` writes Debian names (`arm64`,
/// `amd64`) while the tarball and AppImage carry the Rust/uname spelling.
fn matches_arch(name: &str, arch: &str) -> bool {
    let aliases: &[&str] = match arch {
        "aarch64" => &["aarch64", "arm64"],
        "x86_64" => &["x86_64", "amd64"],
        other => &[other],
    };
    aliases.iter().any(|a| contains_token(name, a))
}

/// Whether `name` contains `token` as a whole architecture field rather than as
/// a bare substring. `ARCH` is `"x86"` on 32-bit x86 and `"arm"` on 32-bit ARM,
/// both substrings of the 64-bit asset names — so a loose test would have an
/// i686 install match the `x86_64` AppImage and rename it over itself, the very
/// clobber arch matching exists to stop.
///
/// A trailing `_` does *not* end the token, because `_` continues one
/// (`x86_64`); a leading one does, because that is how `cargo-deb` delimits
/// fields (`sinclair_1.27.8_arm64.deb`).
fn contains_token(name: &str, token: &str) -> bool {
    let bytes = name.as_bytes();
    name.match_indices(token).any(|(i, _)| {
        let before = i == 0 || !bytes[i - 1].is_ascii_alphanumeric();
        let end = i + token.len();
        let after = end == bytes.len()
            || (!bytes[end].is_ascii_alphanumeric() && bytes[end] != b'_');
        before && after
    })
}

/// The first asset whose name contains `needle`, optionally restricted to
/// assets built for `arch`.
fn pick<'a>(assets: &'a [Asset], needle: &str, arch: Option<&str>) -> Option<&'a Asset> {
    assets
        .iter()
        .find(|a| a.name.contains(needle) && arch.is_none_or(|x| matches_arch(&a.name, x)))
}

impl Release {
    /// The download this install would fetch, if the release has published it.
    /// `None` for [`Install::Unknown`], which has no in-place path at all.
    pub fn asset_for(&self, install: &Install) -> Option<&Asset> {
        match install {
            Install::MacApp(_) => pick(&self.assets, ".dmg", None),
            Install::AppImage(_) => pick(&self.assets, ".AppImage", Some(std::env::consts::ARCH)),
            Install::Unknown => None,
        }
    }

    /// Whether this release has finished publishing what this machine needs.
    ///
    /// For an in-place install that means the exact asset. For everything else
    /// the action is "open the download page", which needs no particular
    /// artifact — so the only thing worth waiting for is the release having
    /// *any* asset at all. Gating those on a per-OS asset instead would strand
    /// anyone whose platform we don't publish for (a source build on riscv64,
    /// say) on "still building" forever, never reaching the page fallback that
    /// `Install::Unknown` exists to provide.
    pub fn ready_for(&self, install: &Install) -> bool {
        match install {
            Install::Unknown => !self.assets.is_empty(),
            _ => self.asset_for(install).is_some(),
        }
    }
}

/// Parse the GitHub `releases/latest` response body into a [`Release`].
fn parse(body: &[u8]) -> Result<Release, String> {
    let v: serde_json::Value =
        serde_json::from_slice(body).map_err(|e| format!("parse release: {e}"))?;
    let tag = v["tag_name"].as_str().ok_or("release has no tag")?;
    let version = tag.trim_start_matches('v').to_string();
    // The version becomes a path component of the staging directory, and
    // `semver::parse` only reads the leading three fields — it happily accepts
    // `1.28.0-/../../..`, which `create_dir_all` would then resolve out of
    // $TMPDIR. Nothing we ship tags that way, so refuse it rather than sanitize.
    if !version.split('.').all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())) {
        return Err(format!("refusing malformed release tag `{tag}`"));
    }
    let assets = v["assets"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| {
                    Some(Asset {
                        name: x["name"].as_str()?.to_string(),
                        url: x["browser_download_url"].as_str()?.to_string(),
                        size: x["size"].as_u64().unwrap_or(0),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let url = v["html_url"].as_str().unwrap_or("").to_string();
    Ok(Release { version, url, assets })
}

/// Fetch the latest published release from GitHub and classify it against the
/// running version and this install. Blocking (spawns `curl`) — run off the UI
/// thread.
pub fn check(current: &str, install: &Install) -> Result<Check, String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let release = parse(&fetch::bytes(&url)?)?;
    if !semver::is_newer(&release.version, current) {
        return Ok(Check::UpToDate);
    }
    if !release.ready_for(install) {
        return Ok(Check::Pending(release.version));
    }
    Ok(Check::Ready(release))
}

#[cfg(test)]
#[path = "../tests/release.rs"]
mod tests;
