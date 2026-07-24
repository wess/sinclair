use super::*;

#[test]
fn plain_values_are_single_quoted() {
    assert_eq!(sh_quote("claude"), "'claude'");
    assert_eq!(sh_quote("my agent"), "'my agent'");
}

#[test]
fn shell_metacharacters_are_neutralized() {
    // Each stays inside one quoted word — no command runs.
    assert_eq!(sh_quote("a; rm -rf /"), "'a; rm -rf /'");
    assert_eq!(sh_quote("$(whoami)"), "'$(whoami)'");
    assert_eq!(sh_quote("a && b"), "'a && b'");
}

#[test]
fn embedded_single_quotes_are_escaped() {
    // The close/escape/reopen dance keeps the value a single shell token.
    assert_eq!(sh_quote("x'; rm -rf /;'"), "'x'\\''; rm -rf /;'\\'''");
}

#[test]
fn minimize_squeezes_whitespace_and_blank_lines() {
    let input = "Fix   the   bug  \n\n\n\nin   the parser\n";
    assert_eq!(minimize_prompt(input), "Fix the bug\n\nin the parser");
}

#[test]
fn minimize_preserves_indentation_and_content() {
    // Leading indent survives so pasted code keeps its shape; only runs after
    // the indent collapse. No words are dropped.
    let input = "    let x =    1;\n\thelp   me   please";
    assert_eq!(minimize_prompt(input), "    let x = 1;\n\thelp me please");
}

#[test]
fn minimize_trims_outer_blank_lines() {
    assert_eq!(minimize_prompt("\n\n  hello  \n\n"), "  hello");
}

#[test]
fn split_args_tokenizes_on_whitespace() {
    assert_eq!(
        split_args("--dangerously-skip-permissions --foo"),
        vec!["--dangerously-skip-permissions", "--foo"]
    );
}

#[test]
fn split_args_keeps_quoted_values_together() {
    assert_eq!(
        split_args("--append-system-prompt \"be terse\" --x"),
        vec!["--append-system-prompt", "be terse", "--x"]
    );
    assert_eq!(split_args("   "), Vec::<String>::new());
}

#[test]
fn extract_json_pulls_the_object_from_a_wrapped_reply() {
    let reply = "Sure! Here's the team:\n```json\n{\"name\":\"web\",\"members\":[{\"name\":\"lead\"}]}\n```\nHope that helps.";
    let json = extract_json(reply).unwrap();
    let spec: TeamSpec = serde_json::from_str(json).unwrap();
    assert_eq!(spec.name, "web");
    assert_eq!(spec.members.len(), 1);
}

#[test]
fn extract_json_balances_nested_braces() {
    let reply = "{\"a\":{\"b\":1},\"c\":2} trailing junk }";
    assert_eq!(extract_json(reply).unwrap(), "{\"a\":{\"b\":1},\"c\":2}");
    assert!(extract_json("no json here").is_none());
}

#[test]
fn launch_member_quotes_hostile_values() {
    let cmd = launch_member(
        &config::Options::default(),
        "x'; rm -rf /;'",
        "worker role",
        "$(whoami)",
        true,
        false,
    );
    // Every interpolated value stays a single quoted shell token.
    assert!(cmd.contains(" launch 'x'\\''; rm -rf /;'\\'''"), "member not quoted: {cmd}");
    assert!(cmd.contains("--role 'worker role'"), "role not quoted: {cmd}");
    assert!(cmd.contains("--agent '$(whoami)'"), "agent not quoted: {cmd}");
    assert!(cmd.contains(" --lead"));
}

#[test]
fn launch_member_omits_empty_agent_flag() {
    let cmd = launch_member(&config::Options::default(), "lead", "supervisor", "  ", false, true);
    assert!(!cmd.contains("--agent "));
    assert!(cmd.contains("--optimize"));
    assert!(cmd.contains(" launch 'lead' --role 'supervisor'"));
}

/// A team fills every split at once, so no human is sitting in a member's pane
/// to answer a permission prompt. Autonomy is on by default, and the flag is
/// provider-agnostic so it still lands when the member inherits its role's
/// agent (no `--agent` to key a per-provider flag off).
#[test]
fn team_members_launch_unattended_by_default() {
    let opts = config::Options::default();
    assert!(opts.relay_team_autonomy);
    let cmd = launch_member(&opts, "backend", "backend", "", false, false);
    assert!(cmd.contains(" --skip-permissions"), "no bypass: {cmd}");
    // Including the lead, so the whole team behaves the same way.
    let lead = launch_member(&opts, "lead", "supervisor", "", true, false);
    assert!(lead.contains(" --skip-permissions"), "lead differs: {lead}");
}

#[test]
fn team_autonomy_off_keeps_the_prompts() {
    let opts = config::Options {
        relay_team_autonomy: false,
        ..config::Options::default()
    };
    let cmd = launch_member(&opts, "backend", "backend", "claude", false, false);
    assert!(!cmd.contains("--skip-permissions"), "bypassed anyway: {cmd}");
}

/// A member with a named provider also picks up that provider's configured
/// flags, which only solo launches used to get.
#[test]
fn launch_member_forwards_configured_provider_flags() {
    let opts = config::Options {
        agent_claude_args: Some("--append-system-prompt \"be terse\"".into()),
        ..config::Options::default()
    };
    let cmd = launch_member(&opts, "backend", "backend", "claude", false, false);
    assert!(cmd.contains("--agent-arg '--append-system-prompt'"), "{cmd}");
    assert!(cmd.contains("--agent-arg 'be terse'"), "{cmd}");
    // A member inheriting its role's agent has no provider to look flags up
    // under, so it gets none — the permission bypass covers it instead.
    let inherited = launch_member(&opts, "backend", "backend", "", false, false);
    assert!(!inherited.contains("--agent-arg"), "{inherited}");
}

/// The roster maps onto the pane tree one member per leaf, in the order the
/// realizer walks it: member 0 lands in the window's first pane as the lead,
/// and every member is launchable.
#[test]
fn team_layout_gives_every_member_a_pane() {
    let opts = config::Options::default();
    let members: Vec<TeamMember> = ["lead", "frontend", "backend"]
        .iter()
        .map(|n| (n.to_string(), "worker".to_string(), String::new()))
        .collect();
    let panes = team_layout(&opts, "columns", &members);
    assert_eq!(panes.layout.leaves(), members.len(), "a member with no pane can't launch");
    assert_eq!(panes.commands.len(), members.len());
    assert!(panes.commands.iter().all(Option::is_some));
    assert!(panes.commands[0].as_ref().unwrap().contains(" --lead"));
    assert!(!panes.commands[1].as_ref().unwrap().contains(" --lead"));
    // Every pane in a team window is an agent, so every one runs unattended.
    assert!(panes
        .commands
        .iter()
        .all(|c| c.as_ref().unwrap().contains(" --skip-permissions")));
    // Pane titles are the member names — the launch runs under a shell, so an
    // untitled pane's tab reads `zsh` and every member looks alike.
    assert_eq!(panes.names, vec!["lead", "frontend", "backend"]);
}

/// A one-member team is a single pane, not a split with an empty side.
#[test]
fn team_layout_handles_a_lone_member() {
    let members = vec![("solo".to_string(), "supervisor".to_string(), String::new())];
    let panes = team_layout(&config::Options::default(), "grid", &members);
    assert_eq!(panes.layout.leaves(), 1);
    assert_eq!(panes.commands.len(), 1);
    assert_eq!(panes.names, vec!["solo"]);
}

#[test]
fn health_response_requires_the_relay_marker() {
    let ok = "HTTP/1.1 200 OK\r\nContent-Length: 11\r\n\r\nrelay 0.1.0";
    assert!(relay_health_response(ok));
    // Daemons before the marker answered a bare "ok".
    let legacy = "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok";
    assert!(relay_health_response(legacy));
    // A foreign service on the port must not read as a running relay.
    let foreign = "HTTP/1.1 200 OK\r\nContent-Length: 9\r\n\r\nIt works!";
    assert!(!relay_health_response(foreign));
    assert!(!relay_health_response("HTTP/1.1 404 Not Found\r\n\r\n"));
    assert!(!relay_health_response(""));
}
