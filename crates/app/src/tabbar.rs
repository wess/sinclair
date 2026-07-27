//! Small helpers for the tab strip and the chrome around it. (The
//! window-level tab strip is gone: tabs now live per-pane inside the
//! `guise::PaneGroup`.)

use theme::Rgb;

/// Linear mix of two colors: `t` 0 is `a`, 1 is `b`. Clamped.
pub fn blend(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    let mix = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Rgb::new(mix(a.r, b.r), mix(a.g, b.g), mix(a.b, b.b))
}

/// Drop the `user@host:` a shell writes into its terminal title, leaving just
/// the part people actually read (usually the path). Backs
/// `tab-title-show-host`.
///
/// Deliberately narrow, because a title is arbitrary text: it only fires when
/// everything before the first `:` looks like `user@host` — one `@`, both
/// sides non-empty, and no slash or whitespace anywhere. That leaves
/// `https://example.com:8080`, `nvim: main.rs`, and anything with a path in
/// front of the colon alone.
pub fn strip_host(title: &str) -> &str {
    let Some(colon) = title.find(':') else {
        return title;
    };
    let (prefix, rest) = title.split_at(colon);
    let rest = &rest[1..];
    if rest.is_empty() {
        return title;
    }
    let Some((user, host)) = prefix.split_once('@') else {
        return title;
    };
    let looks_like_login = !user.is_empty()
        && !host.is_empty()
        && !host.contains('@')
        && !prefix.contains('/')
        && !prefix.chars().any(char::is_whitespace);
    if looks_like_login {
        rest
    } else {
        title
    }
}

#[cfg(test)]
#[path = "../tests/tabbar.rs"]
mod tests;
