//! Minimal JSON-with-comments parser for the settings file. Accepts `//` and
//! `/* */` comments plus trailing commas, so the file stays comfortable to
//! hand-edit. No external dependencies, like the rest of this crate.

/// A parsed JSON value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Value>),
    Obj(Vec<Member>),
}

/// One `"key": value` member of an object, with the 1-based line its key
/// starts on (for diagnostics).
#[derive(Debug, Clone, PartialEq)]
pub struct Member {
    pub key: String,
    pub value: Value,
    pub line: usize,
}

/// A syntax error, with the 1-based line it was found on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub line: usize,
    pub message: String,
}

/// Parse a complete JSONC document into a value.
pub fn parse(text: &str) -> Result<Value, Error> {
    let mut p = Cursor {
        text: text.as_bytes(),
        pos: 0,
    };
    p.skip_trivia();
    let value = p.value()?;
    p.skip_trivia();
    if p.pos < p.text.len() {
        return Err(p.err("unexpected trailing content"));
    }
    Ok(value)
}

/// The top-level members of a JSONC settings document. A blank (or
/// comment-only) document is an empty list; anything but an object at the
/// top level is an error.
pub fn root(text: &str) -> Result<Vec<Member>, Error> {
    let mut p = Cursor {
        text: text.as_bytes(),
        pos: 0,
    };
    p.skip_trivia();
    if p.pos >= p.text.len() {
        return Ok(Vec::new());
    }
    match parse(text)? {
        Value::Obj(members) => Ok(members),
        _ => Err(Error {
            line: 1,
            message: "expected a `{ ... }` object at the top level".into(),
        }),
    }
}

/// Quote and escape a string as a JSON string literal.
pub fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Format a number the way the settings file writes it: integers without a
/// trailing `.0`, everything else in shortest form.
pub fn num(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

struct Cursor<'a> {
    text: &'a [u8],
    pos: usize,
}

impl Cursor<'_> {
    fn err(&self, message: &str) -> Error {
        Error {
            line: self.line(),
            message: message.to_string(),
        }
    }

    fn line(&self) -> usize {
        1 + self.text[..self.pos.min(self.text.len())]
            .iter()
            .filter(|&&b| b == b'\n')
            .count()
    }

    fn peek(&self) -> Option<u8> {
        self.text.get(self.pos).copied()
    }

    /// Skip whitespace and `//` / `/* */` comments.
    fn skip_trivia(&mut self) {
        loop {
            while self.peek().is_some_and(|b| b.is_ascii_whitespace()) {
                self.pos += 1;
            }
            match (self.peek(), self.text.get(self.pos + 1)) {
                (Some(b'/'), Some(b'/')) => {
                    while self.peek().is_some_and(|b| b != b'\n') {
                        self.pos += 1;
                    }
                }
                (Some(b'/'), Some(b'*')) => {
                    self.pos += 2;
                    while self.pos < self.text.len() {
                        if self.text[self.pos] == b'*' && self.text.get(self.pos + 1) == Some(&b'/')
                        {
                            self.pos += 2;
                            break;
                        }
                        self.pos += 1;
                    }
                }
                _ => return,
            }
        }
    }

    fn value(&mut self) -> Result<Value, Error> {
        match self.peek() {
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => Ok(Value::Str(self.string()?)),
            Some(b't') | Some(b'f') | Some(b'n') => self.word(),
            Some(b) if b == b'-' || b.is_ascii_digit() => self.number(),
            Some(_) => Err(self.err("expected a value")),
            None => Err(self.err("unexpected end of file")),
        }
    }

    fn object(&mut self) -> Result<Value, Error> {
        self.pos += 1; // {
        let mut members = Vec::new();
        loop {
            self.skip_trivia();
            match self.peek() {
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(Value::Obj(members));
                }
                Some(b'"') => {}
                Some(_) => return Err(self.err("expected `\"key\"` or `}`")),
                None => return Err(self.err("unclosed `{`")),
            }
            let line = self.line();
            let key = self.string()?;
            self.skip_trivia();
            if self.peek() != Some(b':') {
                return Err(self.err("expected `:` after key"));
            }
            self.pos += 1;
            self.skip_trivia();
            let value = self.value()?;
            members.push(Member { key, value, line });
            self.skip_trivia();
            if self.peek() == Some(b',') {
                self.pos += 1; // trailing comma before `}` is fine
            }
        }
    }

    fn array(&mut self) -> Result<Value, Error> {
        self.pos += 1; // [
        let mut items = Vec::new();
        loop {
            self.skip_trivia();
            match self.peek() {
                Some(b']') => {
                    self.pos += 1;
                    return Ok(Value::Arr(items));
                }
                None => return Err(self.err("unclosed `[`")),
                _ => {}
            }
            items.push(self.value()?);
            self.skip_trivia();
            if self.peek() == Some(b',') {
                self.pos += 1;
            }
        }
    }

    fn string(&mut self) -> Result<String, Error> {
        self.pos += 1; // opening quote
        let mut out = String::new();
        loop {
            match self.peek() {
                None | Some(b'\n') => return Err(self.err("unterminated string")),
                Some(b'"') => {
                    self.pos += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    self.pos += 1;
                    match self.peek() {
                        Some(b'"') => out.push('"'),
                        Some(b'\\') => out.push('\\'),
                        Some(b'/') => out.push('/'),
                        Some(b'b') => out.push('\u{8}'),
                        Some(b'f') => out.push('\u{c}'),
                        Some(b'n') => out.push('\n'),
                        Some(b'r') => out.push('\r'),
                        Some(b't') => out.push('\t'),
                        Some(b'u') => {
                            out.push(self.unicode_escape()?);
                            continue; // unicode_escape leaves pos past the digits
                        }
                        _ => return Err(self.err("bad escape sequence")),
                    }
                    self.pos += 1;
                }
                Some(_) => {
                    // Consume one UTF-8 character (the input is a &str, so
                    // boundaries are valid).
                    let start = self.pos;
                    self.pos += 1;
                    while self.pos < self.text.len() && (self.text[self.pos] & 0xc0) == 0x80 {
                        self.pos += 1;
                    }
                    out.push_str(std::str::from_utf8(&self.text[start..self.pos]).unwrap_or(""));
                }
            }
        }
    }

    /// `\uXXXX`, with surrogate pairs combined. Called with pos on the `u`.
    fn unicode_escape(&mut self) -> Result<char, Error> {
        let hi = self.hex4()?;
        if (0xd800..0xdc00).contains(&hi) {
            if self.peek() == Some(b'\\') && self.text.get(self.pos + 1) == Some(&b'u') {
                self.pos += 1;
                let lo = self.hex4()?;
                let c = 0x10000 + ((hi - 0xd800) << 10) + (lo.wrapping_sub(0xdc00));
                return Ok(char::from_u32(c).unwrap_or('\u{fffd}'));
            }
            return Ok('\u{fffd}');
        }
        Ok(char::from_u32(hi).unwrap_or('\u{fffd}'))
    }

    fn hex4(&mut self) -> Result<u32, Error> {
        self.pos += 1; // the `u`
        let mut v = 0u32;
        for _ in 0..4 {
            let d = self
                .peek()
                .and_then(|b| (b as char).to_digit(16))
                .ok_or_else(|| self.err("bad \\u escape"))?;
            v = v * 16 + d;
            self.pos += 1;
        }
        Ok(v)
    }

    fn word(&mut self) -> Result<Value, Error> {
        for (word, value) in [
            ("true", Value::Bool(true)),
            ("false", Value::Bool(false)),
            ("null", Value::Null),
        ] {
            if self.text[self.pos..].starts_with(word.as_bytes()) {
                self.pos += word.len();
                return Ok(value);
            }
        }
        Err(self.err("expected a value"))
    }

    fn number(&mut self) -> Result<Value, Error> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        while self
            .peek()
            .is_some_and(|b| b.is_ascii_digit() || matches!(b, b'.' | b'e' | b'E' | b'+' | b'-'))
        {
            self.pos += 1;
        }
        std::str::from_utf8(&self.text[start..self.pos])
            .ok()
            .and_then(|s| s.parse().ok())
            .map(Value::Num)
            .ok_or_else(|| self.err("bad number"))
    }
}

#[cfg(test)]
#[path = "../tests/json.rs"]
mod tests;
