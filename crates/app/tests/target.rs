use super::*;

fn panes() -> Vec<Candidate> {
  [(7, "build"), (9, "Dev Server"), (12, "dev logs"), (15, "psql")]
    .into_iter()
    .map(|(token, title)| Candidate {
      token,
      title: title.to_string(),
    })
    .collect()
}

#[test]
fn ids_positions_titles_then_fragments() {
  let p = panes();
  assert_eq!(resolve("pane:12", &p), Ok(2));
  assert_eq!(resolve("2", &p), Ok(1));
  assert_eq!(resolve("PSQL", &p), Ok(3));
  assert_eq!(resolve("server", &p), Ok(1));
}

#[test]
fn whole_title_beats_a_fragment() {
  let mut p = panes();
  p.push(Candidate {
    token: 20,
    title: "build watch".into(),
  });
  assert_eq!(resolve("build", &p), Ok(0));
}

#[test]
fn misses_and_ambiguity_are_labelled() {
  let p = panes();
  assert!(resolve("pane:99", &p).unwrap_err().starts_with(NOT_FOUND));
  assert!(resolve("0", &p).unwrap_err().starts_with(NOT_FOUND));
  assert!(resolve("9", &p).unwrap_err().starts_with(NOT_FOUND));
  assert!(resolve("nope", &p).unwrap_err().starts_with(NOT_FOUND));
  let err = resolve("dev", &p).unwrap_err();
  assert!(err.starts_with(AMBIGUOUS));
  assert!(err.contains("pane:9") && err.contains("pane:12"));
}
