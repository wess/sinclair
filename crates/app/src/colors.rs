//! Map configuration onto the library color set. The resolution machinery
//! (palette lookup, contrast enforcement, gpui conversions) lives in
//! `libsinclair::colors` and is re-exported here.

pub use libsinclair::colors::*;

use theme::Rgb;

/// Build the color set from configuration: scheme by name (default dark),
/// config color overrides applied through [`theme::apply_overrides`] (ANSI
/// slots included), and the 256-entry palette with the extended (>15) config
/// `palette` entries on top. An invalid override logs a diagnostic and falls
/// back to the plain scheme.
pub fn from_config(opts: &config::Options, dark: bool) -> Colors {
    let name = opts.theme_for(dark);
    let base = match theme::builtin(name) {
        Some(scheme) => scheme,
        None => {
            if !name.is_empty() {
                eprintln!("sinclair: unknown theme {name:?}, using default");
            }
            theme::default_scheme()
        }
    };
    // ANSI slots (0..=15) ride through the scheme; higher indices only exist
    // in the built palette.
    type Slots = Vec<(u8, String)>;
    let (ansi, extended): (Slots, Slots) = opts
        .palette
        .iter()
        .cloned()
        .partition(|(index, _)| *index < 16);
    let scheme = theme::apply_overrides(
        base,
        opts.background.as_deref(),
        opts.foreground.as_deref(),
        opts.cursor_color.as_deref(),
        opts.cursor_text.as_deref(),
        opts.selection_foreground.as_deref(),
        opts.selection_background.as_deref(),
        &ansi,
    )
    .unwrap_or_else(|e| {
        eprintln!("sinclair: {e}; ignoring color overrides");
        *base
    });
    let extended: Vec<(u8, Rgb)> = extended
        .iter()
        .filter_map(|(index, hex)| hex.parse::<Rgb>().ok().map(|rgb| (*index, rgb)))
        .collect();
    Colors {
        palette: theme::build(&scheme, &extended),
        fg: scheme.foreground,
        bg: scheme.background,
        cursor: scheme.cursor,
        cursor_text: scheme.cursor_text,
        selection_bg: scheme.selection_background,
        selection_fg: scheme.selection_foreground,
        min_contrast: opts.minimum_contrast,
    }
}

#[cfg(test)]
#[path = "../tests/colors.rs"]
mod tests;
