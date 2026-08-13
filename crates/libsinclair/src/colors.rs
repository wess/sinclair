//! Resolve vt cell colors against a theme scheme into drawable colors.

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
    /// Minimum fg/bg contrast ratio (WCAG, 1..=21). 1 means no enforcement.
    pub min_contrast: f32,
}

impl Colors {
    /// Build the color set straight from a theme scheme, with no contrast
    /// enforcement. Embedders start here; hosts with their own settings
    /// layer (like the Sinclair app) build `Colors` from those instead.
    pub fn from_scheme(scheme: &theme::Scheme) -> Colors {
        Colors {
            palette: theme::build(scheme, &[]),
            fg: scheme.foreground,
            bg: scheme.background,
            cursor: scheme.cursor,
            cursor_text: scheme.cursor_text,
            selection_bg: scheme.selection_background,
            selection_fg: scheme.selection_foreground,
            min_contrast: 1.0,
        }
    }
}

/// Contrast floor the cursor keeps against the cell it covers. Unlike
/// `minimum_contrast` (opt-in, for text) this is always on: themes pick their
/// cursor color against the theme background, but full-screen programs paint
/// their own cell backgrounds and can otherwise swallow the cursor entirely.
pub const CURSOR_MIN_CONTRAST: f32 = 3.0;

/// Relative luminance (WCAG) of an sRGB color, 0.0 (black) .. 1.0 (white).
fn luminance(c: Rgb) -> f32 {
    let lin = |v: u8| {
        let v = v as f32 / 255.0;
        if v <= 0.03928 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(c.r) + 0.7152 * lin(c.g) + 0.0722 * lin(c.b)
}

/// WCAG contrast ratio between two colors (1.0 .. 21.0).
fn contrast_ratio(a: Rgb, b: Rgb) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = if la >= lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// Adjust `fg` toward black or white (whichever raises contrast against `bg`)
/// until it meets `min` ratio, or the endpoint is reached. Returns `fg`
/// unchanged when `min <= 1` or the pair already passes.
pub fn enforce_contrast(fg: Rgb, bg: Rgb, min: f32) -> Rgb {
    if min <= 1.0 || contrast_ratio(fg, bg) >= min {
        return fg;
    }
    // Push toward whichever endpoint is farther from the background luminance.
    let target = if luminance(bg) < 0.5 {
        Rgb::new(255, 255, 255)
    } else {
        Rgb::new(0, 0, 0)
    };
    let mut lo = 0.0f32;
    let mut hi = 1.0f32;
    let lerp = |t: f32| {
        let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
        Rgb::new(
            mix(fg.r, target.r),
            mix(fg.g, target.g),
            mix(fg.b, target.b),
        )
    };
    // Binary-search the smallest blend toward `target` that meets `min`.
    for _ in 0..12 {
        let mid = (lo + hi) / 2.0;
        if contrast_ratio(lerp(mid), bg) >= min {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    lerp(hi)
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
#[cfg(feature = "ui")]
pub fn rgba(c: Rgb) -> gpui::Rgba {
    gpui::Rgba {
        r: c.r as f32 / 255.0,
        g: c.g as f32 / 255.0,
        b: c.b as f32 / 255.0,
        a: 1.0,
    }
}

/// Theme color as a gpui Hsla (opaque).
#[cfg(feature = "ui")]
pub fn hsla(c: Rgb) -> gpui::Hsla {
    rgba(c).into()
}

#[cfg(test)]
#[path = "../tests/colors.rs"]
mod tests;
