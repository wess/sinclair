//! Resolve vt cell colors against the configured theme into gpui colors.

use theme::{Palette, Rgb};

/// Everything the renderer needs to turn a [`vt::Color`] into pixels.
pub struct Colors {
    pub palette: Palette,
    pub fg: Rgb,
    pub bg: Rgb,
    pub cursor: Rgb,
    pub cursor_text: Rgb,
    pub selection_bg: Rgb,
    pub selection_fg: Rgb,
}

/// Build the color set from configuration: scheme by name (default dark),
/// 256-entry palette with config `palette` overrides, and config
/// foreground/background overrides on top of the scheme.
pub fn from_config(opts: &config::Options) -> Colors {
    let scheme = match theme::builtin(&opts.theme) {
        Some(scheme) => scheme,
        None => {
            if !opts.theme.is_empty() {
                eprintln!("prompt: unknown theme {:?}, using default", opts.theme);
            }
            theme::default_scheme()
        }
    };
    let overrides: Vec<(u8, Rgb)> = opts
        .palette
        .iter()
        .filter_map(|(index, hex)| hex.parse::<Rgb>().ok().map(|rgb| (*index, rgb)))
        .collect();
    let parse = |hex: &Option<String>, fallback: Rgb| {
        hex.as_deref()
            .and_then(|s| s.parse::<Rgb>().ok())
            .unwrap_or(fallback)
    };
    Colors {
        palette: theme::build(scheme, &overrides),
        fg: parse(&opts.foreground, scheme.foreground),
        bg: parse(&opts.background, scheme.background),
        cursor: scheme.cursor,
        cursor_text: scheme.cursor_text,
        selection_bg: scheme.selection_background,
        selection_fg: scheme.selection_foreground,
    }
}

/// The colors the terminal should report to programs that query them
/// (OSC 4/10/11/12), built from the resolved theme + overrides.
pub fn report_colors(c: &Colors) -> vt::ReportColors {
    let mut palette = [(0u8, 0u8, 0u8); 256];
    for (i, entry) in palette.iter_mut().enumerate() {
        let rgb = c.palette.get(i as u8);
        *entry = (rgb.r, rgb.g, rgb.b);
    }
    let triple = |rgb: Rgb| (rgb.r, rgb.g, rgb.b);
    vt::ReportColors {
        foreground: triple(c.fg),
        background: triple(c.bg),
        cursor: triple(c.cursor),
        palette,
    }
}

/// Resolve one cell color. `default` is the terminal default fg or bg,
/// `brighten` promotes ANSI 0..=7 to 8..=15 (classic bold brightening),
/// and `term_override` supplies live OSC 4 palette overrides by index.
pub fn cell_rgb(
    color: vt::Color,
    default: Rgb,
    brighten: bool,
    palette: &Palette,
    term_override: impl Fn(u8) -> Option<(u8, u8, u8)>,
) -> Rgb {
    match color {
        vt::Color::Default => default,
        vt::Color::Indexed(index) => {
            let index = if brighten && index < 8 {
                index + 8
            } else {
                index
            };
            match term_override(index) {
                Some((r, g, b)) => Rgb::new(r, g, b),
                None => palette.get(index),
            }
        }
        vt::Color::Rgb(r, g, b) => Rgb::new(r, g, b),
    }
}

/// Theme color as a gpui Rgba (opaque).
pub fn rgba(c: Rgb) -> gpui::Rgba {
    gpui::Rgba {
        r: c.r as f32 / 255.0,
        g: c.g as f32 / 255.0,
        b: c.b as f32 / 255.0,
        a: 1.0,
    }
}

/// Theme color as a gpui Hsla (opaque).
pub fn hsla(c: Rgb) -> gpui::Hsla {
    rgba(c).into()
}

#[cfg(test)]
#[path = "../tests/colors.rs"]
mod tests;
