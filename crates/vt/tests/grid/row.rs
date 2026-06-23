use super::*;
use crate::cell::CellFlags;
use crate::color::Color;

#[test]
fn new_row_is_blank() {
    let r = Row::new(5);
    assert_eq!(r.len(), 5);
    assert_eq!(r.text(), "");
    assert!(!r.wrapped);
}

#[test]
fn fill_resets_wrap() {
    let mut r = Row::new(3);
    r.wrapped = true;
    r.fill(Cell::default());
    assert!(!r.wrapped);
}

#[test]
fn resize_pads_and_truncates() {
    let mut r = Row::new(3);
    let mut blank = Cell::default();
    blank.bg = Color::Indexed(2);
    r.resize(6, blank);
    assert_eq!(r.len(), 6);
    assert_eq!(r.cells[5].bg, Color::Indexed(2));
    r.resize(2, blank);
    assert_eq!(r.len(), 2);
}

#[test]
fn text_skips_wide_spacers() {
    let mut r = Row::new(4);
    r.cells[0].ch = '漢';
    r.cells[0].flags.insert(CellFlags::WIDE);
    r.cells[1].flags.insert(CellFlags::WIDE_SPACER);
    r.cells[2].ch = 'x';
    assert_eq!(r.text(), "漢x");
}
