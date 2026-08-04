//! promptdesigner — design a shell prompt by clicking, preview it in its real
//! colours, then apply it.
//!
//! Runs in the WASM sandbox. The design lives in the ungated per-plugin storage
//! rather than a file in `$HOME`, and applying it runs a visible command in the
//! terminal instead of a silent rewrite of your rc file — a plugin that edits
//! your shell config should show you the edit.

wit_bindgen::generate!({
    world: "designer",
    path: "../../crates/pluginrt/wit",
});

use crate::exports::prompt::plugin::guest::Guest;
use crate::prompt::plugin::host_commands::run_command;
use crate::prompt::plugin::host_core::{storage_get, storage_set};
use crate::prompt::plugin::types::CommandTarget;

use serde_json::{json, Value};

/// Symbols offered for the prompt tip. An allowlist, never free text: the value
/// is interpolated into a shell snippet, so it must not be able to carry syntax.
const SYMBOLS: &[&str] = &["\u{276f}", "\u{279c}", "$", "\u{3bb}", "\u{bb}"];

/// Colours offered, in both the panel preview and the generated snippet.
/// Allowlisted for the same reason as [`SYMBOLS`].
const COLORS: &[&str] = &["cyan", "green", "blue", "magenta", "yellow", "red", "white"];

/// Where the generated snippet is written, and the markers that fence it in the
/// rc file so re-applying replaces rather than accumulates.
const PROMPT_FILE: &str = "$HOME/.config/prompt-designer/prompt.sh";
const BEGIN: &str = "# >>> prompt-designer >>>";
const END: &str = "# <<< prompt-designer <<<";

struct Designer;

/// One prompt design.
#[derive(Clone)]
struct Design {
    userhost: bool,
    cwd: bool,
    git: bool,
    time: bool,
    symbol: String,
    color: String,
}

impl Default for Design {
    fn default() -> Self {
        Self {
            userhost: false,
            cwd: true,
            git: true,
            time: false,
            symbol: SYMBOLS[0].to_string(),
            color: "cyan".to_string(),
        }
    }
}

impl Design {
    fn load() -> Self {
        let Some(raw) = storage_get("design") else {
            return Self::default();
        };
        let Ok(v) = serde_json::from_str::<Value>(&raw) else {
            return Self::default();
        };
        let d = Self::default();
        let flag = |k: &str, fallback: bool| v.get(k).and_then(Value::as_bool).unwrap_or(fallback);
        // An unknown symbol or colour in stored state falls back to the default
        // rather than reaching the snippet — storage is not a trust boundary.
        let pick = |k: &str, allowed: &[&str], fallback: &str| {
            v.get(k)
                .and_then(Value::as_str)
                .filter(|s| allowed.contains(s))
                .unwrap_or(fallback)
                .to_string()
        };
        Self {
            userhost: flag("userhost", d.userhost),
            cwd: flag("cwd", d.cwd),
            git: flag("git", d.git),
            time: flag("time", d.time),
            symbol: pick("symbol", SYMBOLS, &d.symbol),
            color: pick("color", COLORS, &d.color),
        }
    }

    fn save(&self) {
        storage_set(
            "design",
            &json!({
                "userhost": self.userhost,
                "cwd": self.cwd,
                "git": self.git,
                "time": self.time,
                "symbol": self.symbol,
                "color": self.color,
            })
            .to_string(),
        );
    }

    /// The preview, as the segments it is made of — so the panel can paint each
    /// one in the colour it will actually have.
    fn segments(&self) -> Vec<(String, &str)> {
        let mut parts: Vec<(String, &str)> = Vec::new();
        if self.userhost {
            parts.push(("you@host".to_string(), self.color.as_str()));
        }
        if self.cwd {
            parts.push(("~/dev/app".to_string(), self.color.as_str()));
        }
        // git always renders yellow, matching the generated snippet.
        if self.git {
            parts.push(("(main)".to_string(), "yellow"));
        }
        if self.time {
            parts.push(("14:32".to_string(), self.color.as_str()));
        }
        parts.push((self.symbol.clone(), self.color.as_str()));
        parts
    }

    /// The zsh `PROMPT` / bash `PS1` snippet. Only allowlisted symbol and colour
    /// values reach this, so nothing here can carry shell syntax.
    fn snippet(&self, shell: &str) -> String {
        let c = &self.color;
        let sym = &self.symbol;
        if shell == "bash" {
            let col = format!("\\[\\e[{}m\\]", ansi(c));
            let ylw = "\\[\\e[33m\\]";
            let off = "\\[\\e[0m\\]";
            let mut p = String::new();
            if self.userhost {
                p.push_str(&format!("{col}\\u@\\h{off} "));
            }
            if self.cwd {
                p.push_str(&format!("{col}\\w{off} "));
            }
            if self.git {
                p.push_str(&format!("{ylw}$(_pd_git){off}"));
            }
            if self.time {
                p.push_str(&format!("{col}\\t{off} "));
            }
            p.push_str(&format!("{col}{sym}{off} "));
            return [
                "# Generated by Prompt Designer \u{2014} edit in the app, not here.",
                "_pd_git() { local b; b=$(git rev-parse --abbrev-ref HEAD 2>/dev/null) || return; printf '(%s) ' \"$b\"; }",
                &format!("PS1='{p}'"),
                "",
            ]
            .join("\n");
        }
        let mut p = String::new();
        if self.userhost {
            p.push_str(&format!("%F{{{c}}}%n@%m%f "));
        }
        if self.cwd {
            p.push_str(&format!("%F{{{c}}}%~%f "));
        }
        if self.git {
            p.push_str("%F{yellow}$(_pd_git)%f");
        }
        if self.time {
            p.push_str(&format!("%F{{{c}}}%*%f "));
        }
        p.push_str(&format!("%F{{{c}}}{sym}%f "));
        [
            "# Generated by Prompt Designer \u{2014} edit in the app, not here.",
            "setopt prompt_subst 2>/dev/null",
            "_pd_git() { local b; b=$(git rev-parse --abbrev-ref HEAD 2>/dev/null) || return; print -n \"($b) \"; }",
            &format!("PROMPT='{p}'"),
            "",
        ]
        .join("\n")
    }
}

/// The bash SGR code for a colour name.
fn ansi(color: &str) -> &'static str {
    match color {
        "black" => "30",
        "red" => "31",
        "green" => "32",
        "yellow" => "33",
        "blue" => "34",
        "magenta" => "35",
        "white" => "37",
        _ => "36", // cyan
    }
}

fn on_off(b: bool) -> &'static str {
    if b {
        "on"
    } else {
        "off"
    }
}

fn panel(d: &Design) -> Value {
    // The preview: one coloured, monospaced segment per part, laid out in a row
    // so it reads as the prompt it will become rather than as a quoted string.
    let preview: Vec<Value> = d
        .segments()
        .into_iter()
        .map(|(text, color)| json!({ "type": "text", "text": text, "color": color, "mono": true }))
        .collect();

    let mut blocks = vec![
        json!({ "type": "section", "title": "Preview" }),
        json!({ "type": "row", "children": preview }),
        json!({ "type": "divider" }),
        json!({ "type": "section", "title": "Presets" }),
        json!({ "type": "button", "id": "preset:minimal", "label": "Minimal", "variant": "subtle" }),
        json!({ "type": "button", "id": "preset:classic", "label": "Classic", "variant": "subtle" }),
        json!({ "type": "button", "id": "preset:arrow", "label": "Arrow", "variant": "subtle" }),
        json!({ "type": "section", "title": "Segments" }),
        json!({ "type": "button", "id": "toggle:userhost", "label": format!("user@host: {}", on_off(d.userhost)) }),
        json!({ "type": "button", "id": "toggle:cwd", "label": format!("directory: {}", on_off(d.cwd)) }),
        json!({ "type": "button", "id": "toggle:git", "label": format!("git branch: {}", on_off(d.git)) }),
        json!({ "type": "button", "id": "toggle:time", "label": format!("time: {}", on_off(d.time)) }),
        json!({ "type": "section", "title": "Symbol" }),
    ];

    blocks.push(json!({
        "type": "row",
        "children": SYMBOLS.iter().map(|s| json!({
            "type": "button",
            "id": format!("symbol:{s}"),
            "label": s,
            "variant": if *s == d.symbol { "filled" } else { "subtle" }
        })).collect::<Vec<_>>()
    }));

    blocks.push(json!({ "type": "section", "title": "Colour" }));
    for c in COLORS {
        blocks.push(json!({
            "type": "button",
            "id": format!("color:{c}"),
            "label": c,
            "variant": if *c == d.color { "filled" } else { "subtle" }
        }));
    }

    blocks.push(json!({ "type": "divider" }));
    blocks.push(json!({ "type": "section", "title": "Apply" }));
    blocks.push(json!({
        "type": "text",
        "text": "Applying writes the snippet and sources it in the focused pane, so you can see exactly what changed.",
        "dimmed": true
    }));
    blocks.push(json!({ "type": "button", "id": "apply:zsh", "label": "Apply to zsh", "variant": "filled" }));
    blocks.push(json!({ "type": "button", "id": "apply:bash", "label": "Apply to bash", "variant": "filled" }));
    blocks.push(json!({ "type": "button", "id": "remove", "label": "Remove from shell", "variant": "subtle" }));

    json!({ "title": "Prompt Designer", "blocks": blocks })
}

/// The command that installs the design: write the snippet, fence a source line
/// into the rc file (replacing an existing block), and load it now.
fn apply_command(d: &Design, shell: &str) -> String {
    let rc = if shell == "bash" {
        "$HOME/.bashrc"
    } else {
        "$HOME/.zshrc"
    };
    let snippet = d.snippet(shell);
    format!(
        "mkdir -p \"$(dirname {PROMPT_FILE})\" && \
cat > {PROMPT_FILE} <<'PD_EOF'\n{snippet}PD_EOF\n\
grep -qF '{BEGIN}' {rc} 2>/dev/null || \
printf '\\n{BEGIN}\\n[ -f \"{PROMPT_FILE}\" ] && source \"{PROMPT_FILE}\"\\n{END}\\n' >> {rc}; \
source {PROMPT_FILE}"
    )
}

/// The command that takes the block back out of the rc file.
fn remove_command(shell: &str) -> String {
    let rc = if shell == "bash" {
        "$HOME/.bashrc"
    } else {
        "$HOME/.zshrc"
    };
    format!("sed -i.pd-bak '/{BEGIN}/,/{END}/d' {rc} && echo 'Removed. Open a new shell to see it.'")
}

impl Guest for Designer {
    fn init() {}

    fn call_tool(name: String, params_json: String) -> Result<String, String> {
        if name != "snippet" {
            return Err(format!("unknown tool: {name}"));
        }
        let params: Value = serde_json::from_str(&params_json).unwrap_or(Value::Null);
        let shell = params
            .get("shell")
            .and_then(Value::as_str)
            .filter(|s| *s == "bash" || *s == "zsh")
            .unwrap_or("zsh");
        let d = Design::load();
        Ok(json!({ "shell": shell, "snippet": d.snippet(shell) }).to_string())
    }

    fn render(_request_json: String) -> String {
        panel(&Design::load()).to_string()
    }

    fn on_ui_event(event_json: String) {
        let id = serde_json::from_str::<Value>(&event_json)
            .ok()
            .and_then(|v| v.get("id").and_then(Value::as_str).map(str::to_string))
            .unwrap_or_default();
        let mut d = Design::load();

        if let Some(key) = id.strip_prefix("toggle:") {
            match key {
                "userhost" => d.userhost = !d.userhost,
                "cwd" => d.cwd = !d.cwd,
                "git" => d.git = !d.git,
                "time" => d.time = !d.time,
                _ => return,
            }
            d.save();
        } else if let Some(s) = id.strip_prefix("symbol:") {
            if SYMBOLS.contains(&s) {
                d.symbol = s.to_string();
                d.save();
            }
        } else if let Some(c) = id.strip_prefix("color:") {
            if COLORS.contains(&c) {
                d.color = c.to_string();
                d.save();
            }
        } else if let Some(p) = id.strip_prefix("preset:") {
            match p {
                "minimal" => {
                    d = Design { userhost: false, cwd: true, git: false, time: false, symbol: SYMBOLS[0].into(), ..d }
                }
                "classic" => {
                    d = Design { userhost: true, cwd: true, git: true, time: false, symbol: "$".into(), ..d }
                }
                "arrow" => {
                    d = Design { userhost: false, cwd: true, git: true, time: false, symbol: SYMBOLS[1].into(), ..d }
                }
                _ => return,
            }
            d.save();
        } else if let Some(shell) = id.strip_prefix("apply:") {
            let _ = run_command(&apply_command(&d, shell), CommandTarget::Pane);
        } else if id == "remove" {
            let _ = run_command(&remove_command("zsh"), CommandTarget::Pane);
        }
    }
}

export!(Designer);
