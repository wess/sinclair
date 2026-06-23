//! 24-bit RGB color with hex parsing and formatting.

use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseRgbError {
    /// Input was not 3 or 6 hex digits (after an optional `#`).
    Length,
    /// Input contained a non-hex character.
    Digit,
}

impl fmt::Display for ParseRgbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseRgbError::Length => write!(f, "expected 3 or 6 hex digits"),
            ParseRgbError::Digit => write!(f, "invalid hex digit"),
        }
    }
}

impl std::error::Error for ParseRgbError {}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Rgb {
        Rgb { r, g, b }
    }

    /// Lowercase `#rrggbb` form.
    pub fn to_hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }

    /// Relative luminance in `0.0..=1.0` (Rec. 709 weights, linear approximation).
    pub fn luminance(self) -> f64 {
        (0.2126 * self.r as f64 + 0.7152 * self.g as f64 + 0.0722 * self.b as f64) / 255.0
    }
}

fn nibble(b: u8) -> Result<u8, ParseRgbError> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        b'A'..=b'F' => Ok(b - b'A' + 10),
        _ => Err(ParseRgbError::Digit),
    }
}

impl FromStr for Rgb {
    type Err = ParseRgbError;

    /// Accepts `#rgb`, `#rrggbb`, and the same forms without the `#`.
    fn from_str(s: &str) -> Result<Rgb, ParseRgbError> {
        let hex = s.strip_prefix('#').unwrap_or(s).as_bytes();
        match hex.len() {
            3 => {
                let r = nibble(hex[0])?;
                let g = nibble(hex[1])?;
                let b = nibble(hex[2])?;
                Ok(Rgb::new(r * 17, g * 17, b * 17))
            }
            6 => {
                let r = nibble(hex[0])? * 16 + nibble(hex[1])?;
                let g = nibble(hex[2])? * 16 + nibble(hex[3])?;
                let b = nibble(hex[4])? * 16 + nibble(hex[5])?;
                Ok(Rgb::new(r, g, b))
            }
            _ => Err(ParseRgbError::Length),
        }
    }
}

impl fmt::Display for Rgb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }
}

#[cfg(test)]
#[path = "../tests/rgb.rs"]
mod tests;
