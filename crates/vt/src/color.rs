//! Cell colors: terminal default, 256-color indexed, and truecolor.

/// A color as stored in a cell. Resolution to actual RGB happens at render
/// time against a theme palette (plus any OSC 4 overrides held by the term).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Color {
    /// The terminal default foreground/background.
    #[default]
    Default,
    /// One of the 256 palette slots.
    Indexed(u8),
    /// Direct 24-bit color.
    Rgb(u8, u8, u8),
}

/// The 16 classic ANSI palette slots, by name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum NamedColor {
    Black = 0,
    Red = 1,
    Green = 2,
    Yellow = 3,
    Blue = 4,
    Magenta = 5,
    Cyan = 6,
    White = 7,
    BrightBlack = 8,
    BrightRed = 9,
    BrightGreen = 10,
    BrightYellow = 11,
    BrightBlue = 12,
    BrightMagenta = 13,
    BrightCyan = 14,
    BrightWhite = 15,
}

impl NamedColor {
    /// Palette index for this named color.
    pub fn index(self) -> u8 {
        self as u8
    }
}

impl From<NamedColor> for Color {
    fn from(named: NamedColor) -> Self {
        Color::Indexed(named as u8)
    }
}

#[cfg(test)]
#[path = "../tests/color.rs"]
mod tests;
