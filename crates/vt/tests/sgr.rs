use super::*;

fn pen_after(params: &[&[u16]]) -> Cell {
    let mut pen = Cell::default();
    apply(&mut pen, params.iter().copied());
    pen
}

#[test]
fn empty_resets() {
    let mut pen = Cell::default();
    pen.flags.insert(CellFlags::BOLD);
    pen.fg = Color::Indexed(1);
    apply(&mut pen, std::iter::empty());
    assert_eq!(pen, Cell::default());
}

#[test]
fn zero_resets() {
    let mut pen = Cell::default();
    pen.flags.insert(CellFlags::BOLD | CellFlags::ITALIC);
    pen.bg = Color::Rgb(1, 2, 3);
    apply(&mut pen, [&[0u16][..]]);
    assert_eq!(pen, Cell::default());
}

#[test]
fn simple_attributes() {
    let pen = pen_after(&[&[1], &[3], &[4], &[7], &[9]]);
    assert!(pen.flags.contains(CellFlags::BOLD));
    assert!(pen.flags.contains(CellFlags::ITALIC));
    assert!(pen.flags.contains(CellFlags::UNDERLINE));
    assert!(pen.flags.contains(CellFlags::INVERSE));
    assert!(pen.flags.contains(CellFlags::STRIKETHROUGH));
}

#[test]
fn attribute_clears() {
    let pen = pen_after(&[&[1], &[2], &[4], &[22], &[24]]);
    assert!(!pen.flags.contains(CellFlags::BOLD));
    assert!(!pen.flags.contains(CellFlags::DIM));
    assert!(!pen.flags.contains(CellFlags::UNDERLINE));
}

#[test]
fn double_underline_via_21() {
    let pen = pen_after(&[&[4], &[21]]);
    assert!(pen.flags.contains(CellFlags::DOUBLE_UNDERLINE));
    assert!(!pen.flags.contains(CellFlags::UNDERLINE));
}

#[test]
fn underline_styles_colon() {
    assert!(pen_after(&[&[4, 2]])
        .flags
        .contains(CellFlags::DOUBLE_UNDERLINE));
    assert!(pen_after(&[&[4, 3]])
        .flags
        .contains(CellFlags::CURLY_UNDERLINE));
    assert!(pen_after(&[&[4, 4]])
        .flags
        .contains(CellFlags::DOTTED_UNDERLINE));
    assert!(pen_after(&[&[4, 5]])
        .flags
        .contains(CellFlags::DASHED_UNDERLINE));
    assert!(pen_after(&[&[4, 0]]).flags & CellFlags::ANY_UNDERLINE == CellFlags::empty());
}

#[test]
fn named_colors() {
    let pen = pen_after(&[&[31], &[44]]);
    assert_eq!(pen.fg, Color::Indexed(1));
    assert_eq!(pen.bg, Color::Indexed(4));
}

#[test]
fn bright_colors() {
    let pen = pen_after(&[&[91], &[104]]);
    assert_eq!(pen.fg, Color::Indexed(9));
    assert_eq!(pen.bg, Color::Indexed(12));
}

#[test]
fn default_colors() {
    let pen = pen_after(&[&[31], &[44], &[39], &[49]]);
    assert_eq!(pen.fg, Color::Default);
    assert_eq!(pen.bg, Color::Default);
}

#[test]
fn indexed_semicolon() {
    let pen = pen_after(&[&[38], &[5], &[208]]);
    assert_eq!(pen.fg, Color::Indexed(208));
}

#[test]
fn indexed_colon() {
    let pen = pen_after(&[&[38, 5, 123]]);
    assert_eq!(pen.fg, Color::Indexed(123));
}

#[test]
fn truecolor_semicolon() {
    let pen = pen_after(&[&[48], &[2], &[10], &[20], &[30]]);
    assert_eq!(pen.bg, Color::Rgb(10, 20, 30));
}

#[test]
fn truecolor_colon() {
    let pen = pen_after(&[&[38, 2, 1, 2, 3]]);
    assert_eq!(pen.fg, Color::Rgb(1, 2, 3));
}

#[test]
fn truecolor_colon_with_colorspace() {
    let pen = pen_after(&[&[38, 2, 0, 9, 8, 7]]);
    assert_eq!(pen.fg, Color::Rgb(9, 8, 7));
}

#[test]
fn underline_color() {
    let pen = pen_after(&[&[58, 5, 33]]);
    assert_eq!(pen.underline_color, Color::Indexed(33));
    let pen = pen_after(&[&[58], &[2], &[7], &[8], &[9]]);
    assert_eq!(pen.underline_color, Color::Rgb(7, 8, 9));
    let pen = pen_after(&[&[58, 5, 33], &[59]]);
    assert_eq!(pen.underline_color, Color::Default);
}

#[test]
fn malformed_extended_color_is_ignored() {
    let pen = pen_after(&[&[38], &[9]]);
    assert_eq!(pen.fg, Color::Default);
    let pen = pen_after(&[&[38, 2, 1]]);
    assert_eq!(pen.fg, Color::Default);
}

#[test]
fn values_clamp_to_u8() {
    let pen = pen_after(&[&[38, 2, 300, 300, 300]]);
    assert_eq!(pen.fg, Color::Rgb(255, 255, 255));
}

#[test]
fn parameters_after_color_still_apply() {
    let pen = pen_after(&[&[38], &[5], &[10], &[1]]);
    assert_eq!(pen.fg, Color::Indexed(10));
    assert!(pen.flags.contains(CellFlags::BOLD));
}
