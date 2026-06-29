//! macOS-style key handling for single-line text fields, shared by every modal
//! (rename, settings, new agent). Maps a keystroke onto [`TextEdit`] ops and
//! tells the caller whether to submit, cancel, redraw, or pass the key through.

use crate::textedit::TextEdit;
use gpui::Keystroke;

pub enum Outcome {
    /// Enter, the caller should commit.
    Submit,
    /// Escape, the caller should dismiss.
    Cancel,
    /// The field changed; redraw.
    Edited,
    /// Not handled here; the caller may act on it (e.g. Tab, Cmd+W).
    Pass,
}

/// Apply `ks` to `edit`. `platform` is Cmd on macOS; `alt` is Option.
pub fn apply(edit: &mut TextEdit, ks: &Keystroke) -> Outcome {
    let m = &ks.modifiers;
    match ks.key.as_str() {
        "enter" => return Outcome::Submit,
        "escape" => return Outcome::Cancel,
        "left" => {
            if m.platform {
                edit.home();
            } else if m.alt {
                edit.word_left();
            } else {
                edit.left();
            }
            return Outcome::Edited;
        }
        "right" => {
            if m.platform {
                edit.end();
            } else if m.alt {
                edit.word_right();
            } else {
                edit.right();
            }
            return Outcome::Edited;
        }
        "up" => {
            edit.home();
            return Outcome::Edited;
        }
        "down" => {
            edit.end();
            return Outcome::Edited;
        }
        "home" => {
            edit.home();
            return Outcome::Edited;
        }
        "end" => {
            edit.end();
            return Outcome::Edited;
        }
        "backspace" => {
            if m.platform {
                edit.delete_to_start();
            } else if m.alt {
                edit.delete_word_back();
            } else {
                edit.backspace();
            }
            return Outcome::Edited;
        }
        "delete" => {
            if m.platform {
                edit.delete_to_end();
            } else if m.alt {
                edit.delete_word_forward();
            } else {
                edit.delete();
            }
            return Outcome::Edited;
        }
        "k" if m.control => {
            edit.delete_to_end();
            return Outcome::Edited;
        }
        "a" if m.control => {
            edit.home();
            return Outcome::Edited;
        }
        "e" if m.control => {
            edit.end();
            return Outcome::Edited;
        }
        _ => {}
    }
    if !m.platform && !m.control {
        if let Some(t) = ks.key_char.as_deref().filter(|t| !t.is_empty()) {
            edit.insert(t);
            return Outcome::Edited;
        }
    }
    Outcome::Pass
}
