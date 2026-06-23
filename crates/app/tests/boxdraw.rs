use super::*;

#[test]
fn horizontal_line_is_a_centred_bar() {
    let g = rects('\u{2500}', 8.0, 16.0).unwrap();
    let t = thickness(16.0);
    assert_eq!(g.rects, vec![(0.0, ((16.0 - t) / 2.0).round(), 8.0, t)]);
    assert_eq!(g.alpha, 1.0);
}

#[test]
fn cross_has_both_bars() {
    let g = rects('\u{253C}', 8.0, 16.0).unwrap();
    assert_eq!(g.rects.len(), 2); // h_full + v_full
}

#[test]
fn corner_joins_two_arms() {
    // ┌ spans right and down from centre.
    let g = rects('\u{250C}', 10.0, 16.0).unwrap();
    assert_eq!(g.rects.len(), 2);
}

#[test]
fn full_block_fills_cell() {
    let g = rects('\u{2588}', 8.0, 16.0).unwrap();
    assert_eq!(g.rects, vec![(0.0, 0.0, 8.0, 16.0)]);
}

#[test]
fn halves_cover_their_side() {
    assert_eq!(
        rects('\u{2580}', 8.0, 16.0).unwrap().rects,
        vec![(0.0, 0.0, 8.0, 8.0)]
    );
    assert_eq!(
        rects('\u{2584}', 8.0, 16.0).unwrap().rects,
        vec![(0.0, 8.0, 8.0, 8.0)]
    );
    assert_eq!(
        rects('\u{258C}', 8.0, 16.0).unwrap().rects,
        vec![(0.0, 0.0, 4.0, 16.0)]
    );
    assert_eq!(
        rects('\u{2590}', 8.0, 16.0).unwrap().rects,
        vec![(4.0, 0.0, 4.0, 16.0)]
    );
}

#[test]
fn shades_use_alpha() {
    assert_eq!(rects('\u{2591}', 8.0, 16.0).unwrap().alpha, 0.25);
    assert_eq!(rects('\u{2592}', 8.0, 16.0).unwrap().alpha, 0.5);
    assert_eq!(rects('\u{2593}', 8.0, 16.0).unwrap().alpha, 0.75);
}

#[test]
fn lower_eighths_grow_from_bottom() {
    // ▁ = 1/8 tall at the bottom.
    assert_eq!(
        rects('\u{2581}', 8.0, 16.0).unwrap().rects,
        vec![(0.0, 14.0, 8.0, 2.0)]
    );
    // ▇ = 7/8 tall.
    assert_eq!(
        rects('\u{2587}', 8.0, 16.0).unwrap().rects,
        vec![(0.0, 2.0, 8.0, 14.0)]
    );
}

#[test]
fn left_eighths_grow_from_left() {
    // ▏ = 1/8 wide on the left.
    assert_eq!(
        rects('\u{258F}', 8.0, 16.0).unwrap().rects,
        vec![(0.0, 0.0, 1.0, 16.0)]
    );
    // ▉ = 7/8 wide.
    assert_eq!(
        rects('\u{2589}', 8.0, 16.0).unwrap().rects,
        vec![(0.0, 0.0, 7.0, 16.0)]
    );
}

#[test]
fn ordinary_characters_are_not_handled() {
    assert!(rects('a', 8.0, 16.0).is_none());
    assert!(rects('═', 8.0, 16.0).is_none()); // heavy/double not yet covered
}
