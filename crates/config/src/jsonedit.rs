//! Surgical edits to the settings.json text: set or remove one top-level
//! key while preserving every comment, blank line, and hand-formatted value
//! the user wrote. The GUI writes through here; the file stays the source
//! of truth.

/// A top-level member's location in the source text.
struct Span {
    key: String,
    /// Byte offset of the opening quote of the key.
    start: usize,
    /// Byte offset just past the value.
    end: usize,
}

/// Return `text` with `key` set to `raw` (an already-serialized JSON value).
/// The existing value is replaced in place; a missing key is appended before
/// the closing brace. `None` when the text is not an editable object (the
/// caller should refuse the write rather than clobber the file).
pub fn upsert(text: &str, key: &str, raw: &str) -> Option<String> {
    let (spans, close) = scan(text)?;
    if let Some(span) = spans.iter().find(|s| s.key == key) {
        // Replace just the value: everything from after the colon's trivia.
        let colon = text[span.start..].find(':')? + span.start;
        let vstart = colon + 1 + count_trivia(&text[colon + 1..]);
        let mut out = String::with_capacity(text.len() + raw.len());
        out.push_str(&text[..vstart]);
        out.push_str(raw);
        out.push_str(&text[span.end..]);
        return Some(out);
    }
    insert(text, key, raw, &spans, close)
}

/// Return `text` with `key` removed (a no-op when absent). `None` when the
/// text is not an editable object.
pub fn remove(text: &str, key: &str) -> Option<String> {
    let (spans, _) = scan(text)?;
    let Some(span) = spans.iter().find(|s| s.key == key) else {
        return Some(text.to_string());
    };
    // Swallow the line's leading indentation…
    let mut start = span.start;
    while start > 0 && matches!(text.as_bytes()[start - 1], b' ' | b'\t') {
        start -= 1;
    }
    // …and the separator: a trailing comma (plus the rest of its line), or
    // when this is the last member, the comma preceding it.
    let mut end = span.end;
    let bytes = text.as_bytes();
    let mut i = end;
    while i < bytes.len() && matches!(bytes[i], b' ' | b'\t') {
        i += 1;
    }
    if bytes.get(i) == Some(&b',') {
        end = i + 1;
        let mut j = end;
        while j < bytes.len() && matches!(bytes[j], b' ' | b'\t') {
            j += 1;
        }
        if bytes.get(j) == Some(&b'\n') {
            end = j + 1;
        }
    } else {
        while start > 0 && matches!(bytes[start - 1], b' ' | b'\t' | b'\n' | b'\r') {
            start -= 1;
        }
        if start > 0 && bytes[start - 1] == b',' {
            start -= 1;
        }
        if bytes.get(end) == Some(&b'\n') {
            end += 1;
        }
    }
    let mut out = String::with_capacity(text.len());
    out.push_str(&text[..start]);
    out.push_str(&text[end..]);
    Some(out)
}

/// Append a new `"key": raw` member before the closing brace.
fn insert(text: &str, key: &str, raw: &str, spans: &[Span], close: usize) -> Option<String> {
    let mut out = String::with_capacity(text.len() + key.len() + raw.len() + 8);
    if let Some(last) = spans.last() {
        // After the last member: keep its trailing comma if present, else add one.
        out.push_str(&text[..last.end]);
        let rest = &text[last.end..close];
        if !rest.trim_start().starts_with(',') {
            out.push(',');
        }
        out.push_str(rest);
        if !out.ends_with('\n') {
            out.push('\n');
        }
    } else {
        out.push_str(&text[..close]);
        if !out.ends_with('\n') {
            out.push('\n');
        }
    }
    out.push_str(&format!("  {}: {}\n", crate::json::quote(key), raw));
    out.push_str(&text[close..]);
    Some(out)
}

/// Locate every top-level member and the closing brace of the root object.
/// `None` for anything that isn't a single (possibly empty) object.
fn scan(text: &str) -> Option<(Vec<Span>, usize)> {
    let bytes = text.as_bytes();
    let mut pos = count_trivia(text);
    if bytes.get(pos) != Some(&b'{') {
        return None;
    }
    pos += 1;
    let mut spans = Vec::new();
    loop {
        pos += count_trivia(&text[pos..]);
        match bytes.get(pos) {
            Some(b'}') => return Some((spans, pos)),
            Some(b',') => {
                pos += 1;
                continue;
            }
            Some(b'"') => {}
            _ => return None,
        }
        let start = pos;
        let key = read_string(text, &mut pos)?;
        pos += count_trivia(&text[pos..]);
        if bytes.get(pos) != Some(&b':') {
            return None;
        }
        pos += 1;
        pos += count_trivia(&text[pos..]);
        skip_value(text, &mut pos)?;
        spans.push(Span {
            key,
            start,
            end: pos,
        });
    }
}

/// Bytes of whitespace and comments at the start of `text`.
fn count_trivia(text: &str) -> usize {
    let bytes = text.as_bytes();
    let mut pos = 0;
    loop {
        while bytes.get(pos).is_some_and(|b| b.is_ascii_whitespace()) {
            pos += 1;
        }
        match (bytes.get(pos), bytes.get(pos + 1)) {
            (Some(b'/'), Some(b'/')) => {
                while bytes.get(pos).is_some_and(|&b| b != b'\n') {
                    pos += 1;
                }
            }
            (Some(b'/'), Some(b'*')) => {
                pos += 2;
                while pos < bytes.len() {
                    if bytes[pos] == b'*' && bytes.get(pos + 1) == Some(&b'/') {
                        pos += 2;
                        break;
                    }
                    pos += 1;
                }
            }
            _ => return pos,
        }
    }
}

/// Skip a string literal, leaving `pos` past the closing quote. Returns the
/// unescaped contents.
fn read_string(text: &str, pos: &mut usize) -> Option<String> {
    let bytes = text.as_bytes();
    let start = *pos;
    *pos += 1;
    while let Some(&b) = bytes.get(*pos) {
        match b {
            b'"' => {
                *pos += 1;
                return match crate::json::parse(&text[start..*pos]) {
                    Ok(crate::json::Value::Str(s)) => Some(s),
                    _ => None,
                };
            }
            b'\\' => *pos += 2,
            b'\n' => return None,
            _ => *pos += 1,
        }
    }
    None
}

/// Skip one JSON value (scalar, array, or object), comments included,
/// leaving `pos` just past it.
fn skip_value(text: &str, pos: &mut usize) -> Option<()> {
    let bytes = text.as_bytes();
    match bytes.get(*pos)? {
        b'"' => {
            read_string(text, pos)?;
        }
        b'{' | b'[' => {
            let mut depth = 0usize;
            loop {
                match bytes.get(*pos)? {
                    b'{' | b'[' => {
                        depth += 1;
                        *pos += 1;
                    }
                    b'}' | b']' => {
                        depth -= 1;
                        *pos += 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    b'"' => {
                        read_string(text, pos)?;
                    }
                    b'/' => {
                        let n = count_trivia(&text[*pos..]);
                        *pos += n.max(1);
                    }
                    _ => *pos += 1,
                }
            }
        }
        _ => {
            // A bare word or number: run to the next delimiter.
            while bytes.get(*pos).is_some_and(|&b| {
                !matches!(b, b',' | b'}' | b']' | b'/') && !b.is_ascii_whitespace()
            }) {
                *pos += 1;
            }
        }
    }
    Some(())
}

#[cfg(test)]
#[path = "../tests/jsonedit.rs"]
mod tests;
