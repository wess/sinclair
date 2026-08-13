//! settings.json loading: the layered model. Compiled-in defaults come from
//! [`Options::default`]; the user file overrides only the keys it names.
//! Every problem — bad syntax, unknown key, wrong type, unparseable value —
//! becomes a [`Diagnostic`] and the affected key falls back to its default;
//! the load itself never fails.

use crate::apply::apply;
use crate::json::{self, Value};
use crate::kind::{kind, Kind};
use crate::options::Options;
use crate::parse::Diagnostic;

/// Parse settings.json text into options plus any diagnostics.
pub fn parse_json_str(text: &str) -> (Options, Vec<Diagnostic>) {
    let defaults = Options::default();
    let mut opts = defaults.clone();
    let mut diags = Vec::new();

    let members = match json::root(text) {
        Ok(members) => members,
        Err(e) => {
            diags.push(Diagnostic {
                line: e.line,
                key: String::new(),
                message: e.message,
            });
            return (opts, diags);
        }
    };

    for m in &members {
        let Some(kind) = kind(&m.key) else {
            diags.push(Diagnostic {
                line: m.line,
                key: m.key.clone(),
                message: format!("unknown setting `{}`", m.key),
            });
            continue;
        };
        match coerce(kind, &m.value) {
            Ok(values) => {
                for v in values {
                    if let Err(message) = apply(&mut opts, &defaults, &m.key, &v) {
                        diags.push(Diagnostic {
                            line: m.line,
                            key: m.key.clone(),
                            message,
                        });
                    }
                }
            }
            Err(message) => diags.push(Diagnostic {
                line: m.line,
                key: m.key.clone(),
                message,
            }),
        }
    }

    (opts, diags)
}

/// The keys the user's file actually sets (the GUI's modified indicators).
/// Unparseable text yields an empty list.
pub fn user_keys(text: &str) -> Vec<String> {
    json::root(text)
        .map(|members| members.into_iter().map(|m| m.key).collect())
        .unwrap_or_default()
}

/// Turn a JSON value into the string form(s) `apply` takes: one string per
/// scalar, one per element for lists. `null` becomes the empty string, which
/// `apply` treats as reset-to-default.
fn coerce(kind: Kind, value: &Value) -> Result<Vec<String>, String> {
    if matches!(value, Value::Null) {
        return Ok(vec![String::new()]);
    }
    match kind {
        Kind::Bool => match value {
            Value::Bool(b) => Ok(vec![b.to_string()]),
            Value::Str(s) => Ok(vec![s.clone()]),
            _ => Err("expected true or false".to_string()),
        },
        Kind::Int | Kind::Float => match value {
            Value::Num(n) => Ok(vec![json::num(*n)]),
            Value::Str(s) => Ok(vec![s.clone()]),
            _ => Err("expected a number".to_string()),
        },
        Kind::Str => match value {
            Value::Str(s) => Ok(vec![s.clone()]),
            Value::Num(n) => Ok(vec![json::num(*n)]),
            Value::Bool(b) => Ok(vec![b.to_string()]),
            _ => Err("expected a string".to_string()),
        },
        Kind::List => match value {
            Value::Arr(items) => items
                .iter()
                .map(|item| match item {
                    Value::Str(s) => Ok(s.clone()),
                    Value::Num(n) => Ok(json::num(*n)),
                    Value::Bool(b) => Ok(b.to_string()),
                    _ => Err("expected an array of strings".to_string()),
                })
                .collect(),
            Value::Str(s) => Ok(vec![s.clone()]),
            _ => Err("expected an array of strings".to_string()),
        },
    }
}

/// Encode one string value (as the legacy format or the GUI hands it over)
/// as JSON source text of the key's kind. Values that don't fit the kind
/// stay quoted strings — the load will surface the diagnostic.
pub fn encode(key: &str, value: &str) -> String {
    match kind(key) {
        Some(Kind::Bool) => match crate::value::parse_bool(value) {
            Some(b) => b.to_string(),
            None => json::quote(value),
        },
        Some(Kind::Int) | Some(Kind::Float) => match value.parse::<f64>() {
            Ok(n) if n.is_finite() => json::num(n),
            _ => json::quote(value),
        },
        Some(Kind::List) => format!("[{}]", json::quote(value)),
        _ => json::quote(value),
    }
}

/// Encode a list value as a JSON array, one line per entry when non-empty.
pub fn encode_list(values: &[String]) -> String {
    if values.is_empty() {
        return "[]".to_string();
    }
    let items: Vec<String> = values
        .iter()
        .map(|v| format!("    {}", json::quote(v)))
        .collect();
    format!("[\n{}\n  ]", items.join(",\n"))
}

/// Convert a legacy `key = value` config into settings.json text. Scalar
/// keys keep their last occurrence (matching how the legacy parser applied
/// them in order); repeated keys collect into arrays. Empty values meant
/// reset-to-default, so they are simply dropped.
pub fn from_legacy(text: &str) -> String {
    let mut order: Vec<String> = Vec::new();
    let mut values: Vec<(String, Vec<String>)> = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((key, val)) = trimmed.split_once('=') else {
            continue;
        };
        let key = key.trim().to_string();
        let val = crate::value::unquote(val.trim()).to_string();
        if val.is_empty() {
            continue;
        }
        match values.iter_mut().find(|(k, _)| *k == key) {
            Some((_, list)) if kind(&key) == Some(Kind::List) => list.push(val),
            Some((_, list)) => *list = vec![val],
            None => {
                order.push(key.clone());
                values.push((key, vec![val]));
            }
        }
    }

    let mut out = String::from(
        "// Sinclair settings — JSON with comments, migrated from the legacy\n\
         // `config` file (kept next to this one, no longer read). Edits apply\n\
         // live; remove a key to fall back to its built-in default.\n{\n",
    );
    for key in &order {
        let Some((_, vals)) = values.iter().find(|(k, _)| k == key) else {
            continue;
        };
        let raw = if kind(key) == Some(Kind::List) {
            encode_list(vals)
        } else {
            encode(key, &vals[0])
        };
        out.push_str(&format!("  {}: {},\n", json::quote(key), raw));
    }
    // Drop the trailing comma from the final member.
    if out.ends_with(",\n") {
        out.truncate(out.len() - 2);
        out.push('\n');
    }
    out.push_str("}\n");
    out
}

/// The starter file written when no settings exist yet.
pub fn starter() -> String {
    "// Sinclair settings — JSON with comments. Edits apply live; every key\n\
     // is optional, and a removed key falls back to its built-in default.\n\
     {\n}\n"
        .to_string()
}

#[cfg(test)]
#[path = "../tests/settings.rs"]
mod tests;
