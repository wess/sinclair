//! Keybind parsing: `keybind = trigger=action` where the trigger is
//! modifiers and a key joined by `+`.

use crate::action::{Action, SelectAdjust, SplitDirection, SplitFocus};
use crate::parse::Diagnostic;

/// Modifier keys in a trigger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Mods {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub cmd: bool,
}

/// One resolved keybinding.
#[derive(Debug, Clone, PartialEq)]
pub struct Keybind {
    pub mods: Mods,
    /// Normalized key: a single character (lowercase) or a named key
    /// such as `enter` or `page_up`. This is the leader of the sequence.
    pub key: String,
    /// Extra triggers for a multi-key chord (e.g. `ctrl+a>n`). Empty for a
    /// plain single-stroke binding; the leader is [`Self::mods`]/[`Self::key`].
    pub tail: Vec<(Mods, String)>,
    pub action: Action,
}

impl Keybind {
    /// The trigger as a config string, e.g. `cmd+shift+t` or `ctrl+a>n`.
    pub fn trigger(&self) -> String {
        let mut s = format_trigger(self.mods, &self.key);
        for (m, k) in &self.tail {
            s.push('>');
            s.push_str(&format_trigger(*m, k));
        }
        s
    }

    /// All triggers in order, leader first. One entry for a plain binding.
    pub fn sequence(&self) -> Vec<(Mods, String)> {
        let mut v = vec![(self.mods, self.key.clone())];
        v.extend(self.tail.iter().cloned());
        v
    }

    /// Whether this is a multi-key chord rather than a single stroke.
    pub fn is_chord(&self) -> bool {
        !self.tail.is_empty()
    }

    /// True when this binding's trigger sequence matches `other`'s.
    fn same_trigger(&self, other: &Keybind) -> bool {
        self.mods == other.mods && self.key == other.key && self.tail == other.tail
    }

    /// The full `trigger=action` config value for this binding.
    pub fn config_line(&self) -> String {
        format!("{}={}", self.trigger(), self.action.to_config())
    }
}

/// Format a trigger from modifiers and a normalized key, producing a string
/// that [`parse_trigger`] reads back. Punctuation keys use their named form
/// so the result never collides with the `+`/`=` trigger/action separators.
pub fn format_trigger(mods: Mods, key: &str) -> String {
    let mut s = String::new();
    if mods.cmd {
        push_part(&mut s, "cmd");
    }
    if mods.ctrl {
        push_part(&mut s, "ctrl");
    }
    if mods.alt {
        push_part(&mut s, "alt");
    }
    if mods.shift {
        push_part(&mut s, "shift");
    }
    push_part(&mut s, key_to_name(key));
    s
}

fn push_part(s: &mut String, part: &str) {
    if !s.is_empty() {
        s.push('+');
    }
    s.push_str(part);
}

/// The config name for a normalized key. Named keys and alphanumerics pass
/// through; punctuation maps back to the spelled-out name.
fn key_to_name(key: &str) -> &str {
    match key {
        "+" => "plus",
        "-" => "minus",
        "=" => "equal",
        "," => "comma",
        "." => "period",
        "/" => "slash",
        "\\" => "backslash",
        ";" => "semicolon",
        "'" => "apostrophe",
        "`" => "grave_accent",
        "[" => "bracket_left",
        "]" => "bracket_right",
        other => other,
    }
}

/// Given the desired full keybind set, produce the minimal `keybind` config
/// values that transform [`default_keybinds`] into it: an override line for
/// each binding that differs from (or is absent among) the defaults, and an
/// `=unbind` line for each default the set drops.
pub fn diff_from_defaults(desired: &[Keybind]) -> Vec<String> {
    let defaults = default_keybinds();
    let mut out = Vec::new();
    for kb in desired {
        let default_action = defaults
            .iter()
            .find(|d| d.same_trigger(kb))
            .map(|d| &d.action);
        if default_action != Some(&kb.action) {
            out.push(kb.config_line());
        }
    }
    for d in &defaults {
        let kept = desired.iter().any(|kb| kb.same_trigger(d));
        if !kept {
            out.push(format!("{}=unbind", d.trigger()));
        }
    }
    out
}

/// Parse one keybind value, e.g. `ctrl+shift+c=copy_to_clipboard`, or a
/// multi-key chord like `ctrl+a>n=new_tab`. Triggers are separated by `>`.
pub fn parse_keybind(s: &str) -> Result<Keybind, String> {
    let (trigger, action) = s
        .split_once('=')
        .ok_or_else(|| "expected `trigger=action`".to_string())?;
    let mut triggers = Vec::new();
    for part in trigger.split('>') {
        triggers.push(parse_trigger(part.trim())?);
    }
    let (mods, key) = triggers.remove(0);
    let action = Action::parse(action.trim())?;
    Ok(Keybind {
        mods,
        key,
        tail: triggers,
        action,
    })
}

/// Parse a trigger like `cmd+shift+page_up` into modifiers plus a key.
pub fn parse_trigger(s: &str) -> Result<(Mods, String), String> {
    if s.is_empty() {
        return Err("empty trigger".to_string());
    }
    let (mods_part, key_part) = if s == "+" {
        ("", "+")
    } else if s.ends_with("++") {
        (&s[..s.len() - 2], "+")
    } else {
        match s.rfind('+') {
            Some(i) if i + 1 < s.len() => (&s[..i], &s[i + 1..]),
            Some(_) => return Err(format!("missing key in trigger `{s}`")),
            None => ("", s),
        }
    };
    let mut mods = Mods::default();
    if !mods_part.is_empty() {
        for part in mods_part.split('+') {
            match part.trim().to_ascii_lowercase().as_str() {
                "ctrl" | "control" => mods.ctrl = true,
                "shift" => mods.shift = true,
                "alt" | "opt" | "option" => mods.alt = true,
                "super" | "cmd" | "command" => mods.cmd = true,
                other => return Err(format!("unknown modifier `{other}`")),
            }
        }
    }
    let key = normalize_key(key_part.trim()).ok_or_else(|| format!("unknown key `{key_part}`"))?;
    Ok((mods, key))
}

/// Normalize a key: named keys pass through lowercase, punctuation names
/// map to their character, and any single non-whitespace char is itself.
fn normalize_key(s: &str) -> Option<String> {
    let k = s.to_ascii_lowercase();
    if NAMED_KEYS.contains(&k.as_str()) {
        return Some(k);
    }
    let mapped = match k.as_str() {
        "plus" => "+",
        "minus" => "-",
        "equal" => "=",
        "comma" => ",",
        "period" => ".",
        "slash" => "/",
        "backslash" => "\\",
        "semicolon" => ";",
        "apostrophe" => "'",
        "grave_accent" => "`",
        "bracket_left" => "[",
        "bracket_right" => "]",
        _ => "",
    };
    if !mapped.is_empty() {
        return Some(mapped.to_string());
    }
    let mut chars = k.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if !c.is_whitespace() => Some(c.to_string()),
        _ => None,
    }
}

/// Named keys accepted verbatim.
const NAMED_KEYS: &[&str] = &[
    "enter",
    "tab",
    "escape",
    "space",
    "backspace",
    "delete",
    "insert",
    "up",
    "down",
    "left",
    "right",
    "home",
    "end",
    "page_up",
    "page_down",
    "f1",
    "f2",
    "f3",
    "f4",
    "f5",
    "f6",
    "f7",
    "f8",
    "f9",
    "f10",
    "f11",
    "f12",
];

/// The built-in bindings, mirroring the app's hardcoded set.
pub fn default_keybinds() -> Vec<Keybind> {
    let cmd = Mods {
        cmd: true,
        ..Mods::default()
    };
    let cmd_shift = Mods {
        cmd: true,
        shift: true,
        ..Mods::default()
    };
    let cmd_alt = Mods {
        cmd: true,
        alt: true,
        ..Mods::default()
    };
    let cmd_alt_shift = Mods {
        cmd: true,
        alt: true,
        shift: true,
        ..Mods::default()
    };
    let kb = |mods: Mods, key: &str, action: Action| Keybind {
        mods,
        key: key.to_string(),
        tail: Vec::new(),
        action,
    };
    let mut binds = vec![
        kb(cmd, "n", Action::NewWindow),
        kb(cmd, "t", Action::NewTab),
        kb(cmd_shift, "t", Action::NewContainerTab),
        kb(cmd, "w", Action::CloseSurface),
        kb(cmd_alt, "w", Action::CloseTab),
        kb(cmd_shift, "w", Action::CloseWindow),
        kb(cmd, "s", Action::SaveBuffer),
        kb(cmd_alt_shift, "w", Action::CloseAllWindows),
        kb(cmd, "d", Action::NewSplit(SplitDirection::Right)),
        kb(cmd_shift, "d", Action::NewSplit(SplitDirection::Down)),
        kb(cmd_shift, "[", Action::PreviousTab),
        kb(cmd_shift, "]", Action::NextTab),
        kb(cmd_alt, "up", Action::GotoSplit(SplitFocus::Up)),
        kb(cmd_alt, "down", Action::GotoSplit(SplitFocus::Down)),
        kb(cmd_alt, "left", Action::GotoSplit(SplitFocus::Left)),
        kb(cmd_alt, "right", Action::GotoSplit(SplitFocus::Right)),
        kb(cmd, "c", Action::Copy),
        kb(cmd_shift, "o", Action::CopyCommandOutput),
        kb(cmd_shift, "u", Action::Hints),
        kb(cmd_shift, "space", Action::CopyMode),
        kb(cmd, "v", Action::Paste),
        kb(cmd, "+", Action::IncreaseFontSize(1.0)),
        kb(cmd, "=", Action::IncreaseFontSize(1.0)),
        kb(cmd, "-", Action::DecreaseFontSize(1.0)),
        kb(cmd, "0", Action::ResetFontSize),
        kb(cmd, "k", Action::ClearScreen),
        kb(cmd, "f", Action::ToggleSearch),
        kb(cmd_shift, "f", Action::ToggleSemanticSearch),
        kb(cmd_shift, "e", Action::ExplainOutput),
        kb(cmd_shift, "g", Action::ComposeCommand),
        kb(cmd_shift, "b", Action::ToggleBroadcast),
        kb(cmd_shift, "p", Action::CommandPalette),
        kb(cmd, "p", Action::QuickOpen),
        kb(cmd, "b", Action::Sidebar("left".to_string())),
        kb(cmd_alt, "b", Action::Sidebar("right".to_string())),
        kb(cmd_shift, "r", Action::ToggleRecording),
        kb(cmd, "up", Action::JumpToPrompt(-1)),
        kb(cmd, "down", Action::JumpToPrompt(1)),
        kb(cmd, ",", Action::ToggleSettings),
        kb(cmd_alt, "t", Action::ToggleQuickTerminal),
        kb(cmd_shift, "a", Action::RelayLaunch),
        kb(cmd_shift, "i", Action::RelayFeed),
        kb(cmd_shift, "l", Action::RelayLog),
        kb(cmd, "q", Action::Quit),
    ];
    for n in 1..=9 {
        binds.push(kb(cmd, &n.to_string(), Action::GotoTab(n as i32)));
    }
    // Shift+navigation extends an active selection (the macOS / Ghostty
    // convention). With no selection the action is a no-op and the key
    // falls through to its standard escape sequence, so TUI apps still see
    // `CSI 1;2D` etc. Cross-platform: shift is never remapped.
    let shift = Mods {
        shift: true,
        ..Mods::default()
    };
    for (key, dir) in [
        ("left", SelectAdjust::Left),
        ("right", SelectAdjust::Right),
        ("up", SelectAdjust::Up),
        ("down", SelectAdjust::Down),
        ("home", SelectAdjust::Home),
        ("end", SelectAdjust::End),
        ("page_up", SelectAdjust::PageUp),
        ("page_down", SelectAdjust::PageDown),
    ] {
        binds.push(kb(shift, key, Action::AdjustSelection(dir)));
    }
    // Cmd+Shift+Left/Right extends the selection from the cursor to the start
    // or end of the line (macOS), beginning one at the cursor when none
    // exists. Cmd is a GUI-only modifier (never sent to the pty), so there's
    // nothing to fall through to.
    binds.push(kb(
        cmd_shift,
        "left",
        Action::AdjustSelection(SelectAdjust::LineStart),
    ));
    binds.push(kb(
        cmd_shift,
        "right",
        Action::AdjustSelection(SelectAdjust::LineEnd),
    ));
    // Shift+Alt+Left/Right selects by word: alt+arrow jumps a word (the macOS
    // readline default below), so shift+alt+arrow is the natural "select that
    // word" — the macOS option+shift convention. Without this the combo falls
    // through to a modified arrow escape the shell echoes.
    let alt_shift = Mods {
        alt: true,
        shift: true,
        ..Mods::default()
    };
    binds.push(kb(
        alt_shift,
        "left",
        Action::AdjustSelection(SelectAdjust::WordLeft),
    ));
    binds.push(kb(
        alt_shift,
        "right",
        Action::AdjustSelection(SelectAdjust::WordRight),
    ));
    #[cfg(target_os = "macos")]
    {
        let alt = Mods {
            alt: true,
            ..Mods::default()
        };
        binds.push(kb(cmd, "left", Action::SendText(vec![0x01])));
        binds.push(kb(cmd, "right", Action::SendText(vec![0x05])));
        binds.push(kb(cmd, "backspace", Action::SendText(vec![0x15])));
        binds.push(kb(alt, "left", Action::SendText(vec![0x1b, b'b'])));
        binds.push(kb(alt, "right", Action::SendText(vec![0x1b, b'f'])));
        binds.push(kb(alt, "backspace", Action::SendText(vec![0x1b, 0x7f])));
        binds.push(kb(cmd, "a", Action::SelectAll));
    }
    binds
}

/// Resolve raw `keybind` config values against the defaults: a user
/// binding replaces any default with the same trigger, and `unbind`
/// removes it. Invalid entries become diagnostics (line 0) and are
/// skipped.
pub fn resolve(raw: &[String]) -> (Vec<Keybind>, Vec<Diagnostic>) {
    let mut binds = default_keybinds();
    let mut diags = Vec::new();
    for entry in raw {
        match parse_keybind(entry) {
            Ok(kb) => {
                binds.retain(|b| !b.same_trigger(&kb));
                if kb.action != Action::Unbound {
                    binds.push(kb);
                }
            }
            Err(message) => diags.push(Diagnostic {
                line: 0,
                key: "keybind".to_string(),
                message: format!("`{entry}`: {message}"),
            }),
        }
    }
    (binds, diags)
}

#[cfg(test)]
#[path = "../tests/keybind.rs"]
mod tests;
