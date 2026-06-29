//! Bridge Prompt's terminal-centric theme onto guise's Mantine-style theme.
//!
//! guise components resolve their colors from a single app-global
//! [`guise::Theme`]. We derive that theme from the active terminal palette so
//! the chrome (dialogs, panels, inputs built with guise) tracks the same
//! foreground/background/accent the terminal uses, and re-derive it on every
//! live config reload.

use gpui::App;

use crate::colors::{self, Colors};
use crate::tabbar::blend;
use theme::Rgb;

/// Install (or replace) the global guise theme derived from `colors`.
pub fn install(colors: &Colors, cx: &mut App) {
    derive(colors).init(cx);
}

/// Build a guise theme from the resolved terminal colors. Dark vs. light is
/// chosen by the background luminance; the semantic colors are blends of the
/// foreground and background, with the selection color as the accent.
pub fn derive(colors: &Colors) -> guise::Theme {
    let bg = colors.bg;
    let fg = colors.fg;
    let base = if luminance(bg) < 0.5 {
        guise::Theme::dark()
    } else {
        guise::Theme::light()
    };
    base.with_body(colors::hsla(bg))
        .with_text(colors::hsla(fg))
        .with_surface(colors::hsla(blend(bg, fg, 0.06)))
        .with_surface_hover(colors::hsla(blend(bg, fg, 0.10)))
        .with_border(colors::hsla(blend(bg, fg, 0.18)))
        .with_dimmed(colors::hsla(blend(fg, bg, 0.40)))
        .with_primary(colors::hsla(colors.selection_bg))
}

/// Perceptual-ish luminance in 0..=1 (Rec. 601 weights).
fn luminance(c: Rgb) -> f32 {
    (0.299 * c.r as f32 + 0.587 * c.g as f32 + 0.114 * c.b as f32) / 255.0
}
