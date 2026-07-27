use super::*;

fn sbx() -> Sandbox {
    Sandbox::resolve(Some("sinclair-sbx-api-1"), Some("docker"), Some("/repo")).unwrap()
}

#[test]
fn no_name_means_a_host_launch() {
    assert!(Sandbox::resolve(None, Some("docker"), None).is_none());
    assert!(Sandbox::resolve(Some("  "), None, None).is_none());
}

#[test]
fn engine_defaults_to_docker() {
    let s = Sandbox::resolve(Some("c"), None, None).unwrap();
    assert_eq!(s.engine, Engine::Docker);
    assert_eq!(Sandbox::resolve(Some("c"), Some("podman"), None).unwrap().engine, Engine::Podman);
}

#[test]
fn loopback_endpoints_become_the_gateway_host() {
    // Inside a bridged container, 127.0.0.1 is the container itself; the bus
    // lives on the host.
    let s = sbx();
    assert_eq!(
        s.endpoint("http://127.0.0.1:7777/mcp"),
        "http://host.docker.internal:7777/mcp"
    );
    assert_eq!(
        s.endpoint("http://localhost:7777/mcp"),
        "http://host.docker.internal:7777/mcp"
    );
}

#[test]
fn podman_uses_its_own_gateway_name() {
    let s = Sandbox::resolve(Some("c"), Some("podman"), None).unwrap();
    assert_eq!(
        s.endpoint("http://127.0.0.1:7777/mcp"),
        "http://host.containers.internal:7777/mcp"
    );
}

#[test]
fn a_routable_endpoint_is_left_alone() {
    let s = sbx();
    assert_eq!(s.endpoint("http://10.0.0.4:7777/mcp"), "http://10.0.0.4:7777/mcp");
}

#[test]
fn wrap_execs_through_a_login_shell_in_the_workdir() {
    let argv = sbx().wrap("claude", &["--version".to_string()], &[]);
    assert_eq!(&argv[..3], &["docker", "exec", "-it"]);
    let w = argv.iter().position(|a| a == "-w").unwrap();
    assert_eq!(argv[w + 1], "/repo");
    assert_eq!(argv[argv.len() - 3], "sh");
    assert_eq!(argv[argv.len() - 2], "-lc");
    assert_eq!(argv[argv.len() - 1], "exec 'claude' '--version'");
}

#[test]
fn env_reaches_the_container() {
    // Codex takes its bearer token by env var; it has to survive the wrap.
    let argv = sbx().wrap("codex", &[], &[("RELAY_TOKEN".to_string(), "secret".to_string())]);
    assert!(argv.windows(2).any(|w| w[0] == "-e" && w[1] == "RELAY_TOKEN=secret"));
}

#[test]
fn prompts_with_quotes_and_newlines_survive() {
    let prompt = "You're the lead.\nReport to 'supervisor'.";
    let argv = sbx().wrap("claude", &["-p".to_string(), prompt.to_string()], &[]);
    let line = argv.last().unwrap();
    assert!(line.contains(r"'\''"), "single quotes must be escaped, not dropped");
    assert!(line.contains('\n'), "the prompt keeps its newlines");
}

#[test]
fn workdir_defaults_when_unset() {
    assert_eq!(Sandbox::resolve(Some("c"), None, None).unwrap().workdir, ".");
}
