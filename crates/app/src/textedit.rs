//! Pure single-line text-editing model: a string plus a char-index cursor,
//! with the operations an input field needs. No UI, fully unit-testable;
//! the settings panel drives it from key events and renders from `split`.

/// An editable line of text with a cursor.
#[derive(Debug, Clone, Default)]
pub struct TextEdit {
    chars: Vec<char>,
    /// Cursor position as a char index in `0..=chars.len()`.
    cursor: usize,
}

impl TextEdit {
    /// Start editing `text` with the cursor at the end.
    pub fn new(text: &str) -> Self {
        let chars: Vec<char> = text.chars().collect();
        let cursor = chars.len();
        Self { chars, cursor }
    }

    pub fn text(&self) -> String {
        self.chars.iter().collect()
    }

    /// Insert `s` at the cursor, advancing past it.
    pub fn insert(&mut self, s: &str) {
        for c in s.chars() {
            self.chars.insert(self.cursor, c);
            self.cursor += 1;
        }
    }

    /// Delete the char before the cursor. Returns whether anything changed.
    pub fn backspace(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }
        self.cursor -= 1;
        self.chars.remove(self.cursor);
        true
    }

    /// Delete the char at the cursor. Returns whether anything changed.
    pub fn delete(&mut self) -> bool {
        if self.cursor >= self.chars.len() {
            return false;
        }
        self.chars.remove(self.cursor);
        true
    }

    pub fn left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn right(&mut self) {
        if self.cursor < self.chars.len() {
            self.cursor += 1;
        }
    }

    pub fn home(&mut self) {
        self.cursor = 0;
    }

    pub fn end(&mut self) {
        self.cursor = self.chars.len();
    }

    /// Move left to the start of the previous word (Option+Left on macOS).
    pub fn word_left(&mut self) {
        while self.cursor > 0 && !is_word(self.chars[self.cursor - 1]) {
            self.cursor -= 1;
        }
        while self.cursor > 0 && is_word(self.chars[self.cursor - 1]) {
            self.cursor -= 1;
        }
    }

    /// Move right past the end of the next word (Option+Right on macOS).
    pub fn word_right(&mut self) {
        let n = self.chars.len();
        while self.cursor < n && !is_word(self.chars[self.cursor]) {
            self.cursor += 1;
        }
        while self.cursor < n && is_word(self.chars[self.cursor]) {
            self.cursor += 1;
        }
    }

    /// Delete the word before the cursor (Option+Backspace).
    pub fn delete_word_back(&mut self) -> bool {
        let end = self.cursor;
        self.word_left();
        if self.cursor < end {
            self.chars.drain(self.cursor..end);
            true
        } else {
            false
        }
    }

    /// Delete the word after the cursor (Option+Delete / fn+Option+Backspace).
    pub fn delete_word_forward(&mut self) -> bool {
        let start = self.cursor;
        let n = self.chars.len();
        let mut end = self.cursor;
        while end < n && !is_word(self.chars[end]) {
            end += 1;
        }
        while end < n && is_word(self.chars[end]) {
            end += 1;
        }
        if end > start {
            self.chars.drain(start..end);
            true
        } else {
            false
        }
    }

    /// Delete from the cursor to the line start (Cmd+Backspace).
    pub fn delete_to_start(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }
        self.chars.drain(0..self.cursor);
        self.cursor = 0;
        true
    }

    /// Delete from the cursor to the line end (Cmd+Delete / Ctrl+K).
    pub fn delete_to_end(&mut self) -> bool {
        if self.cursor >= self.chars.len() {
            return false;
        }
        self.chars.truncate(self.cursor);
        true
    }

    /// The text before and after the cursor, for rendering a caret between.
    pub fn split(&self) -> (String, String) {
        (
            self.chars[..self.cursor].iter().collect(),
            self.chars[self.cursor..].iter().collect(),
        )
    }
}

/// Word characters for word-wise navigation/deletion.
fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

#[cfg(test)]
#[path = "../tests/textedit.rs"]
mod tests;
