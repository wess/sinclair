use super::*;

#[test]
fn claude_gets_resume_flag() {
    assert_eq!(
        resume_command("claude --model opus", "abc-123"),
        "claude --model opus --resume abc-123"
    );
}

#[test]
fn codex_gets_resume_subcommand() {
    assert_eq!(resume_command("codex", "sess-9"), "codex resume sess-9");
}

#[test]
fn honors_program_path() {
    assert_eq!(
        resume_command("/opt/bin/claude", "s1"),
        "/opt/bin/claude --resume s1"
    );
}

#[test]
fn unknown_program_unchanged() {
    assert_eq!(resume_command("relay launch x --agent claude", "s1"),
        "relay launch x --agent claude");
    assert_eq!(resume_command("bash -lc echo", "s1"), "bash -lc echo");
}

#[test]
fn already_resuming_unchanged() {
    assert_eq!(
        resume_command("claude --resume old", "new"),
        "claude --resume old"
    );
    assert_eq!(resume_command("codex resume old", "new"), "codex resume old");
}

#[test]
fn empty_session_unchanged() {
    assert_eq!(resume_command("claude", ""), "claude");
    assert_eq!(resume_command("claude", "   "), "claude");
}
