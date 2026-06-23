use super::*;

#[test]
fn default_cell_is_blank() {
    let c = Cell::default();
    assert_eq!(c.ch, ' ');
    assert_eq!(c.fg, Color::Default);
    assert_eq!(c.bg, Color::Default);
    assert!(c.flags.is_empty());
}

#[test]
fn erased_keeps_pen_background_only() {
    let mut pen = Cell::default();
    pen.bg = Color::Indexed(4);
    pen.fg = Color::Indexed(1);
    pen.flags = CellFlags::BOLD | CellFlags::UNDERLINE;
    let e = Cell::erased(pen);
    assert_eq!(e.bg, Color::Indexed(4));
    assert_eq!(e.fg, Color::Default);
    assert_eq!(e.ch, ' ');
    assert!(e.flags.is_empty());
}

#[test]
fn any_underline_covers_all_styles() {
    assert!(CellFlags::ANY_UNDERLINE.contains(CellFlags::UNDERLINE));
    assert!(CellFlags::ANY_UNDERLINE.contains(CellFlags::DOUBLE_UNDERLINE));
    assert!(CellFlags::ANY_UNDERLINE.contains(CellFlags::CURLY_UNDERLINE));
    assert!(CellFlags::ANY_UNDERLINE.contains(CellFlags::DOTTED_UNDERLINE));
    assert!(CellFlags::ANY_UNDERLINE.contains(CellFlags::DASHED_UNDERLINE));
    assert!(!CellFlags::ANY_UNDERLINE.contains(CellFlags::STRIKETHROUGH));
}

#[test]
fn cell_is_small() {
    assert!(std::mem::size_of::<Cell>() <= 24);
}
