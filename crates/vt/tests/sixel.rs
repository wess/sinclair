use super::*;

/// RGBA at pixel (x, y).
fn px(img: &Image, x: usize, y: usize) -> [u8; 4] {
  let i = (y * img.width + x) * 4;
  img.rgba[i..i + 4].try_into().unwrap()
}

#[test]
fn single_sixel_paints_top_pixel() {
  // Define+select color 0 as pure red, then one sixel byte with bit 0 set.
  let img = decode(b"#0;2;100;0;0@").unwrap();
  assert_eq!(img.width, 1);
  assert_eq!(img.height, 6);
  assert_eq!(px(&img, 0, 0), [255, 0, 0, 255]);
  assert_eq!(px(&img, 0, 1), [0, 0, 0, 0]); // unset bits are transparent
}

#[test]
fn sixel_bits_map_bottom_to_top() {
  // 0x7e ('~') = 0x3f + 0b111111 -> all six pixels set.
  let img = decode(b"#0;2;0;100;0~").unwrap();
  assert_eq!(img.height, 6);
  for y in 0..6 {
    assert_eq!(px(&img, 0, y), [0, 255, 0, 255]);
  }
}

#[test]
fn run_length_repeats_columns() {
  let img = decode(b"#0;2;0;0;100!4@").unwrap();
  assert_eq!(img.width, 4);
  for x in 0..4 {
    assert_eq!(px(&img, x, 0), [0, 0, 255, 255]);
  }
}

#[test]
fn newline_advances_band() {
  // One pixel, carriage return to band 0, then newline to band 1, paint again.
  let img = decode(b"#0;2;100;100;100@-@").unwrap();
  assert_eq!(img.height, 12); // two bands of six
  assert_eq!(px(&img, 0, 0), [255, 255, 255, 255]);
  assert_eq!(px(&img, 0, 6), [255, 255, 255, 255]);
}

#[test]
fn raster_attributes_reserve_size() {
  // "1;1;5;12 reserves a 5x12 canvas before any data.
  let img = decode(b"\"1;1;5;12#0;2;100;0;0@").unwrap();
  assert_eq!(img.width, 5);
  assert_eq!(img.height, 12);
}

#[test]
fn empty_payload_is_none() {
  assert!(decode(b"").is_none());
  assert!(decode(b"#0;2;100;0;0").is_none()); // color set, nothing drawn
}

#[test]
fn carriage_return_overlays_same_band() {
  // Paint red, CR, paint green over column 0 — last color wins.
  let img = decode(b"#0;2;100;0;0@$#1;2;0;100;0@").unwrap();
  assert_eq!(img.width, 1);
  assert_eq!(px(&img, 0, 0), [0, 255, 0, 255]);
}

#[test]
fn pathological_growth_completes_quickly() {
  // Many bands, then thousands of one-column strips with no raster
  // attributes: every column used to recopy the entire tall buffer
  // (quadratic); geometric growth keeps the decode amortized linear.
  let mut data = vec![b'-'; 133];
  data.extend(std::iter::repeat_n(b'@', 8000));
  let start = std::time::Instant::now();
  let img = decode(&data).unwrap();
  assert_eq!(img.width, 8000);
  assert_eq!(img.height, 804);
  assert_eq!(px(&img, 0, 798), [0, 0, 0, 255]); // painted in the last band
  assert!(start.elapsed() < std::time::Duration::from_secs(5));
}
