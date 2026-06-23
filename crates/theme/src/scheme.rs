//! A named color scheme: 16 ANSI slots plus UI colors.

use crate::rgb::Rgb;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scheme {
    pub name: &'static str,
    pub ansi: [Rgb; 16],
    pub foreground: Rgb,
    pub background: Rgb,
    pub cursor: Rgb,
    pub cursor_text: Rgb,
    pub selection_foreground: Rgb,
    pub selection_background: Rgb,
}

impl Scheme {
    /// True when the background is dark (below mid luminance).
    pub fn is_dark(&self) -> bool {
        self.background.luminance() < 0.5
    }
}

#[cfg(test)]
#[path = "../tests/scheme.rs"]
mod tests;
