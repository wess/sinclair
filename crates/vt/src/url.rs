//! Lightweight URL detection in terminal text, so a URL can be opened with
//! a click even when the program did not emit an OSC 8 hyperlink.

/// Recognized URL schemes (checked case-insensitively).
const SCHEMES: &[&str] = &["https://", "http://", "ftp://", "file://", "mailto:"];

/// Find URLs in `chars` as char-index ranges `[start, end)`.
pub fn find(chars: &[char]) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if let Some(len) = match_at(&chars[i..]) {
            spans.push((i, i + len));
            i += len;
        } else {
            i += 1;
        }
    }
    spans
}

/// If a URL starts at the front of `s`, return its length in chars.
fn match_at(s: &[char]) -> Option<usize> {
    let scheme_len = SCHEMES.iter().find_map(|scheme| {
        let sl: Vec<char> = scheme.chars().collect();
        (s.len() > sl.len()
            && s[..sl.len()]
                .iter()
                .zip(&sl)
                .all(|(a, b)| a.to_ascii_lowercase() == *b))
        .then_some(sl.len())
    })?;

    let mut len = scheme_len;
    while len < s.len() && is_url_char(s[len]) {
        len += 1;
    }
    // Need at least one char of authority/path after the scheme.
    if len == scheme_len {
        return None;
    }
    // Trim trailing sentence punctuation and an unbalanced closing paren.
    while len > scheme_len && is_trailing(s[len - 1]) {
        if s[len - 1] == ')' && balanced_paren(&s[scheme_len..len]) {
            break;
        }
        len -= 1;
    }
    (len > scheme_len).then_some(len)
}

/// Characters allowed inside a URL body (RFC 3986-ish, minus delimiters
/// that commonly bound URLs in prose).
fn is_url_char(c: char) -> bool {
    !c.is_whitespace()
        && !c.is_control()
        && !matches!(c, '"' | '<' | '>' | '`' | '{' | '}' | '|' | '\\' | '^')
}

/// Punctuation often trailing a URL in prose, trimmed from the match.
fn is_trailing(c: char) -> bool {
    matches!(
        c,
        '.' | ',' | ';' | ':' | '!' | '?' | ')' | ']' | '\'' | '"'
    )
}

/// Whether parentheses in `s` are balanced (so a trailing `)` belongs).
fn balanced_paren(s: &[char]) -> bool {
    let opens = s.iter().filter(|&&c| c == '(').count();
    let closes = s.iter().filter(|&&c| c == ')').count();
    opens >= closes
}

#[cfg(test)]
#[path = "../tests/url.rs"]
mod tests;
