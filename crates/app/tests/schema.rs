use super::*;

#[test]
fn every_entry_matches_the_config_kind_table() {
    for s in all() {
        let kind = config::kind(s.key);
        assert!(kind.is_some(), "`{}` is not a known settings key", s.key);
        let expected = match s.control {
            Control::Toggle(_) => config::Kind::Bool,
            Control::Slider(n) => {
                if n.int {
                    config::Kind::Int
                } else {
                    config::Kind::Float
                }
            }
            Control::Choice(_) | Control::Text { .. } => config::Kind::Str,
            Control::List(_) => config::Kind::List,
        };
        assert_eq!(
            kind,
            Some(expected),
            "`{}` control disagrees with its kind",
            s.key
        );
    }
}

#[test]
fn keys_are_unique() {
    let entries = all();
    for (i, a) in entries.iter().enumerate() {
        assert!(
            !entries[i + 1..].iter().any(|b| a.key == b.key),
            "duplicate schema entry `{}`",
            a.key
        );
    }
}

#[test]
fn entries_carry_descriptions() {
    for s in all() {
        assert!(!s.label.is_empty(), "`{}` has no label", s.key);
        assert!(!s.desc.is_empty(), "`{}` has no description", s.key);
    }
}

#[test]
fn find_looks_up_by_key() {
    assert!(find("font-size").is_some());
    assert!(find("bogus").is_none());
}

#[test]
fn search_matches_key_label_and_description() {
    let s = find("copy-on-select").unwrap();
    assert!(s.matches("copy"));
    assert!(s.matches("Clipboard"));
    assert!(s.matches("copy-on-select"));
    assert!(s.matches(""));
    assert!(!s.matches("font"));
}

#[test]
fn tool_keys_exist_in_the_schema() {
    for &key in TOOL_KEYS {
        assert!(find(key).is_some(), "tool key `{key}` missing from schema");
    }
}

#[test]
fn list_kinds_round_trip_values() {
    let opts = config::Options {
        font_family: vec!["Hack".to_string()],
        ..Default::default()
    };
    assert_eq!(ListKind::FontFamily.values(&opts), vec!["Hack"]);
    assert_eq!(
        ListKind::FontFamily.to_values(&[" Hack ".to_string(), String::new()]),
        vec!["Hack"]
    );
    // Keybinds collapse to a diff against the defaults: re-adding only
    // defaults yields no overrides at all.
    let defaults: Vec<String> = config::default_keybinds()
        .iter()
        .map(|kb| kb.config_line())
        .collect();
    assert!(ListKind::Keybind.to_values(&defaults).is_empty());
}
