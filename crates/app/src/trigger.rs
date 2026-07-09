//! Regex output triggers: when new terminal output matches a
//! configured pattern, fire a desktop notification. App-wide, shared as a gpui
//! global and refreshed on config load. Distinct from plugin `[[trigger]]`
//! event hooks (`root/triggers.rs`), which react to terminal *events*.

use std::rc::Rc;

use regex::Regex;

struct Trigger {
    re: Regex,
    title: Option<String>,
}

/// The compiled set of output triggers.
#[derive(Default)]
pub struct Triggers {
    list: Vec<Trigger>,
}

/// gpui global holding the current triggers.
pub struct TriggersGlobal(pub Rc<Triggers>);

impl gpui::Global for TriggersGlobal {}

impl Triggers {
    /// Compile `regex` / `regex | title` entries, skipping any that don't parse.
    pub fn compile(entries: &[String]) -> Self {
        let list = entries
            .iter()
            .filter_map(|entry| {
                let (pat, title) = match entry.split_once('|') {
                    Some((p, t)) => {
                        let t = t.trim();
                        (p.trim(), (!t.is_empty()).then(|| t.to_string()))
                    }
                    None => (entry.trim(), None),
                };
                if pat.is_empty() {
                    return None;
                }
                match Regex::new(pat) {
                    Ok(re) => Some(Trigger { re, title }),
                    Err(e) => {
                        eprintln!("sinclair: invalid trigger {pat:?}: {e}");
                        None
                    }
                }
            })
            .collect();
        Self { list }
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// The `(title, line)` of the first trigger matching `line`, if any.
    pub fn check(&self, line: &str) -> Option<(String, String)> {
        self.list.iter().find(|t| t.re.is_match(line)).map(|t| {
            (
                t.title.clone().unwrap_or_else(|| "Trigger".to_string()),
                line.to_string(),
            )
        })
    }
}

/// The current global triggers, or an empty set if none installed.
pub fn current(cx: &gpui::App) -> Option<Rc<Triggers>> {
    cx.try_global::<TriggersGlobal>()
        .map(|g| g.0.clone())
        .filter(|t| !t.is_empty())
}

/// Install (or replace) the global triggers from the configured patterns.
pub fn install(entries: &[String], cx: &mut gpui::App) {
    cx.set_global(TriggersGlobal(Rc::new(Triggers::compile(entries))));
}

#[cfg(test)]
#[path = "../tests/trigger.rs"]
mod tests;
