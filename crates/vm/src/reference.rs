//! OCI image references: `debian`, `ubuntu:24.04`, `ghcr.io/org/tools:1`,
//! `alpine@sha256:…`, resolved the way `docker pull` resolves them.

/// Docker Hub's API host. `docker.io` is only the name people type.
const HUB: &str = "registry-1.docker.io";

/// A parsed image reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
  /// Registry API host, e.g. `registry-1.docker.io` or `ghcr.io`.
  pub registry: String,
  /// Repository path, e.g. `library/debian`.
  pub repository: String,
  /// Tag or `sha256:` digest to fetch.
  pub reference: String,
}

impl Reference {
  /// Parse `raw`. Bare names live under Docker Hub's `library/`, a missing
  /// tag means `latest`, and a first path component is a registry only when
  /// it looks like a host (has a `.` or `:`, or is `localhost`).
  pub fn parse(raw: &str) -> Result<Self, String> {
    let raw = raw.trim();
    if raw.is_empty() || raw.chars().any(char::is_whitespace) {
      return Err(format!("invalid image reference `{raw}`"));
    }

    let (name, reference) = match raw.split_once('@') {
      Some((name, digest)) => (name, digest.to_string()),
      None => match raw.rsplit_once(':') {
        Some((name, tag)) if !tag.contains('/') => (name, tag.to_string()),
        _ => (raw, "latest".to_string()),
      },
    };

    let (registry, repository) = match name.split_once('/') {
      Some((host, rest)) if host.contains('.') || host.contains(':') || host == "localhost" => {
        (host.to_string(), rest.to_string())
      }
      Some(_) => (HUB.to_string(), name.to_string()),
      None => (HUB.to_string(), format!("library/{name}")),
    };
    let registry = if registry == "docker.io" || registry == "index.docker.io" {
      HUB.to_string()
    } else {
      registry
    };

    if repository.is_empty() || reference.is_empty() {
      return Err(format!("invalid image reference `{raw}`"));
    }
    Ok(Self {
      registry,
      repository: repository.to_ascii_lowercase(),
      reference,
    })
  }

  /// `https://<registry>/v2/<repository>/<kind>/<id>`.
  pub fn url(&self, kind: &str, id: &str) -> String {
    format!("https://{}/v2/{}/{kind}/{id}", self.registry, self.repository)
  }
}

#[cfg(test)]
#[path = "../tests/reference.rs"]
mod tests;
