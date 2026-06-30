use super::*;

#[test]
fn ps_argv_shape() {
    let argv = ps_argv(Engine::Docker);
    assert_eq!(argv[0], "docker");
    assert_eq!(argv[1], "ps");
    assert!(argv.last().unwrap().contains("{{.ID}}"));
}

#[test]
fn parse_ps_rows() {
    let sep = "\u{1f}";
    let out = format!(
        "abc123{sep}web{sep}debian:latest{sep}Up 3 minutes\n\
         def456{sep}db{sep}postgres:16{sep}Up 1 hour (healthy)\n"
    );
    let rows = parse_ps(&out);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].id, "abc123");
    assert_eq!(rows[0].name, "web");
    assert_eq!(rows[0].image, "debian:latest");
    assert_eq!(rows[0].status, "Up 3 minutes");
    assert_eq!(rows[1].name, "db");
}

#[test]
fn parse_ps_skips_blank_and_malformed() {
    let sep = "\u{1f}";
    let out = format!("\n{sep}{sep}{sep}\nzzz{sep}only-id-and-name\n");
    let rows = parse_ps(&out);
    // Blank line and the all-empty line are dropped; the partial row keeps id.
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, "zzz");
    assert_eq!(rows[0].name, "only-id-and-name");
    assert_eq!(rows[0].image, "");
}

#[test]
fn exec_argv_shape() {
    assert_eq!(
        exec_argv(Engine::Docker, "abc123", "bash"),
        vec!["docker", "exec", "-it", "abc123", "bash"]
    );
    // Empty command falls back to sh.
    assert_eq!(
        exec_argv(Engine::Podman, "x", "  "),
        vec!["podman", "exec", "-it", "x", "sh"]
    );
}

#[test]
fn attach_argv_prefers_bash_with_fallback() {
    let argv = attach_argv(Engine::Docker, "abc123");
    assert_eq!(&argv[0..4], &["docker", "exec", "-it", "abc123"]);
    assert_eq!(argv[4], "sh");
    assert_eq!(argv[5], "-c");
    // The fallback script is one argv element.
    assert!(argv[6].contains("exec bash") && argv[6].contains("exec sh"));
    assert_eq!(argv.len(), 7);
}
