use super::*;

fn args(s: &[&str]) -> Vec<String> {
  s.iter().map(|x| x.to_string()).collect()
}

#[test]
fn parses_flags_and_words() {
  let f = parse(&args(&["-p", "build", "--json", "make", "test", "--", "-x"])).unwrap();
  assert_eq!(f.pane.as_deref(), Some("build"));
  assert!(f.json);
  assert_eq!(f.words, vec!["make", "test", "-x"]);
  assert_eq!(parse(&args(&["--bogus"])).unwrap_err().code, USAGE);
  assert_eq!(parse(&args(&["-p"])).unwrap_err().code, USAGE);
}

#[test]
fn durations() {
  assert_eq!(duration("90"), Some(Duration::from_secs(90)));
  assert_eq!(duration("10m"), Some(Duration::from_secs(600)));
  assert_eq!(duration("500ms"), Some(Duration::from_millis(500)));
  assert_eq!(duration("2h"), Some(Duration::from_secs(7200)));
  assert_eq!(duration("x"), None);
  assert_eq!(duration("5d"), None);
}

#[test]
fn errors_map_to_exit_codes() {
  assert_eq!(Failure::from_request("target not found: x".into()).code, TARGET);
  assert_eq!(Failure::from_request("ambiguous target: x".into()).code, TARGET);
  assert_eq!(Failure::from_request("no running sinclair instance".into()).code, UNREACHABLE);
  assert_eq!(Failure::from_request("boom".into()).code, FAILURE);
}

#[test]
fn send_text_unescapes_keys() {
  assert_eq!(unescape(r"q\r"), "q\r");
  assert_eq!(unescape(r"\x03"), "\x03");
  assert_eq!(unescape(r"\e[A\\"), "\x1b[A\\");
  assert_eq!(unescape(r"\q"), r"\q");
}

#[test]
fn wait_needs_exactly_one_condition() {
  let f = parse(&args(&["--exit"])).unwrap();
  assert_eq!(wait::Until::from_flags(&f), Ok(wait::Until::Exit));
  let f = parse(&args(&["--status", "done, error"])).unwrap();
  assert_eq!(
    wait::Until::from_flags(&f),
    Ok(wait::Until::Status(vec!["done".into(), "error".into()]))
  );
  let f = parse(&args(&["--exit", "--command"])).unwrap();
  assert_eq!(wait::Until::from_flags(&f).unwrap_err().code, USAGE);
}

#[test]
fn ls_table_aligns_columns() {
  let out = json!({ "panes": [
    { "index": 1, "id": "pane:1", "signal": "working", "title": "build", "active": true },
    { "index": 2, "id": "pane:12", "signal": null, "title": "logs", "active": false },
  ]});
  assert_eq!(
    ls_table(&out),
    "*1  pane:1   working  build\n 2  pane:12  -        logs\n"
  );
}
