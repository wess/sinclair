//! Translate a resolved config keybind trigger into a gpui keystroke
//! string. gpui joins a keystroke's components with `-` (e.g.
//! `cmd-shift-d`) and names a few keys differently than the config crate.

/// gpui keystroke string for a config trigger, or `None` when the key has
/// no spelling we can emit. The caller still validates the result with
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
        // `secondary` resolves to Command on macOS and Control elsewhere,
        // so a single `cmd+...` config binding is correct on every platform.
        s.push_str("secondary-");
    }
    s.push_str(&key);
    Some(s)
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
