use super::*;

#[test]
fn missing_file_yields_defaults() {
  let (opts, diags) = load_path(std::path::Path::new("/nonexistent/sinclair/settings.json"));
  assert_eq!(opts, Options::default());
  assert!(diags.is_empty());
}

#[test]
fn load_path_reads_json() {
  let dir = std::env::temp_dir().join(format!("sinclairconfigtest{}", std::process::id()));
  std::fs::create_dir_all(&dir).unwrap();
  let file = dir.join("settings.json");
  std::fs::write(&file, "{\n  \"font-size\": 17,\n  \"bogus\": 1\n}\n").unwrap();
  let (opts, diags) = load_path(&file);
  assert_eq!(opts.font_size, 17.0);
  assert_eq!(diags.len(), 1);
  std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn load_path_reads_legacy_format() {
  let dir = std::env::temp_dir().join(format!("sinclairlegacytest{}", std::process::id()));
  std::fs::create_dir_all(&dir).unwrap();
  let file = dir.join("config");
  std::fs::write(&file, "font-size = 17\nbogus = 1\n").unwrap();
  let (opts, diags) = load_path(&file);
  assert_eq!(opts.font_size, 17.0);
  assert_eq!(diags.len(), 1);
  std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn default_path_shape() {
  // Whatever the environment, if a path comes back it must be the JSON
  // settings file.
  if let Some(p) = default_path() {
    assert!(p.ends_with("sinclair/settings.json"), "{p:?}");
  }
}

#[test]
fn legacy_path_shape() {
  if let Some(p) = legacy_path() {
    assert!(
      p.ends_with("sinclair/config") || p.ends_with("prompt/config"),
      "{p:?}"
    );
  }
}
