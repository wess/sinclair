use super::*;

/// `ItemId` is opaque to this crate, so tests build them the one way a host
/// can: off an allocator, in order.
fn ids(n: usize) -> Vec<ItemId> {
    let mut alloc = guise::panegroup::ItemIds::new();
    (0..n).map(|_| alloc.next()).collect()
}

fn ctrl() -> Modifiers {
    Modifiers {
        control: true,
        ..Default::default()
    }
}

#[test]
fn opens_pointing_at_the_previous_item() {
    let order = ids(3);
    let s = Switcher::open(order.clone(), 1, ctrl()).unwrap();
    // Index 0 is the item you are on; a switcher that opened there would make
    // the common tap-and-release a no-op.
    assert_eq!(s.selected(), order[1]);
}

#[test]
fn a_lone_item_has_nothing_to_switch_to() {
    assert!(Switcher::open(ids(1), 1, ctrl()).is_none());
    assert!(Switcher::open(Vec::new(), 1, ctrl()).is_none());
}

#[test]
fn advancing_wraps_both_ways() {
    let order = ids(3);
    let mut s = Switcher::open(order.clone(), 1, ctrl()).unwrap();
    s.advance(1);
    assert_eq!(s.selected(), order[2]);
    s.advance(1);
    assert_eq!(s.selected(), order[0]);
    s.advance(-1);
    assert_eq!(s.selected(), order[2]);
}

#[test]
fn opening_backwards_lands_on_the_last_item() {
    let order = ids(4);
    let s = Switcher::open(order.clone(), -1, ctrl()).unwrap();
    assert_eq!(s.selected(), order[3]);
}

#[test]
fn holding_ends_when_the_gesture_modifier_lifts() {
    let s = Switcher::open(ids(2), 1, ctrl()).unwrap();
    assert!(s.held());
    assert!(s.still_held(ctrl()));
    assert!(!s.still_held(Modifiers::default()));
}

#[test]
fn shift_may_come_and_go_mid_cycle() {
    // Backwards is shift + the same binding, so shift lifting is a direction
    // change. Only the modifier that opened the switcher can close it.
    let s = Switcher::open(ids(2), 1, ctrl()).unwrap();
    let both = Modifiers {
        control: true,
        shift: true,
        ..Default::default()
    };
    assert!(s.still_held(both));
    assert!(s.still_held(ctrl()));
}

#[test]
fn a_switcher_nothing_holds_open_stays_up() {
    // Opened from the palette: there is no key down to wait on, so it must not
    // commit itself on the next stray modifier event.
    let s = Switcher::open(ids(2), 1, Modifiers::default()).unwrap();
    assert!(!s.held());
    assert!(s.still_held(Modifiers::default()));
}
