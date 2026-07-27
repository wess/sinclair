use super::*;
use container::{DevContainer, Engine, Network};
// Project resolution lives beside `spec`, not in it.
use crate::sandbox::{repo_root, too_broad_to_mount};

const PROJECT: &str = "/Users/wess/code/api";

fn env<'a>(dc: Option<&'a DevContainer>) -> Env<'a> {
    Env {
        engine: Engine::Docker,
        project: PROJECT,
        devcontainer: dc,
        host_user: Some("501:20".to_string()),
        relay_dir: None,
        default_agent: "claude",
    }
}

fn opts() -> config::Options {
    config::Options::default()
}

#[test]
fn defaults_build_an_image_with_the_default_agent() {
    let s = build(&opts(), &env(None));
    let recipe = s.recipe.expect("a recipe when no image is pinned");
    assert!(recipe.dockerfile().contains("@anthropic-ai/claude-code"));
    assert_eq!(s.sandbox.image, recipe.tag());
}

#[test]
fn an_explicit_image_is_used_verbatim_and_nothing_is_built() {
    let mut o = opts();
    o.sandbox_image = Some("ghcr.io/me/dev:latest".to_string());
    let s = build(&o, &env(None));
    assert_eq!(s.sandbox.image, "ghcr.io/me/dev:latest");
    assert!(s.recipe.is_none());
}

#[test]
fn the_project_is_identity_mounted() {
    // The whole point: a path means the same thing inside and out, so a git
    // worktree created on either side resolves from the other.
    let s = build(&opts(), &env(None));
    assert_eq!(s.sandbox.workdir, PROJECT);
    assert!(s.sandbox.mounts.iter().any(|m| m.is_identity() && m.source == PROJECT));
}

#[test]
fn the_agent_home_is_a_named_volume() {
    // So one `claude login` survives recreating the container.
    let s = build(&opts(), &env(None));
    assert!(s
        .sandbox
        .mounts
        .iter()
        .any(|m| m.target == container::HOME_DIR && m.source.ends_with("-home")));
}

#[test]
fn a_devcontainer_image_becomes_the_base_layer() {
    // Agents land in the project's own toolchain, with the CLI added on top.
    let dc = DevContainer {
        image: Some("node:22".to_string()),
        ..DevContainer::default()
    };
    let s = build(&opts(), &env(Some(&dc)));
    let recipe = s.recipe.expect("still builds, to add the agent CLI");
    assert!(recipe.dockerfile().starts_with("FROM node:22\n"));
}

#[test]
fn a_devcontainer_that_stops_on_close_is_called_out() {
    let dc = DevContainer::default();
    let s = build(&opts(), &env(Some(&dc)));
    assert!(s.notes.iter().any(|n| n.contains("shutdownAction")));
}

#[test]
fn shutdown_action_none_is_not_flagged() {
    let dc = DevContainer {
        shutdown_action: Some("none".to_string()),
        ..DevContainer::default()
    };
    let s = build(&opts(), &env(Some(&dc)));
    assert!(!s.notes.iter().any(|n| n.contains("shutdownAction")));
}

#[test]
fn devcontainer_env_reaches_the_container() {
    let dc = DevContainer {
        env: vec![("TOKEN".to_string(), "abc".to_string())],
        ..DevContainer::default()
    };
    let s = build(&opts(), &env(Some(&dc)));
    assert!(s.sandbox.env.contains(&("TOKEN".to_string(), "abc".to_string())));
}

#[test]
fn settings_env_wins_over_the_devcontainer() {
    let dc = DevContainer {
        env: vec![("TOKEN".to_string(), "from-dc".to_string())],
        ..DevContainer::default()
    };
    let mut o = opts();
    o.sandbox_env = vec!["TOKEN=from-settings".to_string()];
    let s = build(&o, &env(Some(&dc)));
    assert!(s.sandbox.env.contains(&("TOKEN".to_string(), "from-settings".to_string())));
    assert_eq!(s.sandbox.env.iter().filter(|(k, _)| k == "TOKEN").count(), 1);
}

#[test]
fn bad_mounts_and_env_are_notes_not_failures() {
    let mut o = opts();
    o.sandbox_mount = vec!["/a:relative".to_string(), "/good:/mnt".to_string()];
    o.sandbox_env = vec!["NOEQUALS".to_string()];
    let s = build(&o, &env(None));
    assert!(s.sandbox.mounts.iter().any(|m| m.target == "/mnt"));
    assert_eq!(s.notes.iter().filter(|n| n.contains("sandbox-mount")).count(), 1);
    assert!(s.notes.iter().any(|n| n.contains("sandbox-env")));
}

#[test]
fn network_none_warns_that_the_bus_is_unreachable() {
    let mut o = opts();
    o.sandbox_network = Some("none".to_string());
    let s = build(&o, &env(None));
    assert_eq!(s.sandbox.network, Network::None);
    assert!(s.notes.iter().any(|n| n.contains("relay bus")));
}

#[test]
fn an_unknown_network_falls_back_with_a_note() {
    let mut o = opts();
    o.sandbox_network = Some("quantum".to_string());
    let s = build(&o, &env(None));
    assert_eq!(s.sandbox.network, Network::Bridge);
    assert!(s.notes.iter().any(|n| n.contains("sandbox-network")));
}

#[test]
fn user_host_resolves_to_the_projects_owner() {
    let mut o = opts();
    o.sandbox_user = Some("host".to_string());
    assert_eq!(build(&o, &env(None)).sandbox.user.as_deref(), Some("501:20"));
}

#[test]
fn user_root_leaves_the_flag_off() {
    let mut o = opts();
    o.sandbox_user = Some("root".to_string());
    assert!(build(&o, &env(None)).sandbox.user.is_none());
}

#[test]
fn an_explicit_uid_passes_through() {
    let mut o = opts();
    o.sandbox_user = Some("1000:1000".to_string());
    assert_eq!(build(&o, &env(None)).sandbox.user.as_deref(), Some("1000:1000"));
}

#[test]
fn a_pid_ceiling_is_always_set() {
    // A supervisor can spawn its cap of workers into this one container.
    assert!(build(&opts(), &env(None)).sandbox.limits.pids.is_some());
}

#[test]
fn limits_come_from_settings() {
    let mut o = opts();
    o.sandbox_memory = Some("8g".to_string());
    o.sandbox_cpus = Some("4".to_string());
    let s = build(&o, &env(None));
    assert_eq!(s.sandbox.limits.memory.as_deref(), Some("8g"));
    assert_eq!(s.sandbox.limits.cpus.as_deref(), Some("4"));
}

#[test]
fn a_dockerfile_devcontainer_says_what_to_do_instead() {
    let dc = DevContainer {
        dockerfile: Some("Dockerfile".to_string()),
        ..DevContainer::default()
    };
    let s = build(&opts(), &env(Some(&dc)));
    assert!(s.notes.iter().any(|n| n.contains("sandbox-image")));
}

#[test]
fn a_trailing_slash_does_not_make_a_second_sandbox() {
    let mut e = env(None);
    let a = build(&opts(), &e).sandbox.name;
    e.project = "/Users/wess/code/api/";
    assert_eq!(a, build(&opts(), &e).sandbox.name);
}

#[test]
fn home_and_shallow_paths_are_refused() {
    // The sandbox identity-mounts what it is given. Handing it $HOME would give
    // an agent with its prompts bypassed the user's whole machine.
    let home = std::env::var("HOME").unwrap_or_else(|_| "/Users/wess".into());
    assert!(too_broad_to_mount(&home));
    assert!(too_broad_to_mount(&format!("{home}/")));
    assert!(too_broad_to_mount("/"));
    assert!(too_broad_to_mount(""));
    assert!(too_broad_to_mount("/Users"));
    assert!(too_broad_to_mount("/home"));
}

#[test]
fn a_real_project_path_is_allowed() {
    assert!(!too_broad_to_mount("/Users/wess/code/api"));
    assert!(!too_broad_to_mount("/srv/www/site"));
}

#[test]
fn repo_root_walks_up_from_a_subdirectory() {
    // Any pane inside a repo must resolve to the same project, or a pane three
    // directories down would get a container of its own.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .unwrap()
        .to_path_buf();
    let deep = root.join("crates/app/src/root");
    assert_eq!(repo_root(&deep).as_deref(), Some(root.as_path()));
    assert_eq!(repo_root(&root).as_deref(), Some(root.as_path()));
}

#[test]
fn no_repo_above_a_path_is_none() {
    assert!(repo_root(std::path::Path::new("/")).is_none());
}
