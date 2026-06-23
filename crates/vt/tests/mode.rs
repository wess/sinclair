use super::*;

#[test]
fn defaults_have_autowrap_and_visible_cursor() {
    let m = Modes::default();
    assert!(m.contains(Modes::AUTOWRAP));
    assert!(m.contains(Modes::CURSOR_VISIBLE));
    assert!(!m.contains(Modes::ORIGIN));
    assert!(!m.contains(Modes::ALT_SCREEN));
    assert!(!m.contains(Modes::INSERT));
}

#[test]
fn set_and_clear() {
    let mut m = Modes::default();
    m.insert(Modes::BRACKETED_PASTE);
    assert!(m.contains(Modes::BRACKETED_PASTE));
    m.remove(Modes::BRACKETED_PASTE);
    assert!(!m.contains(Modes::BRACKETED_PASTE));
}
