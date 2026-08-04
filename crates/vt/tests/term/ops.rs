use super::{MAX_GRAPHICS_BYTES, MAX_GRAPHICS_ITEMS};
use crate::term::Terminal;

fn term() -> Terminal {
    Terminal::new(10, 4, 50)
}

#[test]
fn write_char_advances() {
    let mut t = term();
    t.inner.write_char('a');
    t.inner.write_char('b');
    assert_eq!(t.row_text(0), "ab");
    assert_eq!(t.cursor_pos(), (0, 2));
}

#[test]
fn combining_mark_attaches_to_previous_cell() {
    let mut t = term();
    t.inner.write_char('e');
    t.inner.write_char('\u{0301}'); // combining acute accent
    assert_eq!(t.cursor_pos(), (0, 1)); // mark does not advance the cursor
    assert_eq!(
        t.cell(0, 0).combining().collect::<Vec<_>>(),
        vec!['\u{0301}']
    );
    assert_eq!(t.row_text(0), "e\u{0301}");
}

#[test]
fn combining_mark_attaches_to_wide_head() {
    let mut t = term();
    t.inner.write_char('漢');
    t.inner.write_char('\u{0301}');
    assert_eq!(
        t.cell(0, 0).combining().collect::<Vec<_>>(),
        vec!['\u{0301}']
    );
    assert!(t.cell(0, 1).combining().next().is_none());
}

#[test]
fn combining_keeps_first_mark() {
    let mut t = term();
    t.inner.write_char('a');
    for m in ['\u{0301}', '\u{0302}'] {
        t.inner.write_char(m);
    }
    assert_eq!(
        t.cell(0, 0).combining().collect::<Vec<_>>(),
        vec!['\u{0301}']
    );
}

#[test]
fn wide_char_takes_two_cells() {
    let mut t = term();
    t.inner.write_char('漢');
    assert!(t.cell(0, 0).is_wide());
    assert!(t.cell(0, 1).is_wide_spacer());
    assert_eq!(t.cursor_pos(), (0, 2));
}

#[test]
fn overwriting_wide_head_clears_spacer() {
    let mut t = term();
    t.inner.write_char('漢');
    t.inner.cursor_to(0, 0);
    t.inner.write_char('x');
    assert_eq!(t.cell(0, 0).ch, 'x');
    assert!(!t.cell(0, 1).is_wide_spacer());
}

#[test]
fn overwriting_spacer_clears_head() {
    let mut t = term();
    t.inner.write_char('漢');
    t.inner.cursor_to(0, 1);
    t.inner.write_char('x');
    assert_eq!(t.cell(0, 1).ch, 'x');
    assert!(!t.cell(0, 0).is_wide());
    assert_eq!(t.cell(0, 0).ch, ' ');
}

#[test]
fn pending_wrap_then_write_wraps() {
    let mut t = term();
    for _ in 0..10 {
        t.inner.write_char('x');
    }
    assert_eq!(t.cursor_pos(), (0, 9));
    assert!(t.screen().cursor.pending_wrap);
    t.inner.write_char('y');
    assert_eq!(t.cursor_pos(), (1, 1));
    assert_eq!(t.cell(1, 0).ch, 'y');
    assert!(t.grid().row(0).wrapped);
}

#[test]
fn cursor_movement_clamps() {
    let mut t = term();
    t.inner.cursor_up(5);
    assert_eq!(t.cursor_pos(), (0, 0));
    t.inner.cursor_down(99);
    assert_eq!(t.cursor_pos(), (3, 0));
    t.inner.cursor_right(99);
    assert_eq!(t.cursor_pos(), (3, 9));
    t.inner.cursor_left(99);
    assert_eq!(t.cursor_pos(), (3, 0));
}

#[test]
fn scroll_region_constrains_linefeed() {
    let mut t = term();
    // 1-based margins 2..3 = rows 1..2 zero-based.
    t.inner.set_scroll_region(2, 3);
    t.inner.cursor_to(1, 0);
    t.inner.write_char('a');
    t.inner.linefeed();
    t.inner.carriage_return();
    t.inner.write_char('b');
    t.inner.linefeed(); // at bottom margin: scrolls region, cursor stays
    assert_eq!(t.row_text(1), "b");
    assert_eq!(t.row_text(0), ""); // row 0 untouched
    assert_eq!(t.row_text(3), ""); // below region untouched
    assert_eq!(t.cursor_pos().0, 2);
}

#[test]
fn delete_chars_shifts_left() {
    let mut t = term();
    t.feed(b"abcdef");
    t.inner.cursor_to(0, 1);
    t.inner.delete_chars(2);
    assert_eq!(t.row_text(0), "adef");
}

#[test]
fn insert_blank_shifts_right() {
    let mut t = term();
    t.feed(b"abc");
    t.inner.cursor_to(0, 1);
    t.inner.insert_blank(2);
    assert_eq!(t.row_text(0), "a  bc");
}

#[test]
fn full_reset_restores_defaults() {
    let mut t = term();
    t.feed(b"\x1b[?25l\x1b[31mhello");
    t.inner.full_reset();
    assert!(t.cursor_visible());
    assert_eq!(t.row_text(0), "");
    assert_eq!(t.cursor_pos(), (0, 0));
}

#[test]
fn alignment_test_fills_screen() {
    let mut t = term();
    t.inner.screen_alignment_test();
    assert_eq!(t.row_text(0), "EEEEEEEEEE");
    assert_eq!(t.row_text(3), "EEEEEEEEEE");
}

/// 10-col terminal whose row 0 soft-wrapped into row 1.
fn wrapped() -> Terminal {
    let mut t = term();
    t.feed(b"0123456789ab");
    assert!(t.grid().row(0).wrapped);
    t
}

#[test]
fn erase_line_to_end_breaks_wrap() {
    let mut t = wrapped();
    t.feed(b"\x1b[1;5H\x1b[K"); // EL 0 from mid-row
    assert!(!t.grid().row(0).wrapped);
    let mut t = wrapped();
    t.feed(b"\x1b[1;1H\x1b[2K"); // EL 2
    assert!(!t.grid().row(0).wrapped);
    // EL 1 leaves the tail (and the continuation) intact.
    let mut t = wrapped();
    t.feed(b"\x1b[1;5H\x1b[1K");
    assert!(t.grid().row(0).wrapped);
}

#[test]
fn erase_display_below_breaks_wrap_on_cursor_row() {
    let mut t = wrapped();
    t.feed(b"\x1b[1;5H\x1b[J");
    assert!(!t.grid().row(0).wrapped);
}

#[test]
fn char_edits_break_wrap() {
    let mut t = wrapped();
    t.feed(b"\x1b[1;1H\x1b[P"); // DCH shifts the tail left
    assert!(!t.grid().row(0).wrapped);
    let mut t = wrapped();
    t.feed(b"\x1b[1;1H\x1b[@"); // ICH pushes content off the right edge
    assert!(!t.grid().row(0).wrapped);
    let mut t = wrapped();
    t.feed(b"\x1b[1;10H\x1b[X"); // ECH reaching the last column
    assert!(!t.grid().row(0).wrapped);
    // ECH that stops short keeps the continuation.
    let mut t = wrapped();
    t.feed(b"\x1b[1;1H\x1b[X");
    assert!(t.grid().row(0).wrapped);
}

#[test]
fn overwriting_last_cell_breaks_wrap() {
    let mut t = wrapped();
    t.feed(b"\x1b[1;10HZ");
    assert!(!t.grid().row(0).wrapped);
    // ...and a fresh continuation re-sets it.
    t.feed(b"\x1b[2;1H"); // park the cursor; flag stays cleared
    assert!(!t.grid().row(0).wrapped);
    let mut t = wrapped();
    t.feed(b"\x1b[1;10HZw"); // overwrite then continue: re-wrapped
    assert!(t.grid().row(0).wrapped);
    assert_eq!(t.cell(1, 0).ch, 'w');
}

#[test]
fn restore_cursor_reclamps_into_scroll_region_when_origin_restored() {
    let mut t = Terminal::new(80, 24, 0);
    // DECOM on, save at the region top, move the margins, restore, DSR:
    // the cursor must land inside the new region and the report must not
    // underflow past it.
    t.feed(b"\x1b[?6h\x1b7\x1b[5;10r\x1b8\x1b[6n");
    assert_eq!(t.cursor_pos(), (4, 0));
    assert_eq!(t.take_output(), b"\x1b[1;1R");
}

/// Terminal with `ab漢cd`: wide head at column 2, spacer at column 3.
fn with_wide() -> Terminal {
    let mut t = term();
    t.feed("ab\u{6f22}cd".as_bytes());
    assert!(t.cell(0, 2).is_wide());
    assert!(t.cell(0, 3).is_wide_spacer());
    t
}

fn no_wide_halves(t: &Terminal) -> bool {
    !t.grid()
        .row(0)
        .cells
        .iter()
        .any(|c| c.is_wide() || c.is_wide_spacer())
}

#[test]
fn ech_repairs_wide_pair_on_either_half() {
    let mut t = with_wide();
    t.feed(b"\x1b[1;3H\x1b[X"); // ECH on the head
    assert!(no_wide_halves(&t));
    let mut t = with_wide();
    t.feed(b"\x1b[1;4H\x1b[X"); // ECH on the spacer
    assert!(no_wide_halves(&t));
    assert_eq!(t.cell(0, 2).ch, ' ');
}

#[test]
fn dch_repairs_wide_pair_on_either_half() {
    let mut t = with_wide();
    t.feed(b"\x1b[1;3H\x1b[P"); // DCH on the head
    assert!(no_wide_halves(&t));
    assert_eq!(t.row_text(0), "ab cd");
    let mut t = with_wide();
    t.feed(b"\x1b[1;4H\x1b[P"); // DCH on the spacer
    assert!(no_wide_halves(&t));
    assert_eq!(t.row_text(0), "ab cd");
}

#[test]
fn ich_repairs_pair_split_at_cursor() {
    let mut t = with_wide();
    t.feed(b"\x1b[1;4H\x1b[@"); // ICH at the spacer tears the pair apart
    assert!(no_wide_halves(&t));
    assert_eq!(t.cell(0, 5).ch, 'c');
}

#[test]
fn ich_blanks_head_pushed_into_last_column() {
    let mut t = term();
    t.feed("12345678\u{6f22}".as_bytes()); // head at col 8, spacer at col 9
    assert!(t.cell(0, 8).is_wide());
    t.feed(b"\x1b[1;1H\x1b[@"); // shift right: the spacer falls off the edge
    assert!(no_wide_halves(&t));
    assert_eq!(t.cell(0, 1).ch, '1');
}

#[test]
fn el_repairs_wide_pair_at_boundary() {
    let mut t = with_wide();
    t.feed(b"\x1b[1;4H\x1b[K"); // EL 0 from the spacer strands the head
    assert!(no_wide_halves(&t));
    let mut t = with_wide();
    t.feed(b"\x1b[1;3H\x1b[1K"); // EL 1 through the head strands the spacer
    assert!(no_wide_halves(&t));
}

#[test]
fn column_shrink_repairs_sliced_wide_pair() {
    let mut t = Terminal::new(4, 2, 0);
    t.feed("ab\u{6f22}".as_bytes()); // head at col 2, spacer at col 3
    t.resize(3, 2); // truncation cuts the spacer off
    assert!(no_wide_halves(&t));
    assert_eq!(t.row_text(0), "ab");
}

#[test]
fn image_budget_evicts_oldest_placements() {
    let mut t = term();
    // Five ~30 MiB images exceed the pane-wide 128 MiB budget: the oldest goes.
    for _ in 0..5 {
        t.inner.place_sixel(crate::sixel::Image {
            width: 4,
            height: 4,
            rgba: vec![0; 30 << 20].into(),
        });
    }
    assert_eq!(t.images().len(), 4);
    assert_eq!(t.images()[0].id, 1);
    assert!(t.graphics_memory() <= MAX_GRAPHICS_BYTES);
}

#[test]
fn stored_and_placed_copies_share_one_pixel_buffer() {
    let mut t = term();
    let image = crate::sixel::Image {
        width: 4,
        height: 4,
        rgba: vec![0; 30 << 20].into(),
    };
    t.inner.gfx_store.insert(7, image.clone());
    t.inner.place_image(image, 7, false);
    assert!(std::sync::Arc::ptr_eq(
        &t.inner.gfx_store[&7].rgba,
        &t.images()[0].image.rgba
    ));
    assert_eq!(t.graphics_memory(), 30 << 20);
}

#[test]
fn tiny_images_cannot_grow_placement_metadata_without_bound() {
    let mut t = term();
    for id in 0..=MAX_GRAPHICS_ITEMS as u64 {
        t.inner.primary.images.push(crate::sixel::Placement {
            id,
            line: 0,
            col: 0,
            image: crate::sixel::Image {
                width: 1,
                height: 1,
                rgba: vec![0; 4].into(),
            },
            kitty_id: None,
        });
    }
    t.inner.enforce_graphics_budget();
    assert_eq!(t.inner.primary.images.len(), MAX_GRAPHICS_ITEMS);
    assert_eq!(t.inner.primary.images[0].id, 1);
}

#[test]
fn images_are_per_screen_across_alt_round_trip() {
    let mut t = Terminal::new(10, 6, 100);
    t.set_cell_pixels(8, 16);
    t.feed(b"\x1bPq#0;2;100;0;0@\x1b\\"); // image on primary
    assert_eq!(t.images().len(), 1);
    let primary_id = t.images()[0].id;
    t.feed(b"\x1b[?1049h"); // enter alt: primary's image is not visible
    assert!(t.images().is_empty());
    t.feed(b"\x1bPq#0;2;0;100;0@\x1b\\"); // draw one in the alt screen
    assert_eq!(t.images().len(), 1);
    t.feed(b"\x1b[?1049l"); // exit: primary's is back, alt's is gone
    assert_eq!(t.images().len(), 1);
    assert_eq!(t.images()[0].id, primary_id);
    t.feed(b"\x1b[?1049h"); // re-enter: the alt image must not ghost back
    assert!(t.images().is_empty());
}

#[test]
fn sixel_placement_damages_covered_rows() {
    use crate::grid::damage::Damage;
    let mut t = Terminal::new(10, 6, 100);
    t.set_cell_pixels(8, 16);
    t.take_damage();
    // 6px tall in 16px cells: one row, placed without scrolling.
    t.feed(b"\x1bPq#0;2;100;0;0@\x1b\\");
    assert_eq!(t.take_damage(), Damage::Rows(vec![0]));
}
