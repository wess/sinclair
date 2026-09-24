//! A pull-only OCI distribution client: resolve a reference to this host's
//! platform manifest, then fetch its config and layer blobs.
//!
//! Transport is `curl`, as in the updater: https only (redirects too), and a
//! cross-host redirect drops the `Authorization` header, which is what blob
//! storage behind Docker Hub and GHCR requires.

use std::path::Path;
use std::process::Command;

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::reference::Reference;

const ACCEPT: &str = "Accept: application/vnd.oci.image.index.v1+json, \
  application/vnd.docker.distribution.manifest.list.v2+json, \
  application/vnd.oci.image.manifest.v1+json, \
  application/vnd.docker.distribution.manifest.v2+json";

const AGENT: &str = "User-Agent: sinclair-terminal";

/// Cap for manifest and token bodies: 4 MiB.
const MAX_BYTES: &str = "4194304";

/// The resolved single-platform image.
#[derive(Debug, Clone)]
pub struct Image {
  /// `sha256:` digest of the platform manifest; names the unpacked rootfs.
  pub digest: String,
  /// The image config blob (`config.Env`, `config.WorkingDir`, …).
  pub config: Value,
  /// Layer blobs, bottom first: `(digest, media type)`.
  pub layers: Vec<(String, String)>,
}

/// One HTTP response: status, headers of the final hop, body.
pub(crate) struct Response {
  status: u16,
  headers: Vec<(String, String)>,
  body: Vec<u8>,
}

impl Response {
  fn header(&self, name: &str) -> Option<&str> {
    self
      .headers
      .iter()
      .find(|(k, _)| k.eq_ignore_ascii_case(name))
      .map(|(_, v)| v.as_str())
  }
}

pub struct Client {
  reference: Reference,
  token: Option<String>,
}

impl Client {
  pub fn new(reference: Reference) -> Self {
    Self {
      reference,
      token: None,
    }
  }

  /// Resolve the reference to the manifest for `linux/<arch>` and read its
  /// config.
  pub fn resolve(&mut self, arch: &str) -> Result<Image, String> {
    let (body, top) = self.manifest(&self.reference.reference.clone())?;
    let (body, manifest) = if top.get("manifests").is_some() {
      let digest = pick(&top, arch).ok_or_else(|| {
        format!(
          "{} has no linux/{arch} image",
          self.reference.repository
        )
      })?;
      self.manifest(&digest)?
    } else {
      (body, top)
    };

    let config_digest = manifest["config"]["digest"]
      .as_str()
      .ok_or("manifest has no config")?
      .to_string();
    let config = serde_json::from_slice(&self.small(&self.reference.url("blobs", &config_digest))?)
      .map_err(|e| format!("image config: {e}"))?;
    let layers = manifest["layers"]
      .as_array()
      .ok_or("manifest has no layers")?
      .iter()
      .filter_map(|l| {
        Some((
          l["digest"].as_str()?.to_string(),
          l["mediaType"].as_str().unwrap_or_default().to_string(),
        ))
      })
      .collect();

    Ok(Image {
      digest: format!("sha256:{:x}", Sha256::digest(&body)),
      config,
      layers,
    })
  }

  /// Download blob `digest` to `dest`, verifying its sha256. curl draws its
  /// progress bar on stderr, which is the tab the user is watching.
  pub fn blob(&mut self, digest: &str, dest: &Path) -> Result<(), String> {
    let hex = digest
      .strip_prefix("sha256:")
      .ok_or_else(|| format!("unsupported digest `{digest}`"))?;
    let partial = dest.with_extension("part");
    let _ = std::fs::remove_file(&partial);

    let mut cmd = curl();
    cmd.args(["-L", "--fail", "-#", "-o"]).arg(&partial);
    if let Some(token) = &self.token {
      cmd.args(["-H", &format!("Authorization: Bearer {token}")]);
    }
    let status = cmd
      .args(["--", &self.reference.url("blobs", digest)])
      .status()
      .map_err(|e| format!("curl: {e}"))?;
    if !status.success() {
      let _ = std::fs::remove_file(&partial);
      return Err(format!("download of {digest} failed ({status})"));
    }

    let got = sha256_file(&partial)?;
    if got != hex {
      let _ = std::fs::remove_file(&partial);
      return Err(format!("{digest} failed verification"));
    }
    std::fs::rename(&partial, dest).map_err(|e| format!("store blob: {e}"))
  }

  fn manifest(&mut self, id: &str) -> Result<(Vec<u8>, Value), String> {
    let url = self.reference.url("manifests", id);
    let body = self.small(&url)?;
    let value = serde_json::from_slice(&body).map_err(|e| format!("manifest: {e}"))?;
    Ok((body, value))
  }

  /// GET a small body, authenticating once on a 401 challenge.
  fn small(&mut self, url: &str) -> Result<Vec<u8>, String> {
    let mut res = self.get(url)?;
    if res.status == 401 {
      let challenge = res
        .header("www-authenticate")
        .ok_or("registry refused access")?
        .to_string();
      self.token = Some(token(&challenge)?);
      res = self.get(url)?;
    }
    match res.status {
      200 => Ok(res.body),
      401 | 403 => Err("registry refused access (private images are not supported yet)".into()),
      404 => Err(format!("{} not found", self.reference.repository)),
      code => Err(format!("registry answered {code}")),
    }
  }

  fn get(&self, url: &str) -> Result<Response, String> {
    let mut cmd = curl();
    cmd.args(["-L", "-i", "--max-filesize", MAX_BYTES, "-H", ACCEPT]);
    if let Some(token) = &self.token {
      cmd.args(["-H", &format!("Authorization: Bearer {token}")]);
    }
    let out = cmd
      .args(["--", url])
      .output()
      .map_err(|e| format!("curl: {e}"))?;
    if !out.status.success() {
      return Err(format!("could not reach {} ({})", self.reference.registry, out.status));
    }
    parse_response(&out.stdout)
  }
}

fn curl() -> Command {
  let mut cmd = Command::new("curl");
  cmd.args([
    "-sS",
    "--proto",
    "=https",
    "--proto-redir",
    "=https",
    "--connect-timeout",
    "30",
    "-H",
    AGENT,
  ]);
  cmd
}

/// Exchange a `Bearer realm=…,service=…,scope=…` challenge for an anonymous
/// pull token.
fn token(challenge: &str) -> Result<String, String> {
  let params = challenge_params(challenge);
  let realm = params
    .iter()
    .find(|(k, _)| k == "realm")
    .map(|(_, v)| v.clone())
    .ok_or("registry sent no token realm")?;
  if !realm.starts_with("https://") {
    return Err("registry token realm is not https".into());
  }
  let query: Vec<String> = params
    .iter()
    .filter(|(k, _)| k == "service" || k == "scope")
    .map(|(k, v)| format!("{k}={}", encode(v)))
    .collect();
  let url = if query.is_empty() {
    realm
  } else {
    format!("{realm}?{}", query.join("&"))
  };

  let out = curl()
    .args(["-L", "--fail", "--max-filesize", MAX_BYTES, "--", &url])
    .output()
    .map_err(|e| format!("curl: {e}"))?;
  if !out.status.success() {
    return Err("registry refused an anonymous token".into());
  }
  let body: Value = serde_json::from_slice(&out.stdout).map_err(|e| format!("token: {e}"))?;
  body["token"]
    .as_str()
    .or_else(|| body["access_token"].as_str())
    .map(str::to_string)
    .ok_or_else(|| "registry sent no token".into())
}

/// `Bearer realm="https://a",service="b",scope="c"` → `[(realm, https://a), …]`.
pub(crate) fn challenge_params(challenge: &str) -> Vec<(String, String)> {
  let rest = challenge
    .trim()
    .strip_prefix("Bearer")
    .or_else(|| challenge.trim().strip_prefix("bearer"))
    .unwrap_or(challenge)
    .trim();
  let mut out = Vec::new();
  let mut chars = rest.chars().peekable();
  loop {
    while chars.peek().is_some_and(|c| *c == ',' || c.is_whitespace()) {
      chars.next();
    }
    let key: String = chars.by_ref().take_while(|c| *c != '=').collect();
    if key.is_empty() {
      break;
    }
    let value: String = if chars.peek() == Some(&'"') {
      chars.next();
      chars.by_ref().take_while(|c| *c != '"').collect()
    } else {
      chars.by_ref().take_while(|c| *c != ',').collect()
    };
    out.push((key.trim().to_ascii_lowercase(), value));
  }
  out
}

/// The `linux/<arch>` entry of an image index, preferring arm64's `v8` variant
/// when several are listed.
pub(crate) fn pick(index: &Value, arch: &str) -> Option<String> {
  let entries = index["manifests"].as_array()?;
  let matching = entries.iter().filter(|m| {
    m["platform"]["os"].as_str() == Some("linux")
      && m["platform"]["architecture"].as_str() == Some(arch)
  });
  let mut best = None;
  for m in matching {
    let variant = m["platform"]["variant"].as_str();
    if best.is_none() || variant == Some("v8") {
      best = m["digest"].as_str().map(str::to_string);
    }
  }
  best
}

/// Split `curl -i -L` output into the final hop's status and headers, and the
/// body. Redirect hops and `100 Continue` each print their own header block.
pub(crate) fn parse_response(raw: &[u8]) -> Result<Response, String> {
  let mut rest = raw;
  loop {
    let end = find(rest, b"\r\n\r\n").ok_or("malformed registry response")?;
    let head = String::from_utf8_lossy(&rest[..end]).to_string();
    let body = &rest[end + 4..];
    let mut lines = head.lines();
    let status = lines
      .next()
      .and_then(|l| l.split_whitespace().nth(1))
      .and_then(|s| s.parse::<u16>().ok())
      .ok_or("malformed registry status line")?;
    if body.starts_with(b"HTTP/") && (status == 100 || (300..400).contains(&status)) {
      rest = body;
      continue;
    }
    let headers = lines
      .filter_map(|l| l.split_once(':'))
      .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
      .collect();
    return Ok(Response {
      status,
      headers,
      body: body.to_vec(),
    });
  }
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
  hay.windows(needle.len()).position(|w| w == needle)
}

fn encode(s: &str) -> String {
  s.bytes()
    .map(|b| match b {
      b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
      _ => format!("%{b:02X}"),
    })
    .collect()
}

fn sha256_file(path: &Path) -> Result<String, String> {
  let mut file = std::fs::File::open(path).map_err(|e| format!("read blob: {e}"))?;
  let mut hasher = Sha256::new();
  std::io::copy(&mut file, &mut hasher).map_err(|e| format!("read blob: {e}"))?;
  Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
#[path = "../tests/registry.rs"]
mod tests;
