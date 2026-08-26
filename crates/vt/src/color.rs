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

#[cfg(test)]
#[path = "../tests/color.rs"]
mod tests;
