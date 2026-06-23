use super::*;

#[test]
fn missing_file_yields_defaults() {
    let (opts, diags) = load_path(std::path::Path::new("/nonexistent/prompt/config"));
    assert_eq!(opts, Options::default());
    assert!(diags.is_empty());
}

#[test]
fn load_path_reads_file() {
    let dir = std::env::temp_dir().join(format!("promptconfigtest{}", std::process::id()));
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
    // Whatever the environment, if a path comes back it must end with
    // prompt/config.
    if let Some(p) = default_path() {
        assert!(p.ends_with("prompt/config"), "{p:?}");
    }
}
