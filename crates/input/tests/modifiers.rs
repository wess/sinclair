use super::*;

const NONE: Mods = Mods {
  shift: false,
  alt: false,
  ctrl: false,
  cmd: false,
  hyper: false,
  meta: false,
  caps_lock: false,
  num_lock: false,
};

#[test]
fn no_change_reports_nothing() {
  assert!(modifier_events(NONE, NONE).is_empty());
  let held = Mods { shift: true, ..NONE };
  assert!(modifier_events(held, held).is_empty());
}

#[test]
fn holding_and_letting_go_are_a_press_and_a_release() {
  let shift = Mods { shift: true, ..NONE };
  assert_eq!(
    modifier_events(NONE, shift),
    vec![("left_shift", KeyEvent::Press)]
  );
  assert_eq!(
    modifier_events(shift, NONE),
    vec![("left_shift", KeyEvent::Release)]
  );
}

#[test]
fn swapping_one_modifier_for_another_releases_before_it_presses() {
  let shift = Mods { shift: true, ..NONE };
  let ctrl = Mods { ctrl: true, ..NONE };
  assert_eq!(
    modifier_events(shift, ctrl),
    vec![
      ("left_shift", KeyEvent::Release),
      ("left_control", KeyEvent::Press),
    ]
  );
}

#[test]
fn every_modifier_is_covered() {
  let all = Mods {
    shift: true,
    alt: true,
    ctrl: true,
    cmd: true,
    hyper: true,
    meta: true,
    caps_lock: false,
    num_lock: false,
  };
  let events = modifier_events(NONE, all);
  assert_eq!(events.len(), 6);
  assert!(events.iter().all(|(_, e)| *e == KeyEvent::Press));
  assert!(events.iter().any(|(k, _)| *k == "left_meta"));
}

#[test]
fn a_lock_toggle_is_a_whole_keystroke() {
  // The platform reports the lock's state, so the strike that changed it is
  // reported as the press and release it was — in both directions.
  let caps = Mods {
    caps_lock: true,
    ..NONE
  };
  assert_eq!(
    modifier_events(NONE, caps),
    vec![
      ("capslock", KeyEvent::Press),
      ("capslock", KeyEvent::Release),
    ]
  );
  assert_eq!(
    modifier_events(caps, NONE),
    vec![
      ("capslock", KeyEvent::Press),
      ("capslock", KeyEvent::Release),
    ]
  );
}
