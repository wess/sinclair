//! Filesystem-path detection in terminal text, so a path printed by a build,
//! a test runner, or `ls` can be clicked the way a URL can.
//!
//! Deliberately a *candidate* scanner, not a decision. Terminal text is
//! arbitrary, and nothing here can tell `src/main.rs` (a real file) from
//! `and/or` (an English phrase) — so the host resolves what this returns
//! against the pane's working directory and checks the filesystem before
//! offering the user anything. That check is what makes a loose pattern safe:
//! a false positive here becomes silence there, not a wrong menu entry.

/// A path found in a row of text: the char range it occupies and, when the
/// text carried a `:line[:col]` suffix, the position it pointed at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    /// Char index of the first character of the path.
    pub start: usize,
    /// Char index one past the last character *of the whole match*, suffix
    /// included — this is what a caller highlights, so the line number is part
    /// of it even though it is not part of the path.
    pub end: usize,
    /// The path itself, with any `:line:col` suffix removed.
    pub text: String,
    /// The 1-based line from a `path:line` suffix.
    pub line: Option<u32>,
    /// The 1-based column from a `path:line:col` suffix.
    pub column: Option<u32>,
}

/// Every path candidate in `chars`, left to right and non-overlapping.
pub fn find(chars: &[char]) -> Vec<Span> {
    let mut spans = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        // Only start a candidate at a boundary, so the tail of a longer run of
        // text (`https://x/y`, already a URL) cannot be re-found as a path.
        if i > 0 && is_path_char(chars[i - 1]) {
            i += 1;
            continue;
        }
        match match_at(&chars[i..]) {
            Some(mut span) => {
                span.start += i;
                span.end += i;
                i = span.end;
                spans.push(span);
            }
            None => i += 1,
        }
    }
    spans
}

/// If a path candidate starts at the front of `s`, describe it.
fn match_at(s: &[char]) -> Option<Span> {
    let mut len = 0;
    while len < s.len() && is_path_char(s[len]) {
        len += 1;
    }
    if len == 0 {
        return None;
    }
    // Prose ends in punctuation far more often than paths do, and a path that
    // genuinely ends in one is rare enough to be worth losing.
    while len > 0 && is_trailing(s[len - 1]) {
        len -= 1;
    }
    let whole: String = s[..len].iter().collect();
    let (text, line, column) = split_position(&whole);
    if !looks_like_path(&text) {
        return None;
    }
    Some(Span {
        start: 0,
        end: len,
        text,
        line,
        column,
    })
}

/// Split a trailing `:line` or `:line:col` off a candidate — the shape every
/// compiler, linter, and stack trace prints. Without this the path never
/// resolves, because `main.rs:42` is not a file.
fn split_position(s: &str) -> (String, Option<u32>, Option<u32>) {
    let take_num = |rest: &str| -> Option<(String, u32)> {
        let (head, tail) = rest.rsplit_once(':')?;
        // A Windows drive letter (`C:\src`) is not a line number, and neither
        // is an empty tail.
        if tail.is_empty() || !tail.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        let n: u32 = tail.parse().ok()?;
        Some((head.to_string(), n))
    };
    match take_num(s) {
        None => (s.to_string(), None, None),
        Some((head, first)) => match take_num(&head) {
            // Two numbers: the inner one is the line, the outer the column.
            Some((path, line)) if looks_like_path(&path) => (path, Some(line), Some(first)),
            _ => (head, Some(first), None),
        },
    }
}

/// Whether a candidate is path-shaped enough to be worth a filesystem check.
///
/// A separator or a `~` is the bar. A bare word like `README` is a path only in
/// the sense that every word is, and letting those through would mean touching
/// the disk over ordinary prose.
fn looks_like_path(s: &str) -> bool {
    if s.len() < 2 || s.len() > 4096 {
        return false;
    }
    // A scheme means this is a URL, which is already handled as one.
    if s.contains("://") {
        return false;
    }
    s.contains('/') || s.starts_with('~')
}

/// Characters allowed inside a path candidate. Excludes whitespace and the
/// shell metacharacters that reliably end one (quotes, brackets, pipes,
/// redirects), which is what stops a candidate running into the next word.
fn is_path_char(c: char) -> bool {
    match c {
        '/' | '.' | '-' | '_' | '~' | '+' | '@' | '%' | ':' | '#' | '$' | '=' | ',' => true,
        c if c.is_alphanumeric() => true,
        _ => false,
    }
}

/// Punctuation that ends a sentence rather than a path.
fn is_trailing(c: char) -> bool {
    matches!(
        c,
        '.' | ',' | ':' | ';' | '-' | '=' | '@' | '#' | '$' | '%' | '+'
    )
}

#[cfg(test)]
#[path = "../tests/path.rs"]
mod tests;
