//! Translate a resolved config keybind trigger into a gpui keystroke
//! string. gpui joins a keystroke's components with `-` (e.g.
//! `cmd-shift-d`) and names a few keys differently than the config crate.

/// gpui keystroke string for a whole binding, joining a chord's triggers with
/// a space (gpui's sequence separator). `None` if any trigger has no spelling.
pub fn keystroke_seq(kb: &config::Keybind) -> Option<String> {
    let mut parts = Vec::with_capacity(1 + kb.tail.len());
    for (mods, key) in kb.sequence() {
        parts.push(keystroke(mods, &key)?);
    }
    Some(parts.join(" "))
}

/// gpui keystroke string for a single config trigger, or `None` when the key
/// has no spelling we can emit. The caller still validates the result with
/// [`gpui::Keystroke::parse`] before binding, so an odd key is skipped
/// rather than panicking.
pub fn keystroke(mods: config::Mods, key: &str) -> Option<String> {
    if key.is_empty() {
        return None;
    }
    let key = gpui_key(key);
    let mut s = String::new();
    if mods.ctrl {
        s.push_str("ctrl-");
    }
    if mods.alt {
        s.push_str("alt-");
    }
    if mods.shift {
        s.push_str("shift-");
    }
    if mods.cmd {
        s.push_str("secondary-");
    }
    s.push_str(&key);
    Some(s)
}

/// A human keybind hint for a whole binding. For a chord, each trigger's
/// glyphs are joined with a space (e.g. `⌃A N`). `None` if any trigger is
/// unprintable.
pub fn shortcut_glyphs_seq(kb: &config::Keybind) -> Option<String> {
    let mut parts = Vec::with_capacity(1 + kb.tail.len());
    for (mods, key) in kb.sequence() {
        parts.push(shortcut_glyphs(mods, &key)?);
    }
    Some(parts.join(" "))
}

/// A human keybind hint for menus/palette: macOS modifier glyphs followed by
/// the key. `None` for an unprintable key. Modifier order matches the macOS
/// convention (⌃⌥⇧⌘).
pub fn shortcut_glyphs(mods: config::Mods, key: &str) -> Option<String> {
    if key.is_empty() {
        return None;
    }
    let mut s = String::new();
    if mods.ctrl {
        s.push('\u{2303}');
    }
    if mods.alt {
        s.push('\u{2325}');
    }
    if mods.shift {
        s.push('\u{21e7}');
    }
    if mods.cmd {
        s.push('\u{2318}');
    }
    s.push_str(&key_glyph(key));
    Some(s)
}

/// A display glyph for a key in a shortcut hint.
fn key_glyph(key: &str) -> String {
    match key {
        "left" => "\u{2190}".into(),
        "right" => "\u{2192}".into(),
        "up" => "\u{2191}".into(),
        "down" => "\u{2193}".into(),
        "enter" => "\u{21a9}".into(),
        "backspace" => "\u{232b}".into(),
        "delete" => "\u{2326}".into(),
        "escape" => "esc".into(),
        "space" => "space".into(),
        "page_up" => "\u{21de}".into(),
        "page_down" => "\u{21df}".into(),
        other if other.chars().count() == 1 => other.to_uppercase(),
        other => other.to_string(),
    }
}

/// Map config key names onto gpui's spellings. Most match; only the paged
/// navigation keys differ (`page_up` vs `pageup`).
fn gpui_key(key: &str) -> String {
    match key {
        "page_up" => "pageup".to_string(),
        "page_down" => "pagedown".to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
#[path = "../tests/keys.rs"]
mod tests;
