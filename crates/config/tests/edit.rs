use super::*;

#[test]
fn appends_when_absent() {
    assert_eq!(upsert("", "theme", "nord"), "theme = nord\n");
    assert_eq!(
        upsert("font-size = 14\n", "theme", "nord"),
        "font-size = 14\ntheme = nord\n"
    );
}

#[test]
fn replaces_in_place_preserving_rest() {
    let src = "# my config\nfont-size = 12\ntheme = dark\ncopy-on-select = true\n";
    let got = upsert(src, "theme", "nord");
    assert_eq!(
        got,
        "# my config\nfont-size = 12\ntheme = nord\ncopy-on-select = true\n"
    );
}

#[test]
fn ignores_commented_keys() {
    let src = "# theme = dark\nfont-size = 12\n";
    let got = upsert(src, "theme", "nord");
    assert_eq!(got, "# theme = dark\nfont-size = 12\ntheme = nord\n");
}

#[test]
fn only_replaces_first_occurrence() {
    let src = "theme = a\ntheme = b\n";
    assert_eq!(upsert(src, "theme", "c"), "theme = c\ntheme = b\n");
}

#[test]
fn set_list_appends_when_absent() {
    let got = set_list(
        "font-size = 14\n",
        "keybind",
        &["cmd+t=new_tab".into(), "cmd+w=close_surface".into()],
    );
    assert_eq!(
        got,
        "font-size = 14\nkeybind = cmd+t=new_tab\nkeybind = cmd+w=close_surface\n"
    );
}

#[test]
fn set_list_replaces_block_in_place() {
    let src = "# top\nplugin = a\nfont-size = 12\nplugin = b\nplugin = c\n";
    let got = set_list(src, "plugin", &["x".into(), "y".into()]);
    assert_eq!(got, "# top\nplugin = x\nplugin = y\nfont-size = 12\n");
}

#[test]
fn set_list_empty_removes_every_entry() {
    let src = "keybind = cmd+t=new_tab\nfont-size = 12\nkeybind = cmd+w=quit\n";
    assert_eq!(set_list(src, "keybind", &[]), "font-size = 12\n");
}

#[test]
fn round_trips_through_parser() {
    let text = upsert(&upsert("", "theme", "nord"), "font-size", "16");
    let (opts, diags) = crate::parse_str(&text);
    assert!(diags.is_empty(), "{diags:?}");
    assert_eq!(opts.theme, "nord");
    assert_eq!(opts.font_size, 16.0);
}
