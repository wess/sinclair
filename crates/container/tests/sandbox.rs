use super::*;

fn sbx() -> Sandbox {
    Sandbox::for_project(
        Engine::Docker,
        "/Users/wess/code/api",
        "sinclair-sandbox:abc",
    )
}

#[test]
fn name_is_deterministic_and_project_scoped() {
    let a = name_for("/Users/wess/code/api");
    let b = name_for("/Users/wess/code/api");
    let c = name_for("/Users/wess/other/api");
    assert_eq!(a, b, "the same project must resolve to the same container");
    assert_ne!(a, c, "two checkouts named `api` must not collide");
    assert!(a.starts_with("sinclair-sbx-api-"));
}

#[test]
fn trailing_slash_is_the_same_project() {
    assert!(name_for("/Users/wess/code/api/").starts_with("sinclair-sbx-api-"));
}

#[test]
fn project_is_identity_mounted() {
    let s = sbx();
    assert_eq!(s.workdir, "/Users/wess/code/api");
    assert!(s.mounts[0].is_identity());
}

#[test]
fn create_argv_is_detached_and_keeps_alive() {
    let argv = sbx().create_argv();
    assert_eq!(&argv[..3], &["docker", "run", "-d"]);
    assert!(argv.contains(&"--init".to_string()));
    // Not `sleep infinity` — busybox rejects it.
    assert_eq!(
        &argv[argv.len() - 3..],
        &["sh", "-c", "while :; do sleep 3600; done"]
    );
}

#[test]
fn create_argv_carries_labels_and_gateway() {
    let argv = sbx().create_argv().join(" ");
    assert!(argv.contains("sinclair.sandbox=1"));
    assert!(argv.contains("sinclair.project=/Users/wess/code/api"));
    assert!(argv.contains("sinclair.owner=sinclair"));
    // Stamped so VS Code finds this container instead of building its own.
    assert!(argv.contains("devcontainer.local_folder=/Users/wess/code/api"));
    assert!(argv.contains("--add-host host.docker.internal:host-gateway"));
}

#[test]
fn podman_uses_its_own_gateway_name() {
    let s = Sandbox::for_project(Engine::Podman, "/repo", "img");
    assert!(s
        .create_argv()
        .join(" ")
        .contains("host.containers.internal:host-gateway"));
}

#[test]
fn host_network_skips_the_gateway_entry() {
    let mut s = sbx();
    s.network = Network::Host;
    let argv = s.create_argv().join(" ");
    assert!(argv.contains("--network host"));
    assert!(!argv.contains("--add-host"));
}

#[test]
fn limits_and_user_reach_the_argv() {
    let mut s = sbx();
    s.user = Some("501:20".to_string());
    s.limits = Limits {
        memory: Some("8g".to_string()),
        cpus: Some("4".to_string()),
        pids: Some(512),
    };
    let argv = s.create_argv().join(" ");
    assert!(argv.contains("--user 501:20"));
    assert!(argv.contains("--memory 8g"));
    assert!(argv.contains("--cpus 4"));
    assert!(argv.contains("--pids-limit 512"));
}

#[test]
fn shell_argv_is_a_login_shell_in_the_project() {
    let argv = sbx().shell_argv(None);
    assert_eq!(&argv[..3], &["docker", "exec", "-it"]);
    assert!(argv.contains(&"/Users/wess/code/api".to_string()));
    assert!(argv.last().unwrap().contains("exec bash -l"));
}

#[test]
fn exec_argv_honors_a_worktree_cwd() {
    let argv = sbx().exec_argv(
        "claude --version",
        Some("/Users/wess/code/api/.worktrees/ui"),
    );
    let w = argv.iter().position(|a| a == "-w").unwrap();
    assert_eq!(argv[w + 1], "/Users/wess/code/api/.worktrees/ui");
    assert_eq!(&argv[argv.len() - 2..], &["-lc", "claude --version"]);
}

#[test]
fn home_env_points_at_the_volume() {
    let s = sbx().with_home_volume("vol");
    assert!(s.env.iter().any(|(k, v)| k == "HOME" && v == HOME_DIR));
    assert!(s
        .mounts
        .iter()
        .any(|m| m.source == "vol" && m.target == HOME_DIR));
    // Env reaches an exec too, not just create — a bare `docker exec` would
    // otherwise land in the image's HOME and miss the agent's credentials.
    assert!(s
        .shell_argv(None)
        .join(" ")
        .contains(&format!("HOME={HOME_DIR}")));
}

#[test]
fn relay_dir_is_mounted_read_only() {
    let s = sbx().with_relay_dir("/host/relay");
    let m = s.mounts.iter().find(|m| m.target == RELAY_DIR).unwrap();
    assert!(m.readonly);
}

#[test]
fn states_parse() {
    assert_eq!(parse_state(""), State::Missing);
    assert_eq!(parse_state("running\n"), State::Running);
    assert_eq!(parse_state("exited"), State::Exited);
    assert_eq!(parse_state("created"), State::Created);
    assert_eq!(parse_state("paused"), State::Paused);
    assert!(State::Running.is_running());
    assert!(State::Exited.is_stopped());
    assert!(!State::Missing.is_stopped());
}

#[test]
fn state_argv_anchors_the_name() {
    let argv = state_argv(Engine::Docker, "sinclair-sbx-api-1234");
    assert!(argv.contains(&"name=^sinclair-sbx-api-1234$".to_string()));
    assert!(argv.contains(&"-a".to_string()));
}

#[test]
fn network_parses() {
    assert_eq!(Network::parse("host"), Some(Network::Host));
    assert_eq!(Network::parse(" None "), Some(Network::None));
    assert_eq!(Network::parse("bridge"), Some(Network::Bridge));
    assert_eq!(Network::parse("wat"), None);
}

#[test]
fn home_volume_tracks_the_container_name() {
    assert_eq!(
        home_volume_for("/Users/wess/code/api"),
        format!("{}-home", name_for("/Users/wess/code/api"))
    );
}
