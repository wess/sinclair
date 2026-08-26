use super::*;
use crate::options::Options;

#[test]
fn user_file_overrides_defaults() {
  let (opts, diags) = parse_json_str(
    "{\n  \"font-size\": 17,\n  \"copy-on-select\": true,\n  \"theme\": \"nord\"\n}",
  );
  assert!(diags.is_empty(), "{diags:?}");
  assert_eq!(opts.font_size, 17.0);
  assert!(opts.copy_on_select);
  assert_eq!(opts.theme, "nord");
  // Everything unset stays at its default.
  assert_eq!(opts.scrollback_limit, Options::default().scrollback_limit);
}

#[test]
fn comments_are_allowed() {
  let (opts, diags) =
    parse_json_str("// mine\n{\n  // bigger text\n  \"font-size\": 20, /* pt */\n}\n");
  assert!(diags.is_empty());
  assert_eq!(opts.font_size, 20.0);
}

#[test]
fn lists_accumulate() {
  let (opts, diags) = parse_json_str(
        "{ \"font-family\": [\"Hack\", \"Menlo\"], \"keybind\": [\"cmd+t=new_tab\"], \"palette\": [\"0=#101010\"] }",
    );
  assert!(diags.is_empty(), "{diags:?}");
  assert_eq!(opts.font_family, vec!["Hack", "Menlo"]);
  assert_eq!(opts.keybind, vec!["cmd+t=new_tab"]);
  assert_eq!(opts.palette, vec![(0, "#101010".to_string())]);
}

#[test]
fn single_string_is_a_one_entry_list() {
  let (opts, diags) = parse_json_str("{ \"font-family\": \"Hack\" }");
  assert!(diags.is_empty());
  assert_eq!(opts.font_family, vec!["Hack"]);
}

#[test]
fn invalid_values_fall_back_to_defaults_with_warnings() {
  let (opts, diags) =
    parse_json_str("{\n  \"font-size\": \"huge\",\n  \"copy-on-select\": [1],\n  \"bogus\": 1\n}");
  assert_eq!(opts.font_size, Options::default().font_size);
  assert!(!opts.copy_on_select);
  assert_eq!(diags.len(), 3);
  assert_eq!(diags[0].line, 2);
  assert_eq!(diags[1].key, "copy-on-select");
  assert!(diags[2].message.contains("unknown setting"));
}

#[test]
fn null_resets_to_default() {
  let (opts, diags) = parse_json_str("{ \"font-size\": null }");
  assert!(diags.is_empty());
  assert_eq!(opts.font_size, Options::default().font_size);
}

#[test]
fn syntax_errors_yield_defaults_plus_one_diagnostic() {
  let (opts, diags) = parse_json_str("{\n  \"font-size\": \n}");
  assert_eq!(opts, Options::default());
  assert_eq!(diags.len(), 1);
  assert_eq!(diags[0].line, 3);
}

#[test]
fn user_keys_lists_what_the_file_sets() {
  assert_eq!(
    user_keys("{ \"theme\": \"nord\", \"font-size\": 14 }"),
    vec!["theme".to_string(), "font-size".to_string()]
  );
  assert!(user_keys("garbage").is_empty());
}

#[test]
fn encode_uses_the_key_kind() {
  assert_eq!(encode("copy-on-select", "true"), "true");
  assert_eq!(encode("copy-on-select", "yes"), "true");
  assert_eq!(encode("font-size", "14"), "14");
  assert_eq!(encode("font-size", "big"), "\"big\"");
  assert_eq!(encode("theme", "nord"), "\"nord\"");
  assert_eq!(encode("redact", "sk-.*"), "[\"sk-.*\"]");
}

#[test]
fn encode_list_shapes() {
  assert_eq!(encode_list(&[]), "[]");
  let two = encode_list(&["a".to_string(), "b".to_string()]);
  let (opts, diags) = parse_json_str(&format!("{{ \"font-family\": {two} }}"));
  assert!(diags.is_empty());
  assert_eq!(opts.font_family, vec!["a", "b"]);
}

#[test]
fn migration_from_legacy() {
  let legacy = "# a comment\nfont-size = 14\ntheme = \"nord\"\nfont-size = 17\n\
                  keybind = cmd+t=new_tab\nkeybind = cmd+d=new_split:right\nbadge = \n";
  let json = from_legacy(legacy);
  let (opts, diags) = parse_json_str(&json);
  assert!(diags.is_empty(), "{diags:?}\n{json}");
  // Scalars keep the last occurrence; lists collect; empty values drop.
  assert_eq!(opts.font_size, 17.0);
  assert_eq!(opts.theme, "nord");
  assert_eq!(opts.keybind, vec!["cmd+t=new_tab", "cmd+d=new_split:right"]);
  assert_eq!(opts.badge, None);
}

#[test]
fn starter_parses_clean() {
  let (opts, diags) = parse_json_str(&starter());
  assert!(diags.is_empty());
  assert_eq!(opts, Options::default());
}
