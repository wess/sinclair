use super::*;

#[test]
fn the_table_is_the_size_the_spec_defines() {
  assert_eq!(DIACRITICS.len(), 297);
  assert_eq!(DIACRITICS[0], 0x0305);
  assert_eq!(DIACRITICS[296], 0x1D244);
}

#[test]
fn the_table_is_sorted_so_the_search_is_valid() {
  assert!(DIACRITICS.windows(2).all(|w| w[0] < w[1]));
}

#[test]
fn a_diacritic_round_trips_to_its_value() {
  for value in [0u16, 1, 42, 255, 296] {
    let c = diacritic(value).expect("in range");
    assert_eq!(diacritic_value(c), Some(value));
  }
  assert_eq!(diacritic(297), None);
}

#[test]
fn ordinary_characters_are_not_diacritics() {
  assert_eq!(diacritic_value('a'), None);
  // U+0301 (acute) is deliberately excluded from the table.
  assert_eq!(diacritic_value('\u{0301}'), None);
}

#[test]
fn marks_fill_row_then_column_then_the_id_high_byte() {
  let mut p = Placeholder::default();
  assert_eq!(p, Placeholder::default());
  p.push(diacritic(3).unwrap());
  assert_eq!(p.row, Some(3));
  p.push(diacritic(9).unwrap());
  assert_eq!(p.col, Some(9));
  p.push(diacritic(200).unwrap());
  assert_eq!(p.id_high, Some(200));
  // A fourth mark is not defined and is dropped rather than corrupting one.
  p.push(diacritic(1).unwrap());
  assert_eq!((p.row, p.col, p.id_high), (Some(3), Some(9), Some(200)));
}

#[test]
fn a_non_diacritic_mark_is_ignored() {
  let mut p = Placeholder::default();
  p.push('a');
  assert_eq!(p, Placeholder::default());
}

#[test]
fn the_image_id_comes_from_the_foreground_color() {
  let mut p = Placeholder::default();
  assert_eq!(p.image_id((0, 0, 7)), 7);
  assert_eq!(p.image_id((1, 2, 3)), 0x010203);
  // The third diacritic supplies the byte the color cannot carry.
  p.push(diacritic(0).unwrap());
  p.push(diacritic(0).unwrap());
  p.push(diacritic(0xAB).unwrap());
  assert_eq!(p.image_id((1, 2, 3)), 0xAB01_0203);
}

/// Build a row of placeholder cells. Each entry is `(row, col, id_high)`,
/// with `None` meaning the diacritic was omitted.
fn row(id: u32, spec: &[(Option<u16>, Option<u16>, Option<u8>)]) -> Vec<crate::cell::Cell> {
  spec
    .iter()
    .map(|&(r, c, high)| {
      let mut cell = crate::cell::Cell {
        ch: PLACEHOLDER,
        fg: crate::color::Color::Rgb((id >> 16) as u8, (id >> 8) as u8, id as u8),
        ..Default::default()
      };
      for value in [r, c, high.map(u16::from)].into_iter().flatten() {
        cell.push_combining(diacritic(value).unwrap());
      }
      cell
    })
    .collect()
}

#[test]
fn a_fully_spelled_run_collapses_into_one_draw() {
  let cells = row(7, &[(Some(0), Some(0), None), (Some(0), Some(1), None)]);
  let mut out = Vec::new();
  scan_row(&cells, 3, &mut out);
  assert_eq!(
    out,
    vec![Draw {
      row: 3,
      col: 0,
      len: 2,
      image_id: 7,
      placement_id: 0,
      image_row: 0,
      image_col: 0,
    }]
  );
}

#[test]
fn omitted_coordinates_continue_the_run() {
  // The compact form clients actually emit: spell the first cell, leave the
  // rest bare.
  let cells = row(9, &[(Some(2), Some(5), None), (None, None, None), (None, None, None)]);
  let mut out = Vec::new();
  scan_row(&cells, 0, &mut out);
  assert_eq!(out.len(), 1);
  assert_eq!(out[0].len, 3);
  assert_eq!((out[0].image_row, out[0].image_col), (2, 5));
}

#[test]
fn a_gap_in_the_text_breaks_the_run() {
  let mut cells = row(1, &[(Some(0), Some(0), None), (Some(0), Some(1), None)]);
  cells.insert(1, crate::cell::Cell::default());
  let mut out = Vec::new();
  scan_row(&cells, 0, &mut out);
  assert_eq!(out.len(), 2, "an ordinary cell splits the run");
  assert_eq!((out[0].col, out[0].len), (0, 1));
  assert_eq!((out[1].col, out[1].len), (2, 1));
}

#[test]
fn a_different_image_breaks_the_run() {
  let mut cells = row(1, &[(Some(0), Some(0), None)]);
  cells.extend(row(2, &[(Some(0), Some(1), None)]));
  let mut out = Vec::new();
  scan_row(&cells, 0, &mut out);
  assert_eq!(out.len(), 2);
  assert_eq!((out[0].image_id, out[1].image_id), (1, 2));
}

#[test]
fn a_column_jump_breaks_the_run() {
  // Adjacent cells whose image columns are not adjacent are two draws.
  let cells = row(1, &[(Some(0), Some(0), None), (Some(0), Some(4), None)]);
  let mut out = Vec::new();
  scan_row(&cells, 0, &mut out);
  assert_eq!(out.len(), 2);
  assert_eq!(out[1].image_col, 4);
}

#[test]
fn an_indexed_foreground_names_a_small_image_id() {
  // A client with only 256 colors spells the id as a palette index.
  let mut cell = crate::cell::Cell {
    ch: PLACEHOLDER,
    fg: crate::color::Color::Indexed(12),
    ..Default::default()
  };
  cell.push_combining(diacritic(0).unwrap());
  let mut out = Vec::new();
  scan_row(&[cell], 0, &mut out);
  assert_eq!(out[0].image_id, 12);
}

#[test]
fn the_underline_color_names_the_placement() {
  let mut cell = crate::cell::Cell {
    ch: PLACEHOLDER,
    fg: crate::color::Color::Rgb(0, 0, 3),
    underline_color: crate::color::Color::Rgb(0, 1, 0),
    ..Default::default()
  };
  cell.push_combining(diacritic(0).unwrap());
  let mut out = Vec::new();
  scan_row(&[cell], 0, &mut out);
  assert_eq!((out[0].image_id, out[0].placement_id), (3, 256));
}

#[test]
fn a_row_with_no_placeholders_yields_nothing() {
  let cells = vec![crate::cell::Cell::default(); 8];
  let mut out = Vec::new();
  scan_row(&cells, 0, &mut out);
  assert!(out.is_empty());
}
