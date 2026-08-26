use super::*;

#[test]
fn validates_plugin_names() {
  assert!(valid_name("git"));
  assert!(valid_name("k9s"));
  assert!(valid_name("lazy-docker"));
  assert!(!valid_name(""));
  assert!(!valid_name("../etc"));
  assert!(!valid_name("a/b"));
  assert!(!valid_name("Git"));
  assert!(!valid_name("has space"));
}

#[test]
fn validates_file_names() {
  assert!(valid_file("plugin.toml"));
  assert!(valid_file("readme.md"));
  assert!(!valid_file(".."));
  assert!(!valid_file("a/b.txt"));
  assert!(!valid_file(""));
  assert!(!valid_file(".bashrc")); // no dotfiles
  assert!(!valid_file(".netrc"));
}
