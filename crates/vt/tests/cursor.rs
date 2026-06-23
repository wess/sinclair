use super::*;

#[test]
fn default_cursor_is_home() {
    let c = Cursor::default();
    assert_eq!((c.row, c.col), (0, 0));
    assert!(!c.pending_wrap);
}

#[test]
fn decscusr_mapping() {
    assert_eq!(
        CursorStyle::from_decscusr(0),
        Some(CursorStyle::BlinkingBlock)
    );
    assert_eq!(
        CursorStyle::from_decscusr(1),
        Some(CursorStyle::BlinkingBlock)
    );
    assert_eq!(
        CursorStyle::from_decscusr(2),
        Some(CursorStyle::SteadyBlock)
    );
    assert_eq!(
        CursorStyle::from_decscusr(3),
        Some(CursorStyle::BlinkingUnderline)
    );
    assert_eq!(
        CursorStyle::from_decscusr(4),
        Some(CursorStyle::SteadyUnderline)
    );
    assert_eq!(
        CursorStyle::from_decscusr(5),
        Some(CursorStyle::BlinkingBar)
    );
    assert_eq!(CursorStyle::from_decscusr(6), Some(CursorStyle::SteadyBar));
    assert_eq!(CursorStyle::from_decscusr(7), None);
}

#[test]
fn saved_cursor_default_is_home() {
    let s = SavedCursor::default();
    assert_eq!((s.row, s.col), (0, 0));
    assert!(!s.origin);
}
