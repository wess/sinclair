//! Naming a pane from outside: the CLI's `-p` and the MCP tools' `pane`
//! argument. One spec resolves, in order, as
//!
//! 1. an id, `pane:<n>` (what `sinclair ls` prints, and `$SINCLAIR_PANE`'s
//!    value with the prefix);
//! 2. a position, `<n>`, counting tabs from one in strip order;
//! 3. a title, matched whole and case-insensitively;
//! 4. a fragment of one title, case-insensitively — only when exactly one
//!    title contains it.
//!
//! A spec that matches nothing, or more than one pane at the same step, is an
//! error naming what it saw; the CLI turns those into exit status 3.

/// What the resolver knows about one pane.
#[derive(Clone, Debug)]
pub struct Candidate {
  /// The pane token (`SINCLAIR_PANE`).
  pub token: u64,
  pub title: String,
}

/// Errors start with one of these so a client can tell a bad target from any
/// other failure without parsing the rest.
pub const NOT_FOUND: &str = "target not found";
pub const AMBIGUOUS: &str = "ambiguous target";

/// The printed form of a pane id.
pub fn id(token: u64) -> String {
  format!("pane:{token}")
}

/// Resolve `spec` against `panes` (in tab order). Returns the index into
/// `panes`.
pub fn resolve(spec: &str, panes: &[Candidate]) -> Result<usize, String> {
  let spec = spec.trim();
  if let Some(n) = spec.strip_prefix("pane:") {
    let token: u64 = n
      .parse()
      .map_err(|_| format!("{NOT_FOUND}: `{spec}` is not a pane id"))?;
    return panes
      .iter()
      .position(|p| p.token == token)
      .ok_or_else(|| format!("{NOT_FOUND}: no pane {spec}"));
  }
  if let Ok(n) = spec.parse::<usize>() {
    return match n {
      1.. if n <= panes.len() => Ok(n - 1),
      _ => Err(format!(
        "{NOT_FOUND}: no tab {n} (there are {})",
        panes.len()
      )),
    };
  }
  let lower = spec.to_lowercase();
  let whole: Vec<usize> = (0..panes.len())
    .filter(|&i| panes[i].title.trim().to_lowercase() == lower)
    .collect();
  if let Some(found) = single(spec, &whole, panes)? {
    return Ok(found);
  }
  let part: Vec<usize> = (0..panes.len())
    .filter(|&i| panes[i].title.to_lowercase().contains(&lower))
    .collect();
  single(spec, &part, panes)?.ok_or_else(|| format!("{NOT_FOUND}: nothing matches `{spec}`"))
}

/// One match, none, or an ambiguity error listing what matched.
fn single(spec: &str, hits: &[usize], panes: &[Candidate]) -> Result<Option<usize>, String> {
  match hits {
    [] => Ok(None),
    [one] => Ok(Some(*one)),
    many => Err(format!(
      "{AMBIGUOUS}: `{spec}` matches {}",
      many
        .iter()
        .map(|&i| format!("{} ({})", id(panes[i].token), panes[i].title))
        .collect::<Vec<_>>()
        .join(", ")
    )),
  }
}

#[cfg(test)]
#[path = "../tests/target.rs"]
mod tests;
