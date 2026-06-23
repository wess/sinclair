use super::*;

fn mods(ctrl: bool, shift: bool, alt: bool, cmd: bool) -> Mods {
    Mods {
        ctrl,
        shift,
        alt,
        cmd,
    }
}

fn trigger(s: &str) -> (Mods, String) {
    parse_trigger(s).unwrap_or_else(|e| panic!("{s}: {e}"))
}

#[test]
fn plain_key() {
    assert_eq!(trigger("a"), (Mods::default(), "a".to_string()));
    assert_eq!(trigger("enter"), (Mods::default(), "enter".to_string()));
}

#[test]
fn modifier_order_does_not_matter() {
    let want = (mods(true, true, false, false), "a".to_string());
    assert_eq!(trigger("ctrl+shift+a"), want);
    assert_eq!(trigger("shift+ctrl+a"), want);
}

#[test]
fn modifier_aliases() {
    let alt = (mods(false, false, true, false), "x".to_string());
    assert_eq!(trigger("alt+x"), alt);
    assert_eq!(trigger("opt+x"), alt);
    assert_eq!(trigger("option+x"), alt);
    let cmd = (mods(false, false, false, true), "x".to_string());
    assert_eq!(trigger("cmd+x"), cmd);
    assert_eq!(trigger("command+x"), cmd);
    assert_eq!(trigger("super+x"), cmd);
    assert_eq!(trigger("control+x"), trigger("ctrl+x"));
}

#[test]
fn modifiers_are_case_insensitive() {
    assert_eq!(trigger("Ctrl+Shift+A"), trigger("ctrl+shift+a"));
}

#[test]
fn named_keys() {
    for name in [
        "enter",
        "tab",
        "escape",
        "backspace",
        "delete",
        "up",
        "down",
        "left",
        "right",
        "home",
        "end",
        "page_up",
        "page_down",
        "space",
        "insert",
        "f1",
        "f12",
    ] {
        assert_eq!(trigger(&format!("ctrl+{name}")).1, name, "{name}");
    }
}

#[test]
fn punctuation_key_names_map_to_chars() {
    assert_eq!(trigger("cmd+plus").1, "+");
    assert_eq!(trigger("cmd+minus").1, "-");
    assert_eq!(trigger("cmd+equal").1, "=");
    assert_eq!(trigger("cmd+comma").1, ",");
    assert_eq!(trigger("cmd+bracket_left").1, "[");
    assert_eq!(trigger("cmd+bracket_right").1, "]");
}

#[test]
fn trailing_plus_is_the_plus_key() {
    assert_eq!(
        trigger("cmd++"),
        (mods(false, false, false, true), "+".to_string())
    );
    assert_eq!(trigger("+"), (Mods::default(), "+".to_string()));
}

#[test]
fn single_chars_lowercase() {
    assert_eq!(trigger("cmd+A").1, "a");
    assert_eq!(trigger("cmd+[").1, "[");
    assert_eq!(trigger("cmd+9").1, "9");
}

#[test]
fn bad_triggers() {
    assert!(parse_trigger("").is_err());
    assert!(parse_trigger("cmd+").is_err());
    assert!(parse_trigger("hyper+a").is_err());
    assert!(parse_trigger("cmd+foo").is_err());
}

#[test]
fn parse_keybind_full() {
    let kb = parse_keybind("ctrl+shift+c=copy_to_clipboard").unwrap();
    assert_eq!(kb.mods, mods(true, true, false, false));
    assert_eq!(kb.key, "c");
    assert_eq!(kb.action, Action::Copy);

    let kb = parse_keybind("cmd+shift+d=new_split:down").unwrap();
    assert_eq!(kb.mods, mods(false, true, false, true));
    assert_eq!(kb.key, "d");
    assert_eq!(kb.action, Action::NewSplit(SplitDirection::Down));

    let kb = parse_keybind(" cmd+9 = goto_tab:-1 ").unwrap();
    assert_eq!(kb.action, Action::GotoTab(-1));
}

#[test]
fn parse_keybind_errors() {
    assert!(parse_keybind("no equals here").is_err());
    assert!(parse_keybind("cmd+t=do_a_flip").is_err());
    assert!(parse_keybind("cmd+huh=new_tab").is_err());
    // Binding `=` needs the `equal` name; `cmd+=` has no key.
    assert!(parse_keybind("cmd+=copy").is_err());
    assert!(parse_keybind("cmd+equal=copy").is_ok());
}

#[test]
fn defaults_cover_the_hardcoded_set() {
    let binds = default_keybinds();
    let cmd = mods(false, false, false, true);
    let cmd_shift = mods(false, true, false, true);
    let cmd_alt = mods(false, false, true, true);
    let cmd_alt_shift = mods(false, true, true, true);
    let find = |m: Mods, k: &str| {
        binds
            .iter()
            .find(|b| b.mods == m && b.key == k)
            .unwrap_or_else(|| panic!("missing {m:?}+{k}"))
            .action
            .clone()
    };
    assert_eq!(find(cmd, "n"), Action::NewWindow);
    assert_eq!(find(cmd, "t"), Action::NewTab);
    assert_eq!(find(cmd, "w"), Action::CloseSurface);
    assert_eq!(find(cmd_alt, "w"), Action::CloseTab);
    assert_eq!(find(cmd_shift, "w"), Action::CloseWindow);
    assert_eq!(find(cmd_alt_shift, "w"), Action::CloseAllWindows);
    assert_eq!(find(cmd, "d"), Action::NewSplit(SplitDirection::Right));
    assert_eq!(find(cmd_shift, "d"), Action::NewSplit(SplitDirection::Down));
    assert_eq!(find(cmd_shift, "["), Action::PreviousTab);
    assert_eq!(find(cmd_shift, "]"), Action::NextTab);
    assert_eq!(find(cmd_alt, "up"), Action::GotoSplit(SplitFocus::Up));
    assert_eq!(find(cmd_alt, "down"), Action::GotoSplit(SplitFocus::Down));
    assert_eq!(find(cmd_alt, "left"), Action::GotoSplit(SplitFocus::Left));
    assert_eq!(find(cmd_alt, "right"), Action::GotoSplit(SplitFocus::Right));
    for n in 1..=9 {
        assert_eq!(find(cmd, &n.to_string()), Action::GotoTab(n));
    }
    assert_eq!(find(cmd, "c"), Action::Copy);
    assert_eq!(find(cmd, "v"), Action::Paste);
    assert_eq!(find(cmd, "+"), Action::IncreaseFontSize(1.0));
    assert_eq!(find(cmd, "="), Action::IncreaseFontSize(1.0));
    assert_eq!(find(cmd, "-"), Action::DecreaseFontSize(1.0));
    assert_eq!(find(cmd, "0"), Action::ResetFontSize);
    assert_eq!(find(cmd, "k"), Action::ClearScreen);
    assert_eq!(find(cmd, ","), Action::ToggleSettings);
    assert_eq!(find(cmd_shift, ","), Action::ReloadConfig);
    assert_eq!(find(cmd, "q"), Action::Quit);
    // No duplicate triggers among defaults.
    for (i, a) in binds.iter().enumerate() {
        for b in &binds[i + 1..] {
            assert!(
                !(a.mods == b.mods && a.key == b.key),
                "duplicate default trigger {a:?}"
            );
        }
    }
}

#[test]
fn resolve_with_no_user_binds_is_defaults() {
    let (binds, diags) = resolve(&[]);
    assert_eq!(binds, default_keybinds());
    assert!(diags.is_empty());
}

#[test]
fn user_bind_overrides_default_with_same_trigger() {
    let raw = vec!["cmd+t=quit".to_string()];
    let (binds, diags) = resolve(&raw);
    assert!(diags.is_empty());
    assert_eq!(binds.len(), default_keybinds().len());
    let cmd_t: Vec<_> = binds
        .iter()
        .filter(|b| b.mods == mods(false, false, false, true) && b.key == "t")
        .collect();
    assert_eq!(cmd_t.len(), 1);
    assert_eq!(cmd_t[0].action, Action::Quit);
}

#[test]
fn later_user_bind_wins_over_earlier() {
    let raw = vec!["ctrl+x=new_tab".to_string(), "ctrl+x=quit".to_string()];
    let (binds, _) = resolve(&raw);
    let hits: Vec<_> = binds
        .iter()
        .filter(|b| b.mods == mods(true, false, false, false) && b.key == "x")
        .collect();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].action, Action::Quit);
}

#[test]
fn new_trigger_is_appended() {
    let raw = vec!["ctrl+shift+page_up=scroll_page_up".to_string()];
    let (binds, diags) = resolve(&raw);
    assert!(diags.is_empty());
    assert_eq!(binds.len(), default_keybinds().len() + 1);
    let kb = binds.last().unwrap();
    assert_eq!(kb.mods, mods(true, true, false, false));
    assert_eq!(kb.key, "page_up");
    assert_eq!(kb.action, Action::ScrollPageUp);
}

#[test]
fn unbind_removes_a_default() {
    let raw = vec!["cmd+q=unbind".to_string()];
    let (binds, diags) = resolve(&raw);
    assert!(diags.is_empty());
    assert_eq!(binds.len(), default_keybinds().len() - 1);
    assert!(!binds
        .iter()
        .any(|b| b.mods == mods(false, false, false, true) && b.key == "q"));
}

#[test]
fn unbind_unknown_trigger_is_harmless() {
    let raw = vec!["ctrl+alt+f9=unbind".to_string()];
    let (binds, diags) = resolve(&raw);
    assert!(diags.is_empty());
    assert_eq!(binds, default_keybinds());
}

#[test]
fn invalid_entries_diagnose_and_skip() {
    let raw = vec![
        "cmd+t=do_a_flip".to_string(),
        "garbage".to_string(),
        "ctrl+x=new_tab".to_string(),
    ];
    let (binds, diags) = resolve(&raw);
    assert_eq!(diags.len(), 2);
    assert!(diags.iter().all(|d| d.key == "keybind"));
    assert!(diags[0].message.contains("do_a_flip"));
    // The bad cmd+t entry must not have removed the default.
    assert!(binds
        .iter()
        .any(|b| b.key == "t" && b.action == Action::NewTab));
    assert_eq!(binds.len(), default_keybinds().len() + 1);
}

#[test]
fn format_trigger_round_trips() {
    let cmd_shift = mods(false, true, false, true);
    assert_eq!(format_trigger(cmd_shift, "t"), "cmd+shift+t");
    assert_eq!(parse_trigger("cmd+shift+t").unwrap(), (cmd_shift, "t".into()));
    // Punctuation keys spell out so the line never mis-splits.
    assert_eq!(format_trigger(mods(false, false, false, true), "+"), "cmd+plus");
    assert_eq!(format_trigger(mods(false, false, false, true), "="), "cmd+equal");
    assert_eq!(format_trigger(mods(false, false, false, true), "["), "cmd+bracket_left");
    for key in ["plus", "equal", "bracket_left", "comma"] {
        let (m, k) = parse_trigger(&format!("cmd+{key}")).unwrap();
        assert_eq!(format_trigger(m, &k), format!("cmd+{key}"));
    }
}

#[test]
fn diff_round_trips_through_resolve() {
    // Start from defaults, change one action, drop one, add one new.
    let mut desired = default_keybinds();
    for kb in &mut desired {
        if kb.mods == mods(false, false, false, true) && kb.key == "t" {
            kb.action = Action::Quit; // change cmd+t
        }
    }
    desired.retain(|kb| !(kb.mods == mods(false, false, false, true) && kb.key == "q")); // drop cmd+q
    desired.push(Keybind {
        mods: mods(true, true, false, false),
        key: "page_up".into(),
        action: Action::ScrollPageUp,
    }); // add new

    let lines = diff_from_defaults(&desired);
    let (resolved, diags) = resolve(&lines);
    assert!(diags.is_empty(), "{diags:?}");

    let key = |b: &Keybind| (b.mods, b.key.clone());
    let mut got: Vec<_> = resolved.iter().map(|b| (key(b), b.action.clone())).collect();
    let mut want: Vec<_> = desired.iter().map(|b| (key(b), b.action.clone())).collect();
    got.sort_by(|a, b| format!("{a:?}").cmp(&format!("{b:?}")));
    want.sort_by(|a, b| format!("{a:?}").cmp(&format!("{b:?}")));
    assert_eq!(got, want);
}

#[test]
fn diff_of_defaults_is_empty() {
    assert!(diff_from_defaults(&default_keybinds()).is_empty());
}

#[test]
fn equivalent_triggers_in_different_spellings_collide() {
    let raw = vec![
        "super+t=quit".to_string(), // same trigger as default cmd+t
    ];
    let (binds, _) = resolve(&raw);
    let hits: Vec<_> = binds
        .iter()
        .filter(|b| b.mods == mods(false, false, false, true) && b.key == "t")
        .collect();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].action, Action::Quit);
}
