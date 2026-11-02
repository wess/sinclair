use super::*;
use crate::settings::parse_json_str;

const TEXT: &str = r#"{
  // mine
  "keybind": [
    "cmd+t=new_window",
    "resize/left=resize_split:left",
    "cmd+j=do_a_flip",
    "ctrl+r=enter_mode:resze"
  ],
  "keymode": ["resize:exclusive", "x:wat"]
}
"#;

fn map() -> Keymap {
  let (opts, _) = parse_json_str(TEXT);
  build(
    &opts,
    Some(TEXT),
    &["cmd+shift+z=plugin_command:demo/run".into()],
  )
}

#[test]
fn line_numbers_for_entries() {
  let (opts, _) = parse_json_str(TEXT);
  assert_eq!(
    entry_lines(TEXT, "keybind", &opts.keybind),
    vec![4, 5, 6, 7]
  );
  assert_eq!(entry_lines(TEXT, "keymode", &opts.keymode), vec![9, 9]);
  assert_eq!(entry_lines("{}", "keybind", &opts.keybind), vec![0; 4]);
}

#[test]
fn sources_and_overrides() {
  let m = map();
  let find = |trigger: &str, mode: Option<&str>| {
    m.bindings
      .iter()
      .find(|b| b.trigger == trigger && b.mode.as_deref() == mode)
      .cloned()
      .unwrap_or_else(|| panic!("no {trigger}"))
  };
  let t = find("cmd+t", None);
  assert_eq!(t.action, "new_window");
  assert_eq!(t.source, Source::Settings(4));
  assert_eq!(find("cmd+n", None).source, Source::Default);
  assert_eq!(find("cmd+shift+z", None).source, Source::Plugin);
  let left = find("left", Some("resize"));
  assert_eq!(left.action, "resize_split:left");
  // Mode bindings sort after the always-on ones.
  assert_eq!(m.bindings.last().unwrap().mode.as_deref(), Some("resize"));
}

#[test]
fn problems_are_reported_with_lines() {
  let m = map();
  let msgs: Vec<String> = m
    .problems
    .iter()
    .map(|p| format!("{} {}", p.source.label(), p.message))
    .collect();
  assert_eq!(m.problems.len(), 3, "{msgs:?}");
  assert!(
    msgs[0].starts_with("settings.json:6 unknown action"),
    "{msgs:?}"
  );
  assert!(
    msgs[1].starts_with("settings.json:9 unknown mode option"),
    "{msgs:?}"
  );
  assert!(msgs[2].contains("resze"), "{msgs:?}");
}

#[test]
fn modes_listed() {
  let m = map();
  assert_eq!(m.modes.len(), 1);
  assert!(m.modes[0].exclusive);
}

#[test]
fn text_and_json_render() {
  let m = map();
  let text = render_text(&m);
  assert!(text.starts_with("TRIGGER"));
  assert!(text.contains("resize (exclusive)"));
  assert!(text.contains("settings.json:6: `cmd+j=do_a_flip`"));
  let json = render_json(&m);
  let parsed = crate::json::parse(&json).expect("valid json");
  let crate::json::Value::Obj(members) = parsed else {
    panic!("not an object");
  };
  assert_eq!(members.len(), 3);
}
