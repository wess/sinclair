//! Substring search over the grid + scrollback. Matching is per visual
//! row (matches do not span line breaks), which covers the common case.

/// One search hit: a global row index (same space as
/// [`crate::Terminal::prompt_lines`]) and an inclusive column range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Match {
    pub line: usize,
    pub start_col: usize,
    pub end_col: usize,
}

/// Find every occurrence of `needle` in one row, given the row's visible
/// chars and their columns (wide spacers already removed). `fold` lowercases
/// for case-insensitive search. Non-overlapping, left to right.
pub fn in_row(
    needle: &[char],
    chars: &[char],
    col_of: &[usize],
    line: usize,
    fold: bool,
    wide_tail: impl Fn(usize) -> bool,
) -> Vec<Match> {
    let mut hits = Vec::new();
    if needle.is_empty() || needle.len() > chars.len() {
        return hits;
    }
    let eq = |a: char, b: char| {
        if fold {
            a.to_ascii_lowercase() == b.to_ascii_lowercase()
        } else {
            a == b
        }
    };
    let mut i = 0;
    while i + needle.len() <= chars.len() {
        if chars[i..i + needle.len()]
            .iter()
            .zip(needle)
            .all(|(&a, &b)| eq(a, b))
        {
            let last = col_of[i + needle.len() - 1];
            hits.push(Match {
                line,
                start_col: col_of[i],
                end_col: last + usize::from(wide_tail(last)),
            });
            i += needle.len();
        } else {
            i += 1;
        }
    }
    hits
}

#[cfg(test)]
#[path = "../tests/search.rs"]
mod tests;
