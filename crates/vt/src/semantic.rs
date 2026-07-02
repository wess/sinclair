//! Semantic token detection for smart selection: given a row of characters and
//! a click column, return the span of the meaningful token there — a URL,
//! email, filesystem path, or git-style hex hash — so a double-click can select
//! the whole thing instead of a single whitespace word. Pure, no allocation
//! beyond the small validation strings.

/// The `[start, end)` char span of the semantic token at `col`, or `None` when
/// there isn't one (the caller falls back to plain word selection). Tries URL,
/// then email, then path, then hash — most specific first.
pub fn token_at(chars: &[char], col: usize) -> Option<(usize, usize)> {
    if col >= chars.len() {
        return None;
    }

    // URLs reuse the existing detector (handles schemes, brackets, trailing
    // punctuation) and win over the looser scanners below.
    for (s, e) in crate::url::find(chars) {
        if col >= s && col < e {
            return Some((s, e));
        }
    }

    if let Some(span) = expand(chars, col, is_email_char) {
        if is_email(&slice(chars, span)) {
            return Some(span);
        }
    }

    if let Some(span) = expand(chars, col, is_path_char) {
        if is_pathlike(&slice(chars, span)) {
            return Some(span);
        }
    }

    if let Some(span) = expand(chars, col, |c| c.is_ascii_hexdigit()) {
        // A git short hash is >= 7 hex digits; avoid grabbing plain numbers.
        if span.1 - span.0 >= 7 {
            return Some(span);
        }
    }

    None
}

/// Grow left and right from `col` while `pred` holds. `None` if `col` itself
/// fails the predicate.
fn expand(chars: &[char], col: usize, pred: impl Fn(char) -> bool) -> Option<(usize, usize)> {
    if !pred(chars[col]) {
        return None;
    }
    let mut start = col;
    while start > 0 && pred(chars[start - 1]) {
        start -= 1;
    }
    let mut end = col + 1;
    while end < chars.len() && pred(chars[end]) {
        end += 1;
    }
    Some((start, end))
}

fn slice(chars: &[char], (s, e): (usize, usize)) -> String {
    chars[s..e].iter().collect()
}

fn is_email_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '.' | '_' | '%' | '+' | '-' | '@')
}

fn is_email(s: &str) -> bool {
    let mut at = s.split('@');
    match (at.next(), at.next(), at.next()) {
        (Some(local), Some(domain), None) => {
            !local.is_empty()
                && domain.contains('.')
                && !domain.starts_with('.')
                && !domain.ends_with('.')
        }
        _ => false,
    }
}

fn is_path_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '/' | '.' | '_' | '-' | '~')
}

fn is_pathlike(s: &str) -> bool {
    s.len() > 1 && s.contains('/') && s.chars().any(|c| c != '/' && c != '.')
}

#[cfg(test)]
#[path = "../tests/semantic.rs"]
mod tests;
