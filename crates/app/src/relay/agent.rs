//! Building the command line that launches a coding agent: provider
//! resolution, argument assembly, prompt minimization, and shell quoting.

use super::*;

/// How to launch a provider: a built-in `--agent` (with an optional explicit
/// `--bin` path), or a custom `--cmd` template, plus any extra CLI flags the
/// user configured for it.
struct Resolved {
    agent: Option<String>,
    bin: Option<String>,
    custom: Option<String>,
    args: Vec<String>,
}

/// Split a configured flags string into individual argv tokens, honoring simple
/// single/double quoting so a flag value with spaces stays one argument.
pub(crate) fn split_args(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut has = false;
    for c in s.chars() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => cur.push(c),
            None if c == '\'' || c == '"' => {
                quote = Some(c);
                has = true;
            }
            None if c.is_whitespace() => {
                if has {
                    out.push(std::mem::take(&mut cur));
                    has = false;
                }
            }
            None => {
                cur.push(c);
                has = true;
            }
        }
    }
    if has {
        out.push(cur);
    }
    out
}

/// Resolve a provider label to its launch shape using the configured paths and
/// custom tools. Unknown labels fall back to `--agent <label>`.
fn resolve_provider(opts: &config::Options, provider: &str) -> Resolved {
    let bin = |p: &Option<String>| p.clone().filter(|s| !s.trim().is_empty());
    let args = |a: &Option<String>| a.as_deref().map(split_args).unwrap_or_default();
    match provider {
        "claude" => Resolved {
            agent: Some("claude".into()),
            bin: bin(&opts.agent_claude_path),
            custom: None,
            args: args(&opts.agent_claude_args),
        },
        "codex" => Resolved {
            agent: Some("codex".into()),
            bin: bin(&opts.agent_codex_path),
            custom: None,
            args: args(&opts.agent_codex_args),
        },
        "gemini" => Resolved {
            agent: Some("gemini".into()),
            bin: bin(&opts.agent_gemini_path),
            custom: None,
            args: args(&opts.agent_gemini_args),
        },
        "ollama" => Resolved {
            agent: Some("ollama".into()),
            bin: None,
            custom: None,
            args: Vec::new(),
        },
        other => {
            if let Some((_, tmpl)) = custom_tools(opts).into_iter().find(|(l, _)| l == other) {
                Resolved {
                    agent: None,
                    bin: None,
                    custom: Some(tmpl),
                    args: Vec::new(),
                }
            } else {
                Resolved {
                    agent: Some(other.to_string()),
                    bin: None,
                    custom: None,
                    args: Vec::new(),
                }
            }
        }
    }
}

/// The extra CLI flags configured for `provider`, already split into argv
/// tokens. Empty for a provider with no flags field (ollama) and for a custom
/// tool, whose command template carries its own.
pub(crate) fn provider_args(opts: &config::Options, provider: &str) -> Vec<String> {
    resolve_provider(opts, provider).args
}

/// Available role names (built-in + user + project), via the relay CLI.
pub fn role_list() -> Vec<String> {
    let Ok(out) = std::process::Command::new(binary())
        .args(["role", "list", "--json"])
        .output()
    else {
        return Vec::new();
    };
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or(serde_json::Value::Null);
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|r| r["name"].as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

/// Build a `relay launch` command for a specific provider/name/role-or-task.
/// `opts` supplies any configured explicit binary path or custom command
/// template for the provider.
pub fn launch_agent_command(
    opts: &config::Options,
    provider: &str,
    name: &str,
    role: Option<&str>,
    task: Option<&str>,
) -> String {
    let r = resolve_provider(opts, provider);
    let mut s = format!(
        "{} --home {} launch {}",
        sh_quote(&binary()),
        sh_quote(&home_str()),
        sh_quote(name)
    );
    if let Some(agent) = &r.agent {
        s.push_str(&format!(" --agent {}", sh_quote(agent)));
    }
    if let Some(bin) = &r.bin {
        s.push_str(&format!(" --bin {}", sh_quote(bin)));
    }
    if let Some(tmpl) = &r.custom {
        s.push_str(&format!(" --cmd {}", sh_quote(tmpl)));
    }
    if let Some(r) = role.filter(|r| !r.is_empty()) {
        s.push_str(&format!(" --role {}", sh_quote(r)));
    }
    if let Some(t) = task.filter(|t| !t.is_empty()) {
        let t = if opts.ai_optimize_tokens { minimize_prompt(t) } else { t.to_string() };
        if !t.is_empty() {
            s.push_str(&format!(" --task {}", sh_quote(&t)));
        }
    }
    if opts.ai_optimize_tokens {
        s.push_str(" --optimize");
    }
    for arg in &r.args {
        s.push_str(&format!(" --agent-arg {}", sh_quote(arg)));
    }
    keep_open(s)
}

/// Immediately launch a configured provider (Claude Code, Codex, …) as a one-off
/// agent — the quick-launch menu entries. Reuses [`launch_agent_command`] (so the
/// token-optimization threading applies) with a generated unique name, the
/// default `worker` role, and no standing task.
pub fn quick_launch_command(opts: &config::Options, provider: &str) -> String {
    let name = unique_agent_name(provider);
    launch_agent_command(opts, provider, &name, None, None)
}

/// A friendly display name for a provider, for menus. Built-ins get their brand
/// name; custom tools already carry a user-chosen label, so pass it through.
pub fn provider_label(provider: &str) -> String {
    match provider {
        "claude" => "Claude Code".to_string(),
        "codex" => "Codex".to_string(),
        "gemini" => "Gemini".to_string(),
        "ollama" => "Ollama".to_string(),
        other => other.to_string(),
    }
}

/// A mesh name unlikely to collide across quick launches: the provider plus a
/// short suffix derived from the wall clock (seconds since the epoch, base-36).
fn unique_agent_name(provider: &str) -> String {
    let base: String = provider
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect();
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{base}-{}", radix36(secs))
}

/// Lower-case base-36 encoding of `n` (0-9a-z), for compact, readable suffixes.
fn radix36(mut n: u64) -> String {
    if n == 0 {
        return "0".to_string();
    }
    const DIGITS: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut out = Vec::new();
    while n > 0 {
        out.push(DIGITS[(n % 36) as usize]);
        n /= 36;
    }
    out.reverse();
    String::from_utf8(out).unwrap()
}

/// Compact a prompt to spend fewer tokens without dropping content: strip
/// trailing whitespace from every line, collapse runs of spaces/tabs that
/// follow the leading indent into a single space (indentation is preserved so
/// pasted code keeps its shape), and squeeze runs of blank lines down to one.
/// Outer blank lines are trimmed off entirely.
pub(crate) fn minimize_prompt(text: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut blank_run = false;
    for line in text.lines() {
        let indent: String = line.chars().take_while(|c| *c == ' ' || *c == '\t').collect();
        let body = &line[indent.len()..];
        let mut compact = String::with_capacity(body.len());
        let mut prev_space = false;
        for c in body.chars() {
            let is_space = c == ' ' || c == '\t';
            if is_space {
                if !prev_space {
                    compact.push(' ');
                }
            } else {
                compact.push(c);
            }
            prev_space = is_space;
        }
        let joined = format!("{indent}{}", compact.trim_end());
        if joined.trim().is_empty() {
            if !out.is_empty() {
                blank_run = true;
            }
        } else {
            if blank_run {
                out.push(String::new());
            }
            blank_run = false;
            out.push(joined);
        }
    }
    out.join("\n")
}

/// Single-quote a value for safe interpolation into a `/bin/sh -c` string:
/// wrap it in single quotes and escape any embedded single quote, making the
/// content inert to the shell (no word-splitting, globbing, or `$()`/`;`).
pub(crate) fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Wrap a launch command so a failure leaves the pane open with the reason
/// (instead of the shell exiting and the pane vanishing). On success the agent
/// `exec`s and replaces the shell, so the fallback never runs.
pub(crate) fn keep_open(cmd: String) -> String {
    format!(
        "{cmd} || {{ echo; echo '[relay] launch failed — check Settings → AI (is the server running?)'; exec \"${{SHELL:-/bin/sh}}\"; }}"
    )
}
