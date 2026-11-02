use super::*;
use crate::keybind::parse_keybind;

fn binds(lines: &[&str]) -> Vec<Keybind> {
  lines.iter().map(|l| parse_keybind(l).unwrap()).collect()
}

fn keys(trigger: &str) -> Vec<(Mods, String)> {
  parse_keybind(&format!("{trigger}=new_tab"))
    .unwrap()
    .sequence()
}

#[test]
fn names() {
  assert!(valid_name("resize"));
  assert!(valid_name("vi-nav_2"));
  assert!(!valid_name(""));
  assert!(!valid_name("cmd+"));
  assert!(!valid_name("a b"));
}

#[test]
fn parse_declarations() {
  assert_eq!(
    parse_keymode("resize").unwrap(),
    KeyMode {
      name: "resize".into(),
      exclusive: false
    }
  );
  assert!(parse_keymode("Resize:exclusive").unwrap().exclusive);
  assert_eq!(parse_keymode("Resize:exclusive").unwrap().name, "resize");
  assert!(parse_keymode("resize:sticky").is_err());
  assert!(parse_keymode("re size").is_err());
}

#[test]
fn modes_from_declarations_and_bindings() {
  let b = binds(&["resize/left=resize_split:left", "nav/j=goto_split:down"]);
  let (modes, diags) = resolve_modes(&["resize:exclusive".into(), "bogus:nope".into()], &b);
  assert_eq!(diags.len(), 1);
  assert_eq!(
    modes,
    vec![
      KeyMode {
        name: "resize".into(),
        exclusive: true
      },
      KeyMode {
        name: "nav".into(),
        exclusive: false
      },
    ]
  );
}

#[test]
fn undefined_enter_targets() {
  let b = binds(&["ctrl+r=enter_mode:resze", "resize/left=resize_split:left"]);
  let (modes, _) = resolve_modes(&[], &b);
  assert_eq!(
    undefined_targets(&b, &modes),
    vec![(0, "resze".to_string())]
  );
}

#[test]
fn innermost_mode_wins() {
  let b = binds(&[
    "outer/h=goto_split:left",
    "outer/x=close_surface",
    "inner/h=resize_split:left",
  ]);
  let mut stack = ModeStack::default();
  assert_eq!(stack.lookup(&b, &keys("h")), None);
  stack.enter("outer", false);
  assert_eq!(stack.lookup(&b, &keys("h")), Some(0));
  stack.enter("inner", false);
  assert_eq!(stack.lookup(&b, &keys("h")), Some(2));
  // Falls through to the outer mode for keys the inner one doesn't bind.
  assert_eq!(stack.lookup(&b, &keys("x")), Some(1));
  assert_eq!(stack.lookup(&b, &keys("q")), None);
  assert_eq!(stack.exit().as_deref(), Some("inner"));
  assert_eq!(stack.lookup(&b, &keys("h")), Some(0));
}

#[test]
fn reentering_moves_to_top() {
  let mut stack = ModeStack::default();
  stack.enter("a", false);
  stack.enter("b", false);
  stack.enter("a", false);
  assert_eq!(stack.names().collect::<Vec<_>>(), vec!["b", "a"]);
  stack.exit_all();
  assert!(stack.is_empty());
}

#[test]
fn once_mode_leaves_after_a_bound_key() {
  let mut stack = ModeStack::default();
  stack.enter("sticky", false);
  stack.enter("tmux", true);
  stack.fired();
  assert_eq!(stack.innermost(), Some("sticky"));
  // A persistent mode survives its keys.
  stack.fired();
  assert_eq!(stack.innermost(), Some("sticky"));
}

#[test]
fn exclusive_follows_the_innermost_mode() {
  let modes = vec![
    KeyMode {
      name: "lock".into(),
      exclusive: true,
    },
    KeyMode {
      name: "open".into(),
      exclusive: false,
    },
  ];
  let mut stack = ModeStack::default();
  assert!(!stack.exclusive(&modes));
  stack.enter("lock", false);
  assert!(stack.exclusive(&modes));
  stack.enter("open", false);
  assert!(!stack.exclusive(&modes));
}
