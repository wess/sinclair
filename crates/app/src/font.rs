//! Build a gpui [`Font`] from configuration: primary family, fallback
//! chain (emoji/symbol coverage), base style, and OpenType features
//! (ligatures, stylistic sets, …).

use std::sync::Arc;

use gpui::{Font, FontFallbacks, FontFeatures, FontStyle, FontWeight};

/// Construct the base terminal font from config. Per-cell bold/italic is
/// applied later by the element; this sets the document defaults.
pub fn build(opts: &config::Options) -> Font {
    let (weight, style) = weight_style(opts.font_style);
    Font {
        family: opts.primary_font().to_string().into(),
        features: features(&opts.font_feature),
        fallbacks: fallbacks(opts),
        weight,
        style,
    }
}

fn weight_style(s: config::FontStyle) -> (FontWeight, FontStyle) {
    match s {
        config::FontStyle::Normal => (FontWeight::NORMAL, FontStyle::Normal),
        config::FontStyle::Bold => (FontWeight::BOLD, FontStyle::Normal),
        config::FontStyle::Italic => (FontWeight::NORMAL, FontStyle::Italic),
        config::FontStyle::BoldItalic => (FontWeight::BOLD, FontStyle::Italic),
    }
}

/// The configured fallback families (everything after the primary).
fn fallbacks(opts: &config::Options) -> Option<FontFallbacks> {
    let list = opts.font_fallbacks();
    (!list.is_empty()).then(|| FontFallbacks::from_fonts(list.to_vec()))
}

/// Parse config `font-feature` entries into gpui [`FontFeatures`]. Accepts
/// `liga` / `+liga` (enable), `-liga` (disable), and `tag=N` (explicit
/// value). Unknown shapes are skipped.
pub fn features(entries: &[String]) -> FontFeatures {
    let pairs: Vec<(String, u32)> = entries.iter().filter_map(|e| parse_feature(e)).collect();
    FontFeatures(Arc::new(pairs))
}

fn parse_feature(s: &str) -> Option<(String, u32)> {
    let s = s.trim();
    let (s, signed) = match s.strip_prefix('+') {
        Some(rest) => (rest, Some(1)),
        None => match s.strip_prefix('-') {
            Some(rest) => (rest, Some(0)),
            None => (s, None),
        },
    };
    if let Some((tag, value)) = s.split_once('=') {
        let value: u32 = value.trim().parse().ok()?;
        return valid_tag(tag).map(|t| (t, value));
    }
    valid_tag(s).map(|t| (t, signed.unwrap_or(1)))
}

/// OpenType feature tags are 1–4 ASCII characters.
fn valid_tag(tag: &str) -> Option<String> {
    let tag = tag.trim();
    (!tag.is_empty() && tag.len() <= 4 && tag.is_ascii()).then(|| tag.to_string())
}

#[cfg(test)]
#[path = "../tests/font.rs"]
mod tests;
