//! Ghostty-style `key = value` parser. Never hard-fails: bad lines become
//! diagnostics and parsing continues.

use crate::apply::apply;
use crate::options::Options;
use crate::value;

/// A non-fatal problem found while parsing a config file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// 1-based line number.
    pub line: usize,
    /// The key on the offending line (empty if none).
    pub key: String,
    /// Human-readable description.
    pub message: String,
}

/// Parse config text into options plus any diagnostics.
pub fn parse_str(input: &str) -> (Options, Vec<Diagnostic>) {
    let mut opts = Options::default();
    let mut diags = Vec::new();

    for (i, raw) in input.lines().enumerate() {
        let lineno = i + 1;
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, val)) = line.split_once('=') else {
            diags.push(Diagnostic {
                line: lineno,
                key: String::new(),
                message: "expected `key = value`".to_string(),
            });
            continue;
        };
        let key = key.trim();
        let val = value::unquote(val.trim());
        if key.is_empty() {
            diags.push(Diagnostic {
                line: lineno,
                key: String::new(),
                message: "missing key before `=`".to_string(),
            });
            continue;
        }
        if let Err(message) = apply(&mut opts, key, val) {
            diags.push(Diagnostic {
                line: lineno,
                key: key.to_string(),
                message,
            });
        }
    }

    (opts, diags)
}

#[cfg(test)]
#[path = "../tests/parse.rs"]
mod tests;
