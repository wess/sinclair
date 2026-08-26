//! Command-line completion: prefix matching, dedup, and a light relevance rank
//! over candidate command lines. Pure and host-agnostic — the caller assembles
//! candidates from its sources (history, common commands, path completions) in
//! priority order and this filters/orders them against the typed input.

/// Candidates that start with `input` (and extend it), deduped, keeping the
/// caller's order, capped at `limit`. `input` is matched as-is (shell commands
/// are case-sensitive); an empty input yields nothing.
pub fn complete(input: &str, candidates: &[String], limit: usize) -> Vec<String> {
  if input.is_empty() {
    return Vec::new();
  }
  let mut seen = std::collections::HashSet::new();
  let mut out = Vec::new();
  for c in candidates {
    if c.len() > input.len() && c.starts_with(input) && seen.insert(c.as_str()) {
      out.push(c.clone());
      if out.len() >= limit {
        break;
      }
    }
  }
  out
}

/// The best single completion's remaining suffix after `input`, or `None`. This
/// is what a ghost-text overlay draws past the cursor.
pub fn ghost(input: &str, candidates: &[String]) -> Option<String> {
  complete(input, candidates, 1)
    .into_iter()
    .next()
    .map(|c| c[input.len()..].to_string())
}

/// Reorder matching candidates by a light relevance heuristic: the closest
/// completion (shortest suffix) first, ties broken by the caller's order. Used
/// when the "assist" source is on to surface tighter matches ahead of older,
/// longer history entries.
pub fn rank(input: &str, candidates: &[String], limit: usize) -> Vec<String> {
  let mut matches = complete(input, candidates, candidates.len());
  // Stable sort by suffix length keeps original order among equal lengths.
  matches.sort_by_key(|c| c.len().saturating_sub(input.len()));
  matches.truncate(limit);
  matches
}

#[cfg(test)]
#[path = "../tests/suggest.rs"]
mod tests;
