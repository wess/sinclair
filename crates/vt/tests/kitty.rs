use super::*;

#[test]
fn empty_stack_is_legacy() {
    let k = KittyKeyboard::default();
    assert_eq!(k.current(), 0);
}

#[test]
fn push_and_pop() {
    let mut k = KittyKeyboard::default();
    k.push(flags::DISAMBIGUATE);
    assert_eq!(k.current(), 1);
    k.push(flags::DISAMBIGUATE | flags::REPORT_EVENT_TYPES);
    assert_eq!(k.current(), 3);
    k.pop(1);
    assert_eq!(k.current(), 1);
    k.pop(9); // clamps
    assert_eq!(k.current(), 0);
}

#[test]
fn set_modes() {
    let mut k = KittyKeyboard::default();
    k.set(0b0001, 1); // set
    assert_eq!(k.current(), 0b0001);
    k.set(0b0010, 2); // add
    assert_eq!(k.current(), 0b0011);
    k.set(0b0001, 3); // clear
    assert_eq!(k.current(), 0b0010);
}

#[test]
fn flags_are_masked_to_defined_bits() {
    let mut k = KittyKeyboard::default();
    k.push(0xff);
    assert_eq!(k.current(), flags::ALL);
}

#[test]
fn depth_is_bounded() {
    let mut k = KittyKeyboard::default();
    for _ in 0..(MAX_DEPTH + 5) {
        k.push(flags::DISAMBIGUATE);
    }
    // Still only MAX_DEPTH entries: popping that many empties it.
    k.pop(MAX_DEPTH);
    assert_eq!(k.current(), 0);
}
