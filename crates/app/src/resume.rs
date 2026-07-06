//! Rewrite an agent launch command to resume its native session after a
//! restart. When an agent reports a native session id (via its hooks; see
//! `agenthooks.rs`), that id is persisted alongside the command that launched
//! the pane. On session restore, a recognized agent CLI is relaunched pointed at
//! its old session instead of starting fresh; anything unrecognized (or already
//! carrying a resume flag) is relaunched unchanged.

/// Produce a command that resumes `session` for the agent originally launched by
/// `command`. Recognizes a small set of agent CLIs by program name; returns the
/// command unchanged when the program is unknown or already resumes a session.
pub fn resume_command(command: &str, session: &str) -> String {
    let session = session.trim();
    if session.is_empty() {
        return command.to_string();
    }
    // Already resuming something — don't stack a second resume flag.
    if command.contains("--resume") || command.contains(" resume ") {
        return command.to_string();
    }
    match program_base(command) {
        "claude" => format!("{command} --resume {session}"),
        "codex" => format!("{command} resume {session}"),
        _ => command.to_string(),
    }
}

/// The basename of a command's program (its first whitespace-delimited token,
/// after any directory). `"/usr/bin/claude --foo"` → `"claude"`.
fn program_base(command: &str) -> &str {
    let program = command.split_whitespace().next().unwrap_or("");
    program.rsplit('/').next().unwrap_or(program)
}

#[cfg(test)]
#[path = "../tests/resume.rs"]
mod tests;
