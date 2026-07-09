//! Secret redaction: mask configured regex matches before text reaches the
//! clipboard, so API keys and tokens don't leak on copy. Invalid patterns are
//! dropped (with a warning) rather than aborting the whole set.

use std::rc::Rc;

use regex::Regex;

/// App-wide redactor, shared as a gpui global so any pane's copy path can reach
/// it without threading it through the render chain. Refreshed on config load.
pub struct Redaction(pub Rc<Redactor>);

impl gpui::Global for Redaction {}

/// Mask `text` through the current global redactor, if one is installed.
pub fn mask(text: String, cx: &gpui::App) -> String {
    match cx.try_global::<Redaction>() {
        Some(r) if !r.0.is_empty() => r.0.mask(&text),
        _ => text,
    }
}

/// Install (or replace) the global redactor from the configured patterns.
pub fn install(patterns: &[String], cx: &mut gpui::App) {
    cx.set_global(Redaction(Rc::new(Redactor::compile(patterns))));
}

/// A compiled set of redaction patterns.
#[derive(Default)]
pub struct Redactor {
    patterns: Vec<Regex>,
}

impl Redactor {
    /// Compile the configured `redact` patterns, skipping any that don't parse.
    pub fn compile(patterns: &[String]) -> Self {
        let patterns = patterns
            .iter()
            .filter(|p| !p.trim().is_empty())
            .filter_map(|p| match Regex::new(p) {
                Ok(re) => Some(re),
                Err(e) => {
                    eprintln!("sinclair: invalid redact pattern {p:?}: {e}");
                    None
                }
            })
            .collect();
        Self { patterns }
    }

    pub fn is_empty(&self) -> bool {
        self.patterns.is_empty()
    }

    /// Replace every match of every pattern with `•` of the same character
    /// length, preserving the surrounding text and line structure.
    pub fn mask(&self, text: &str) -> String {
        if self.patterns.is_empty() {
            return text.to_string();
        }
        let mut out = text.to_string();
        for re in &self.patterns {
            out = re
                .replace_all(&out, |caps: &regex::Captures| {
                    "\u{2022}".repeat(caps[0].chars().count())
                })
                .into_owned();
        }
        out
    }
}

#[cfg(test)]
#[path = "../tests/redact.rs"]
mod tests;
