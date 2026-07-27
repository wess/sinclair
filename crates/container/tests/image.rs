use super::*;

#[test]
fn default_base_when_unset() {
    let d = Recipe::default().dockerfile();
    assert!(d.starts_with(&format!("FROM {DEFAULT_BASE}\n")));
}

#[test]
fn base_is_overridable() {
    let r = Recipe {
        base: "ubuntu:24.04".to_string(),
        ..Recipe::default()
    };
    assert!(r.dockerfile().starts_with("FROM ubuntu:24.04\n"));
}

#[test]
fn agents_install_their_npm_packages() {
    let r = Recipe::with_agents(&["claude".to_string(), "codex".to_string()]);
    let d = r.dockerfile();
    assert!(d.contains("npm install -g @anthropic-ai/claude-code @openai/codex"));
}

#[test]
fn unknown_agents_install_nothing() {
    let r = Recipe::with_agents(&["ollama".to_string()]);
    assert!(!r.dockerfile().contains("npm install -g"));
}

#[test]
fn home_is_world_writable_for_arbitrary_uids() {
    // A fresh named volume inherits these permissions, which is what lets the
    // sandbox run as --user <uid> on Linux and still write credentials.
    let d = Recipe::default().dockerfile();
    assert!(d.contains(&format!("chmod 0777 {HOME_DIR}")));
    assert!(d.contains(&format!("ENV HOME={HOME_DIR}")));
}

#[test]
fn extra_packages_merge_without_duplicating() {
    let r = Recipe {
        packages: vec!["git".to_string(), "ripgrep".to_string()],
        ..Recipe::default()
    };
    let d = r.dockerfile();
    assert!(d.contains("ripgrep"));
    assert_eq!(d.matches(" git ").count(), 1);
}

#[test]
fn setup_lines_become_run_layers() {
    let r = Recipe {
        setup: vec!["cargo install just".to_string(), "  ".to_string()],
        ..Recipe::default()
    };
    let d = r.dockerfile();
    assert!(d.contains("RUN cargo install just\n"));
    assert!(!d.contains("RUN \n"));
}

#[test]
fn tag_tracks_the_recipe() {
    let a = Recipe::default().tag();
    let b = Recipe::default().tag();
    let c = Recipe::with_agents(&["claude".to_string()]).tag();
    assert_eq!(a, b, "an unchanged recipe must be a cache hit");
    assert_ne!(a, c, "a changed recipe must not reuse a stale image");
    assert!(a.starts_with("sinclair-sandbox:"));
}

#[test]
fn build_reads_the_dockerfile_from_stdin() {
    let argv = Recipe::default().build_argv(Engine::Docker);
    assert_eq!(argv[0], "docker");
    assert_eq!(argv[1], "build");
    assert_eq!(argv.last().unwrap(), "-", "no build context directory");
}

#[test]
fn exists_argv_is_quiet() {
    assert_eq!(
        exists_argv(Engine::Podman, "sinclair-sandbox:abc"),
        vec!["podman", "images", "-q", "sinclair-sandbox:abc"]
    );
}
