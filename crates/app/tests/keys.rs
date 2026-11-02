use super::*;

fn mods(ctrl: bool, shift: bool, alt: bool, cmd: bool) -> config::Mods {
  config::Mods {
    ctrl,
    shift,
    alt,
    cmd,
  }
}

#[test]
fn plain_and_modified_keys() {
  assert_eq!(
    keystroke(mods(false, false, false, true), "t").unwrap(),
    "secondary-t"
  );
  assert_eq!(
    keystroke(mods(false, true, false, true), "d").unwrap(),
    "shift-secondary-d"
  );
  assert_eq!(
    keystroke(mods(true, false, true, false), "x").unwrap(),
    "ctrl-alt-x"
  );
  assert_eq!(keystroke(config::Mods::default(), "a").unwrap(), "a");
}

#[test]
fn modifier_order_is_canonical() {
  // ctrl, alt, shift, cmd(secondary) regardless of how many are set.
  assert_eq!(
    keystroke(mods(true, true, true, true), "k").unwrap(),
    "ctrl-alt-shift-secondary-k"
  );
}

#[test]
fn paged_keys_are_renamed() {
  assert_eq!(
    keystroke(mods(false, true, false, false), "page_up").unwrap(),
    "shift-pageup"
  );
  assert_eq!(
    keystroke(mods(false, true, false, false), "page_down").unwrap(),
    "shift-pagedown"
  );
}

#[test]
fn punctuation_keys_pass_through() {
  // The minus key with cmd renders as `secondary--`, which gpui parses
  // as the platform modifier + the `-` key.
  assert_eq!(
    keystroke(mods(false, false, false, true), "-").unwrap(),
    "secondary--"
  );
  assert_eq!(
    keystroke(mods(false, false, false, true), "+").unwrap(),
    "secondary-+"
  );
  assert_eq!(
    keystroke(mods(false, false, false, true), "=").unwrap(),
    "secondary-="
  );
  assert_eq!(
    keystroke(mods(false, false, false, true), ",").unwrap(),
    "secondary-,"
  );
}

#[test]
fn named_keys_pass_through() {
  for k in ["enter", "tab", "escape", "space", "up", "home", "f5"] {
    assert_eq!(keystroke(config::Mods::default(), k).unwrap(), k);
  }
}

#[test]
fn every_emitted_default_binding_parses_in_gpui() {
  // The whole default set must produce gpui-parseable keystrokes,
  // so binding them at startup never panics.
  for kb in config::default_keybinds() {
    let ks = keystroke(kb.mods, &kb.key)
      .unwrap_or_else(|| panic!("no keystroke for {:?}+{}", kb.mods, kb.key));
    gpui::Keystroke::parse(&ks).unwrap_or_else(|e| panic!("gpui rejected {ks:?}: {e:?}"));
  }
}

#[test]
fn pressed_keystroke_to_trigger() {
  let ks = |s: &str| gpui::Keystroke::parse(s).unwrap();
  assert_eq!(
    trigger_of(&ks("cmd-shift-k")),
    Some((mods(false, true, false, true), "k".to_string()))
  );
  assert_eq!(
    trigger_of(&ks("ctrl-pageup")),
    Some((mods(true, false, false, false), "page_up".to_string()))
  );
  // Only a modifier held: still waiting for the real key.
  let bare = gpui::Keystroke {
    modifiers: gpui::Modifiers {
      platform: true,
      ..Default::default()
    },
    key: "cmd".into(),
    key_char: None,
  };
  assert_eq!(trigger_of(&bare), None);
}

#[test]
fn bare_keys_would_steal_typing() {
  let none = config::Mods::default();
  let shift = mods(false, true, false, false);
  assert!(steals_typing(none, "a"));
  assert!(steals_typing(shift, "a"));
  assert!(steals_typing(none, "enter"));
  assert!(!steals_typing(none, "f5"));
  assert!(!steals_typing(mods(true, false, false, false), "a"));
  assert!(!steals_typing(mods(false, false, false, true), "enter"));
}
