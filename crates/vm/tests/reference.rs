use super::*;

fn parse(raw: &str) -> (String, String, String) {
  let r = Reference::parse(raw).unwrap();
  (r.registry, r.repository, r.reference)
}

#[test]
fn bare_name_is_a_hub_library_image_at_latest() {
  assert_eq!(
    parse("debian"),
    ("registry-1.docker.io".into(), "library/debian".into(), "latest".into())
  );
}

#[test]
fn tag_is_kept() {
  assert_eq!(parse("ubuntu:24.04").2, "24.04");
}

#[test]
fn hub_user_repo_has_no_library_prefix() {
  assert_eq!(parse("wess/tools:1").1, "wess/tools");
}

#[test]
fn host_component_is_the_registry() {
  assert_eq!(
    parse("ghcr.io/org/tools:1"),
    ("ghcr.io".into(), "org/tools".into(), "1".into())
  );
}

#[test]
fn registry_port_is_not_a_tag() {
  assert_eq!(
    parse("localhost:5000/tools"),
    ("localhost:5000".into(), "tools".into(), "latest".into())
  );
}

#[test]
fn digest_reference() {
  assert_eq!(parse("alpine@sha256:abc").2, "sha256:abc");
}

#[test]
fn docker_io_alias_maps_to_the_api_host() {
  assert_eq!(parse("docker.io/library/alpine").0, "registry-1.docker.io");
}

#[test]
fn rejects_empty_and_spaces() {
  assert!(Reference::parse("").is_err());
  assert!(Reference::parse("debian latest").is_err());
}

#[test]
fn manifest_url() {
  let r = Reference::parse("debian:12").unwrap();
  assert_eq!(
    r.url("manifests", &r.reference),
    "https://registry-1.docker.io/v2/library/debian/manifests/12"
  );
}
