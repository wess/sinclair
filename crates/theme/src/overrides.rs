//! Apply config-sourced string overrides on top of a base scheme.

use std::fmt;

use crate::rgb::{ParseRgbError, Rgb};
use crate::scheme::Scheme;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverrideError {
    /// A hex color string failed to parse; `field` names the entry.
    Hex {
        field: String,
        value: String,
        error: ParseRgbError,
    },
    /// An ANSI override index outside `0..=15`.
    Index(u8),
}

impl fmt::Display for OverrideError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OverrideError::Hex {
                field,
                value,
                error,
            } => {
                write!(f, "invalid color {value:?} for {field}: {error}")
            }
            OverrideError::Index(index) => {
                write!(f, "ansi override index {index} out of range (0..=15)")
            }
        }
    }
}

impl std::error::Error for OverrideError {}

fn parse(field: &str, value: &str) -> Result<Rgb, OverrideError> {
    value.parse().map_err(|error| OverrideError::Hex {
        field: field.to_string(),
        value: value.to_string(),
        error,
    })
}

/// Copy `scheme` and replace any field whose override is `Some`, plus
/// the listed ANSI slots (`0..=15`). Hex strings accept `#rgb`,
/// `#rrggbb`, and the same forms without `#`. The first invalid value
/// or out-of-range index aborts with an error; the base scheme is
/// never partially mutated.
#[allow(clippy::too_many_arguments)]
pub fn apply_overrides(
    scheme: &Scheme,
    background: Option<&str>,
    foreground: Option<&str>,
    cursor: Option<&str>,
    cursor_text: Option<&str>,
    selection_foreground: Option<&str>,
    selection_background: Option<&str>,
    ansi: &[(u8, String)],
) -> Result<Scheme, OverrideError> {
    let mut out = *scheme;
    if let Some(value) = background {
        out.background = parse("background", value)?;
    }
    if let Some(value) = foreground {
        out.foreground = parse("foreground", value)?;
    }
    if let Some(value) = cursor {
        out.cursor = parse("cursor", value)?;
    }
    if let Some(value) = cursor_text {
        out.cursor_text = parse("cursor_text", value)?;
    }
    if let Some(value) = selection_foreground {
        out.selection_foreground = parse("selection_foreground", value)?;
    }
    if let Some(value) = selection_background {
        out.selection_background = parse("selection_background", value)?;
    }
    for (index, value) in ansi {
        if *index > 15 {
            return Err(OverrideError::Index(*index));
        }
        out.ansi[*index as usize] = parse(&format!("ansi[{index}]"), value)?;
    }
    Ok(out)
}

#[cfg(test)]
#[path = "../tests/overrides.rs"]
mod tests;
