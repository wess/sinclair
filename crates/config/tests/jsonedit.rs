use super::*;

const FILE: &str = "// Sinclair settings.\n{\n  // how big\n  \"font-size\": 14,\n  \"theme\": \"nord\", // current\n  \"keybind\": [\n    \"cmd+t=new_tab\",\n  ],\n}\n";

#[test]
fn upsert_replaces_in_place() {
    let out = upsert(FILE, "font-size", "17").unwrap();
    assert!(out.contains("\"font-size\": 17,"));
    // Comments and the other members survive untouched.
    assert!(out.contains("// how big"));
    assert!(out.contains("\"theme\": \"nord\", // current"));
    assert!(out.contains("cmd+t=new_tab"));
}

#[test]
fn upsert_replaces_arrays() {
    let out = upsert(FILE, "keybind", "[\"cmd+d=new_split:right\"]").unwrap();
    assert!(out.contains("\"keybind\": [\"cmd+d=new_split:right\"],"));
    assert!(!out.contains("cmd+t=new_tab"));
    assert!(out.contains("// how big"));
}

#[test]
fn upsert_appends_missing_key() {
    let out = upsert(FILE, "copy-on-select", "true").unwrap();
    assert!(out.contains("  \"copy-on-select\": true\n"));
    // Still parses, with every key present.
    let keys = crate::settings::user_keys(&out);
    assert!(keys.contains(&"copy-on-select".to_string()));
    assert_eq!(keys.len(), 4);
}

#[test]
fn upsert_into_empty_object() {
    let out = upsert("{\n}\n", "theme", "\"nord\"").unwrap();
    assert_eq!(crate::settings::user_keys(&out), vec!["theme".to_string()]);
    let again = upsert(&out, "font-size", "12").unwrap();
    assert_eq!(crate::settings::user_keys(&again).len(), 2);
}

#[test]
fn upsert_after_member_without_trailing_comma() {
    let text = "{\n  \"theme\": \"nord\"\n}\n";
    let out = upsert(text, "font-size", "12").unwrap();
    assert_eq!(crate::settings::user_keys(&out).len(), 2);
}

#[test]
fn remove_deletes_member_and_separator() {
    let out = remove(FILE, "theme").unwrap();
    assert!(!out.contains("theme"));
    assert_eq!(crate::settings::user_keys(&out).len(), 2);
}

#[test]
fn remove_last_member_without_trailing_comma() {
    let text = "{\n  \"font-size\": 14,\n  \"theme\": \"nord\"\n}\n";
    let out = remove(text, "theme").unwrap();
    assert!(!out.contains("theme"));
    assert_eq!(
        crate::settings::user_keys(&out),
        vec!["font-size".to_string()]
    );
}

#[test]
fn remove_absent_key_is_a_noop() {
    assert_eq!(remove(FILE, "nope").unwrap(), FILE);
}

#[test]
fn refuses_non_object_text() {
    assert!(upsert("not json", "theme", "\"nord\"").is_none());
    assert!(remove("[1, 2]", "theme").is_none());
}
