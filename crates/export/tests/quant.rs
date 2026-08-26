use super::*;
use crate::colors::Colors;

fn quant() -> Quantizer {
  Quantizer::from_colors(&Colors::from_scheme(theme::default_scheme()))
}

#[test]
fn palette_is_a_full_256_color_table() {
  let q = quant();
  assert_eq!(q.palette().len(), 256 * 3);
}

#[test]
fn maps_one_index_per_pixel() {
  let q = quant();
  // two opaque pixels
  let rgba = [10, 20, 30, 255, 200, 100, 50, 255];
  let mut out = Vec::new();
  q.map(&rgba, &mut out);
  assert_eq!(out.len(), 2);
}

#[test]
fn background_and_foreground_map_to_distinct_slots() {
  let scheme = theme::default_scheme();
  let colors = Colors::from_scheme(scheme);
  let q = Quantizer::from_colors(&colors);
  let px = |c: theme::Rgb| {
    let mut out = Vec::new();
    q.map(&[c.r, c.g, c.b, 255], &mut out);
    out[0]
  };
  // The default scheme's fg and bg differ, so they must land on different
  // palette entries or the text would be invisible.
  assert_ne!(px(colors.bg), px(colors.fg));
}

#[test]
fn exact_palette_color_round_trips() {
  // A color that is exactly the background should map to a slot whose RGB is
  // that background (nearest-match distance zero).
  let colors = Colors::from_scheme(theme::default_scheme());
  let q = Quantizer::from_colors(&colors);
  let mut out = Vec::new();
  q.map(&[colors.bg.r, colors.bg.g, colors.bg.b, 255], &mut out);
  let idx = out[0] as usize * 3;
  let pal = q.palette();
  // Within the 5-bit quantization bucket of the background.
  assert!((pal[idx] as i32 - colors.bg.r as i32).abs() <= 8);
  assert!((pal[idx + 1] as i32 - colors.bg.g as i32).abs() <= 8);
  assert!((pal[idx + 2] as i32 - colors.bg.b as i32).abs() <= 8);
}
