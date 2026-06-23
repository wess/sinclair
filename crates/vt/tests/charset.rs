use super::*;

#[test]
fn ascii_passes_through() {
    let cs = Charsets::default();
    assert_eq!(cs.map('q'), 'q');
    assert_eq!(cs.map('A'), 'A');
}

#[test]
fn dec_special_maps_line_drawing() {
    let cs = Charsets {
        g0: Charset::DecSpecial,
        ..Default::default()
    };
    assert_eq!(cs.map('q'), '─');
    assert_eq!(cs.map('x'), '│');
    assert_eq!(cs.map('l'), '┌');
    assert_eq!(cs.map('j'), '┘');
    assert_eq!(cs.map('A'), 'A');
}

#[test]
fn shift_out_selects_g1() {
    let mut cs = Charsets {
        g1: Charset::DecSpecial,
        ..Default::default()
    };
    assert_eq!(cs.map('q'), 'q');
    cs.shifted = true;
    assert_eq!(cs.map('q'), '─');
    cs.shifted = false;
    assert_eq!(cs.map('q'), 'q');
}

#[test]
fn final_byte_designation() {
    assert_eq!(from_final(b'0'), Charset::DecSpecial);
    assert_eq!(from_final(b'B'), Charset::Ascii);
    assert_eq!(from_final(b'A'), Charset::Ascii);
}
