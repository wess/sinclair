//! Small value parsing helpers for config values.

/// Parse a boolean value. Accepts true/false, 1/0, yes/no (case-insensitive).
pub fn parse_bool(s: &str) -> Option<bool> {
    match s.to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" => Some(true),
        "false" | "0" | "no" => Some(false),
        _ => None,
    }
}

/// Parse an f32.
pub fn parse_f32(s: &str) -> Option<f32> {
    s.parse().ok()
}

/// Parse a u32.
pub fn parse_u32(s: &str) -> Option<u32> {
    s.parse().ok()
}

/// Parse a usize.
pub fn parse_usize(s: &str) -> Option<usize> {
    s.parse().ok()
}

/// Parse a finite f32 and clamp it into `lo..=hi`.
pub fn parse_f32_range(s: &str, lo: f32, hi: f32) -> Option<f32> {
    let v: f32 = s.parse().ok()?;
    if !v.is_finite() {
        return None;
    }
    Some(v.clamp(lo, hi))
}

/// Parse a cell-size adjustment: an integer pixel count with an optional
/// `px` suffix, e.g. `2`, `-1`, `+3px`.
pub fn parse_adjust(s: &str) -> Option<i32> {
    let t = s.strip_suffix("px").unwrap_or(s).trim();
    t.parse().ok()
}

/// Parse and normalize a hex color: optional `#`, then 6 hex digits.
/// Returns the normalized `#rrggbb` (lowercase) form.
pub fn parse_color(s: &str) -> Option<String> {
    let hex = s.strip_prefix('#').unwrap_or(s);
    if hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(format!("#{}", hex.to_ascii_lowercase()))
    } else {
        None
    }
}

/// Validate a font feature: optional `+`/`-` sign, then an alphanumeric
/// tag like `liga` or `ss01`. Returned verbatim.
pub fn parse_fontfeature(s: &str) -> Option<String> {
    let tag = s.strip_prefix(['+', '-']).unwrap_or(s);
    if !tag.is_empty() && tag.chars().all(|c| c.is_ascii_alphanumeric()) {
        Some(s.to_string())
    } else {
        None
    }
}

/// Strip a single pair of surrounding double quotes, if present.
pub fn unquote(s: &str) -> &str {
    let s = s.trim();
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

/// Parse the `N=#rrggbb` palette form into (index, color).
/// The color part is kept as a string; it must start with `#` and have
/// 6 hex digits after it.
pub fn parse_palette(s: &str) -> Option<(u8, String)> {
    let (idx, color) = s.split_once('=')?;
    let idx: u8 = idx.trim().parse().ok()?;
    let color = unquote(color.trim());
    let hex = color.strip_prefix('#')?;
    if hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        Some((idx, color.to_string()))
    } else {
        None
    }
}

#[cfg(test)]
#[path = "../tests/value.rs"]
mod tests;
