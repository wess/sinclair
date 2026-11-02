//! The effective keymap as data, for `sinclair keymap`: every binding that
//! is live after defaults, plugins, and settings.json are layered, where each
//! one came from, and every entry that was rejected and why.

use crate::json;
use crate::keybind::resolve_tagged;
use crate::keymode::{parse_keymode, resolve_modes, undefined_targets, KeyMode};
use crate::options::Options;

/// Where a binding (or a rejected entry) came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
  Default,
  Plugin,
  /// settings.json, with the 1-based line when it could be found (0 if not).
  Settings(usize),
}

impl Source {
  /// `default`, `plugin`, `settings.json:12`, or `settings.json`.
  pub fn label(&self) -> String {
    match self {
      Self::Default => "default".into(),
      Self::Plugin => "plugin".into(),
      Self::Settings(0) => "settings.json".into(),
      Self::Settings(line) => format!("settings.json:{line}"),
    }
  }
}

/// One live binding.
#[derive(Debug, Clone, PartialEq)]
pub struct Binding {
  pub trigger: String,
  pub mode: Option<String>,
  pub action: String,
  pub source: Source,
}

/// An entry that didn't take effect, or did but looks like a mistake.
#[derive(Debug, Clone, PartialEq)]
pub struct Problem {
  pub entry: String,
  pub message: String,
  pub source: Source,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Keymap {
  /// Always-on bindings first, then each mode's, in resolution order.
  pub bindings: Vec<Binding>,
  pub modes: Vec<KeyMode>,
  pub problems: Vec<Problem>,
}

/// Build the keymap from loaded options, the settings.json text they came
/// from (only used to find line numbers), and plugin-contributed keybinds.
/// Plugins resolve before settings, as in the app, so the user wins.
pub fn build(opts: &Options, text: Option<&str>, plugin: &[String]) -> Keymap {
  let bind_lines = text
    .map(|t| entry_lines(t, "keybind", &opts.keybind))
    .unwrap_or_else(|| vec![0; opts.keybind.len()]);
  let raw = plugin.iter().map(|e| (e.as_str(), Source::Plugin)).chain(
    opts
      .keybind
      .iter()
      .zip(bind_lines)
      .map(|(e, line)| (e.as_str(), Source::Settings(line))),
  );
  let (resolved, bad) = resolve_tagged(raw, Source::Default);
  let mut problems: Vec<Problem> = bad
    .into_iter()
    .map(|(entry, message, source)| Problem {
      entry,
      message,
      source,
    })
    .collect();

  let binds: Vec<_> = resolved.iter().map(|(kb, _)| kb.clone()).collect();
  let (modes, _) = resolve_modes(&opts.keymode, &binds);
  let mode_lines = text
    .map(|t| entry_lines(t, "keymode", &opts.keymode))
    .unwrap_or_else(|| vec![0; opts.keymode.len()]);
  for (entry, line) in opts.keymode.iter().zip(mode_lines) {
    if let Err(message) = parse_keymode(entry) {
      problems.push(Problem {
        entry: entry.clone(),
        message,
        source: Source::Settings(line),
      });
    }
  }
  for (i, name) in undefined_targets(&binds, &modes) {
    let (kb, source) = &resolved[i];
    problems.push(Problem {
      entry: kb.config_line(),
      message: format!("mode `{name}` has no bindings and no `keymode` declaration"),
      source: *source,
    });
  }

  let mut bindings: Vec<Binding> = resolved
    .into_iter()
    .map(|(kb, source)| Binding {
      trigger: kb.trigger(),
      mode: kb.mode.clone(),
      action: kb.action.to_config(),
      source,
    })
    .collect();
  // Stable, so resolution order holds within each group.
  bindings.sort_by(|a, b| a.mode.cmp(&b.mode));
  Keymap {
    bindings,
    modes,
    problems,
  }
}

/// The line of each `entries` string inside the top-level `key` member,
/// 0 where it can't be located. Searches the quoted literal forward from
/// the member's line, so duplicates map to successive occurrences. An entry
/// written with escapes the quoting doesn't reproduce falls back to the
/// member's own line.
pub fn entry_lines(text: &str, key: &str, entries: &[String]) -> Vec<usize> {
  let Some(member) = json::root(text)
    .ok()
    .and_then(|members| members.into_iter().rfind(|m| m.key == key))
  else {
    return vec![0; entries.len()];
  };
  let mut cursor = line_offset(text, member.line);
  entries
    .iter()
    .map(|entry| match text[cursor..].find(&json::quote(entry)) {
      Some(at) => {
        cursor += at + 1;
        text[..cursor].matches('\n').count() + 1
      }
      None => member.line,
    })
    .collect()
}

/// Byte offset where 1-based `line` starts.
fn line_offset(text: &str, line: usize) -> usize {
  if line <= 1 {
    return 0;
  }
  text
    .match_indices('\n')
    .nth(line - 2)
    .map(|(i, _)| i + 1)
    .unwrap_or(text.len())
}

/// The plain-text listing: one aligned row per binding, then the modes and
/// any problems.
pub fn render_text(map: &Keymap) -> String {
  let rows: Vec<[String; 4]> = map
    .bindings
    .iter()
    .map(|b| {
      [
        b.trigger.clone(),
        b.mode.clone().unwrap_or_else(|| "-".into()),
        b.action.clone(),
        b.source.label(),
      ]
    })
    .collect();
  let header = [
    "TRIGGER".to_string(),
    "MODE".into(),
    "ACTION".into(),
    "SOURCE".into(),
  ];
  let mut widths = [0usize; 4];
  for row in std::iter::once(&header).chain(&rows) {
    for (w, cell) in widths.iter_mut().zip(row) {
      *w = (*w).max(cell.chars().count());
    }
  }
  let mut out = String::new();
  for row in std::iter::once(&header).chain(&rows) {
    let line = format!(
      "{:w0$}  {:w1$}  {:w2$}  {}",
      row[0],
      row[1],
      row[2],
      row[3],
      w0 = widths[0],
      w1 = widths[1],
      w2 = widths[2],
    );
    out.push_str(line.trim_end());
    out.push('\n');
  }
  if !map.modes.is_empty() {
    out.push_str("\nmodes:\n");
    for m in &map.modes {
      let opts = if m.exclusive { " (exclusive)" } else { "" };
      out.push_str(&format!("  {}{opts}\n", m.name));
    }
  }
  if !map.problems.is_empty() {
    out.push_str(&format!("\nproblems ({}):\n", map.problems.len()));
    for p in &map.problems {
      out.push_str(&format!(
        "  {}: `{}`: {}\n",
        p.source.label(),
        p.entry,
        p.message
      ));
    }
  }
  out
}

/// The listing as a JSON document.
pub fn render_json(map: &Keymap) -> String {
  let q = |s: &str| json::quote(s);
  let line = |s: &Source| match s {
    Source::Settings(n) if *n > 0 => n.to_string(),
    _ => "null".into(),
  };
  let source = |s: &Source| match s {
    Source::Default => "default",
    Source::Plugin => "plugin",
    Source::Settings(_) => "settings.json",
  };
  let bindings: Vec<String> = map
    .bindings
    .iter()
    .map(|b| {
      format!(
        "{{\"trigger\":{},\"mode\":{},\"action\":{},\"source\":{},\"line\":{}}}",
        q(&b.trigger),
        b.mode.as_deref().map(q).unwrap_or_else(|| "null".into()),
        q(&b.action),
        q(source(&b.source)),
        line(&b.source),
      )
    })
    .collect();
  let modes: Vec<String> = map
    .modes
    .iter()
    .map(|m| format!("{{\"name\":{},\"exclusive\":{}}}", q(&m.name), m.exclusive))
    .collect();
  let problems: Vec<String> = map
    .problems
    .iter()
    .map(|p| {
      format!(
        "{{\"entry\":{},\"message\":{},\"source\":{},\"line\":{}}}",
        q(&p.entry),
        q(&p.message),
        q(source(&p.source)),
        line(&p.source),
      )
    })
    .collect();
  format!(
    "{{\"bindings\":[{}],\"modes\":[{}],\"problems\":[{}]}}\n",
    bindings.join(","),
    modes.join(","),
    problems.join(",")
  )
}

#[cfg(test)]
#[path = "../tests/keymap.rs"]
mod tests;
