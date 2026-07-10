use super::*;

use crate::cell::{Cell, CellFlags};
use crate::color::Color;
use crate::hyperlink::HyperlinkId;

fn cell(ch: char) -> Cell {
    Cell {
        ch,
        ..Cell::default()
    }
}

/// A row exercising every encoded field: colors, flags, wide pair,
/// combining mark, hyperlink, and trailing blanks.
fn busy_row(cols: usize) -> Row {
    let mut row = Row::new(cols);
    row.wrapped = true;
    row.prompt = true;
    row.cells[0] = Cell {
        ch: 'x',
        fg: Color::Rgb(1, 2, 3),
        bg: Color::Indexed(42),
        underline_color: Color::Rgb(200, 100, 0),
        flags: CellFlags::BOLD | CellFlags::CURLY_UNDERLINE | CellFlags::INVERSE,
        hyperlink: HyperlinkId::from_u16(7),
        zw: '\u{0301}',
    };
    row.cells[1] = Cell {
        ch: '漢',
        flags: CellFlags::WIDE,
        ..Cell::default()
    };
    row.cells[2] = Cell {
        flags: CellFlags::WIDE_SPACER,
        ..Cell::default()
    };
    row.cells[3] = cell('z');
    row
}

#[test]
fn round_trips_every_field() {
    let rows = vec![busy_row(10), Row::new(10), busy_row(10)];
    let block = encode(&rows, 10);
    assert_eq!(decode(&block, 10), rows);
}

#[test]
fn trailing_blanks_trim_and_restore() {
    let mut row = Row::new(80);
    row.cells[0] = cell('a');
    row.cells[1] = cell('b');
    let block = encode(std::slice::from_ref(&row), 80);
    let out = decode(&block, 80);
    assert_eq!(out[0], row);
    assert_eq!(out[0].len(), 80);
}

#[test]
fn fully_blank_row_survives() {
    let rows = vec![Row::new(5)];
    assert_eq!(decode(&encode(&rows, 5), 5), rows);
}

#[test]
fn flag_only_row_keeps_flags() {
    let mut row = Row::new(4);
    row.wrapped = true;
    let out = decode(&encode(std::slice::from_ref(&row), 4), 4);
    assert!(out[0].wrapped);
    assert!(!out[0].prompt);
}

#[test]
fn decode_widens_to_current_cols() {
    let mut row = Row::new(4);
    row.cells[3] = cell('d');
    let block = encode(std::slice::from_ref(&row), 4);
    let out = decode(&block, 9);
    assert_eq!(out[0].len(), 9);
    assert_eq!(out[0].cells[3].ch, 'd');
    assert_eq!(out[0].cells[8], Cell::default());
}

#[test]
fn decode_narrows_and_repairs_sliced_wide_pair() {
    let mut row = Row::new(6);
    row.cells[4] = Cell {
        ch: '漢',
        flags: CellFlags::WIDE,
        ..Cell::default()
    };
    row.cells[5] = Cell {
        flags: CellFlags::WIDE_SPACER,
        ..Cell::default()
    };
    let block = encode(std::slice::from_ref(&row), 6);
    // Cutting at 5 strands the wide head in the last column; it must be
    // blanked exactly as `Row::resize` does on a live row.
    let out = decode(&block, 5);
    assert_eq!(out[0].len(), 5);
    let mut expect = row.clone();
    expect.resize(5, Cell::default());
    assert_eq!(out[0], expect);
    assert_eq!(out[0].cells[4], Cell::default());
}
