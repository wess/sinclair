use super::*;
use crate::colors::Colors;
use crate::font::Font;

fn setup(text: &[u8], cols: usize, rows: usize) -> (vt::Terminal, Colors, Font) {
  let mut term = vt::Terminal::new(cols, rows, 0);
  term.feed(text);
  let colors = Colors::from_scheme(theme::default_scheme());
  let font = Font::bundled(16.0);
  (term, colors, font)
}

fn render(term: &vt::Terminal, colors: &Colors, font: &mut Font) -> Rgba {
  let mut out = Rgba::filled(
    term.cols() * font.cell_w,
    term.rows() * font.cell_h,
    colors.bg,
  );
  paint(&mut out, term, colors, font);
  out
}

#[test]
fn image_dimensions_match_grid() {
  let (term, colors, mut font) = setup(b"", 10, 3);
  let img = render(&term, &colors, &mut font);
  assert_eq!(img.width, 10 * font.cell_w);
  assert_eq!(img.height, 3 * font.cell_h);
  assert_eq!(img.data.len(), img.width * img.height * 4);
}

#[test]
fn blank_screen_is_all_background() {
  // Hide the cursor (DECTCEM) so nothing is painted over the blank grid.
  let (term, colors, mut font) = setup(b"\x1b[?25l", 4, 2);
  let img = render(&term, &colors, &mut font);
  let bg = colors.bg;
  assert!(img
    .data
    .as_chunks::<4>().0.iter()
    .all(|px| px[0] == bg.r && px[1] == bg.g && px[2] == bg.b && px[3] == 255));
}

#[test]
fn glyph_paints_foreground_pixels() {
  // A printed letter must put some non-background pixels on screen.
  let (term, colors, mut font) = setup(b"W", 4, 2);
  let img = render(&term, &colors, &mut font);
  let bg = colors.bg;
  let painted = img
    .data
    .as_chunks::<4>().0.iter()
    .any(|px| px[0] != bg.r || px[1] != bg.g || px[2] != bg.b);
  assert!(painted, "expected glyph pixels distinct from background");
}

#[test]
fn every_pixel_is_opaque() {
  let (term, colors, mut font) = setup(b"hello \x1b[31mred\x1b[0m", 20, 2);
  let img = render(&term, &colors, &mut font);
  assert!(img.data.as_chunks::<4>().0.iter().all(|px| px[3] == 255));
}
