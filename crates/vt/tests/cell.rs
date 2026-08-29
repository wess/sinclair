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
  let pen = Cell {
    bg: Color::Indexed(4),
    fg: Color::Indexed(1),
    flags: CellFlags::BOLD | CellFlags::UNDERLINE,
    ..Default::default()
  };
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

#[test]
fn a_placeholder_cell_decodes_its_marks_as_coordinates() {
  use crate::placeholder::{diacritic, PLACEHOLDER};
  let mut cell = Cell {
    ch: PLACEHOLDER,
    ..Default::default()
  };
  cell.push_combining(diacritic(4).unwrap());
  cell.push_combining(diacritic(11).unwrap());
  let p = cell.placeholder().expect("a placeholder");
  assert_eq!((p.row, p.col), (Some(4), Some(11)));
  // Those marks are coordinates, not text.
  assert_eq!(cell.combining().count(), 0);
  let mut text = String::new();
  cell.write_grapheme(&mut text);
  assert_eq!(text, PLACEHOLDER.to_string());
}

#[test]
fn an_ordinary_cell_still_keeps_one_combining_mark() {
  let mut cell = Cell {
    ch: 'e',
    ..Default::default()
  };
  cell.push_combining('\u{0301}');
  cell.push_combining('\u{0302}');
  assert_eq!(cell.combining().collect::<Vec<_>>(), vec!['\u{0301}']);
  assert!(cell.placeholder().is_none());
}

#[test]
fn the_zero_width_payload_costs_no_extra_cell_bytes() {
  // The packed form has to stay the size of the `char` it replaced, or every
  // grid and every scrollback block grows.
  assert_eq!(
    std::mem::size_of::<ZeroWidth>(),
    std::mem::size_of::<char>()
  );
}

#[test]
fn packed_placeholder_fields_round_trip() {
  use crate::placeholder::{diacritic, Placeholder, PLACEHOLDER};
  for (r, c, high) in [(0u16, 0u16, 0u8), (296, 296, 255), (1, 2, 3)] {
    let mut cell = Cell {
      ch: PLACEHOLDER,
      ..Default::default()
    };
    for v in [r, c, high as u16] {
      cell.push_combining(diacritic(v).unwrap());
    }
    assert_eq!(
      cell.placeholder(),
      Some(Placeholder {
        row: Some(r),
        col: Some(c),
        id_high: Some(high),
      })
    );
  }
}
