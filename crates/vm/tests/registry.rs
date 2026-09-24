use super::*;
use serde_json::json;

#[test]
fn challenge_params_read_quoted_values() {
  let p = challenge_params(
    r#"Bearer realm="https://auth.docker.io/token",service="registry.docker.io",scope="repository:library/debian:pull""#,
  );
  assert_eq!(
    p,
    vec![
      ("realm".into(), "https://auth.docker.io/token".into()),
      ("service".into(), "registry.docker.io".into()),
      ("scope".into(), "repository:library/debian:pull".into()),
    ]
  );
}

#[test]
fn encode_escapes_scope_separators() {
  assert_eq!(encode("repository:a/b:pull"), "repository%3Aa%2Fb%3Apull");
}

#[test]
fn pick_matches_os_and_arch() {
  let index = json!({"manifests": [
    {"digest": "sha256:amd", "platform": {"os": "linux", "architecture": "amd64"}},
    {"digest": "sha256:arm7", "platform": {"os": "linux", "architecture": "arm", "variant": "v7"}},
    {"digest": "sha256:arm64", "platform": {"os": "linux", "architecture": "arm64", "variant": "v8"}},
    {"digest": "sha256:att", "platform": {"os": "unknown", "architecture": "unknown"}},
  ]});
  assert_eq!(pick(&index, "arm64").as_deref(), Some("sha256:arm64"));
  assert_eq!(pick(&index, "amd64").as_deref(), Some("sha256:amd"));
  assert_eq!(pick(&index, "riscv64"), None);
}

#[test]
fn pick_prefers_the_v8_variant() {
  let index = json!({"manifests": [
    {"digest": "sha256:plain", "platform": {"os": "linux", "architecture": "arm64"}},
    {"digest": "sha256:v8", "platform": {"os": "linux", "architecture": "arm64", "variant": "v8"}},
  ]});
  assert_eq!(pick(&index, "arm64").as_deref(), Some("sha256:v8"));
}

#[test]
fn parse_response_skips_redirect_hops() {
  let raw = b"HTTP/2 307\r\nlocation: https://cdn/x\r\n\r\nHTTP/2 200\r\ncontent-type: application/json\r\n\r\n{}";
  let res = parse_response(raw).unwrap();
  assert_eq!(res.status, 200);
  assert_eq!(res.header("Content-Type"), Some("application/json"));
  assert_eq!(res.body, b"{}");
}

#[test]
fn parse_response_keeps_a_401_challenge() {
  let raw = b"HTTP/1.1 401 Unauthorized\r\nWWW-Authenticate: Bearer realm=\"https://a\"\r\n\r\n";
  let res = parse_response(raw).unwrap();
  assert_eq!(res.status, 401);
  assert_eq!(res.header("www-authenticate"), Some("Bearer realm=\"https://a\""));
}
