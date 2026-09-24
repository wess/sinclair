//! Payload → RGBA. Handles every transmission format the protocol defines:
//! raw RGB (`f=24`), raw RGBA (`f=32`), and PNG (`f=100`), each optionally
//! zlib-compressed (`o=z`).

use super::{Control, GfxError};
use crate::image::Image;

/// Largest image dimension (px) we will decode, and the pixel-count cap that
/// bounds a raw-format allocation. Mirrors `sixel::MAX_DIM` in spirit.
pub(crate) const MAX_DIM: u32 = 10_000;
pub(crate) const MAX_PIXELS: usize = 25_000_000; // 25 MP

/// Ceiling on a decompressed (`o=z`) payload. A valid image never needs more
/// than `MAX_PIXELS` RGBA bytes (a PNG file is smaller still), so anything past
/// this is a decompression bomb and is refused before it is fully allocated.
pub(crate) const MAX_DECODED_BYTES: u64 = (MAX_PIXELS * 4) as u64;

/// Decode a fully-assembled payload into RGBA using `control`'s format and
/// compression. The caller has already base64-decoded it, joined its chunks,
/// and — for the file and shared-memory media — read it off the medium.
pub(crate) fn decode(control: &Control, raw: &[u8]) -> Result<Image, GfxError> {
  let data = if control.compressed {
    inflate(raw)?
  } else {
    raw.to_vec()
  };
  match control.format {
    24 => raw_pixels(control, &data, 3),
    32 => raw_pixels(control, &data, 4),
    100 => decode_png(&data),
    _ => Err(GfxError("EFORMAT")),
  }
}

/// zlib-inflate the payload, refusing decompression bombs. `Read::take` bounds
/// how many bytes `read_to_end` will pull, so the output allocation can never
/// exceed the cap even for a hostile stream that expands ~1000x.
fn inflate(raw: &[u8]) -> Result<Vec<u8>, GfxError> {
  use std::io::Read;
  let mut out = Vec::new();
  flate2::read::ZlibDecoder::new(raw)
    .take(MAX_DECODED_BYTES + 1)
    .read_to_end(&mut out)
    .map_err(|_| GfxError("EINFLATE"))?;
  if out.len() as u64 > MAX_DECODED_BYTES {
    return Err(GfxError("EINFLATE"));
  }
  Ok(out)
}

/// Build an RGBA image from raw pixels with `channels` (3 = RGB, 4 = RGBA),
/// using the declared `s`×`v` dimensions.
fn raw_pixels(control: &Control, data: &[u8], channels: usize) -> Result<Image, GfxError> {
  let (w, h) = (control.width, control.height);
  let pixels = check_dims(w, h)?;
  if data.len() < pixels * channels {
    return Err(GfxError("ESIZE"));
  }
  let mut rgba = Vec::with_capacity(pixels * 4);
  for px in data[..pixels * channels].chunks_exact(channels) {
    rgba.extend_from_slice(&[px[0], px[1], px[2], if channels == 4 { px[3] } else { 255 }]);
  }
  Ok(Image {
    width: w as usize,
    height: h as usize,
    rgba: rgba.into(),
  })
}

/// Decode a PNG payload to RGBA8. Palette/grayscale/16-bit inputs are
/// normalized to 8-bit RGB(A) by the decoder's transformations, then folded to
/// RGBA here.
pub(crate) fn decode_png(data: &[u8]) -> Result<Image, GfxError> {
  let mut decoder = png::Decoder::new(data);
  decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
  let mut reader = decoder.read_info().map_err(|_| GfxError("EPNG"))?;
  let info = reader.info();
  let (w, h) = (info.width, info.height);
  let pixels = check_dims(w, h).map_err(|_| GfxError("EDIM"))?;
  let mut buf = vec![0u8; reader.output_buffer_size()];
  let frame = reader.next_frame(&mut buf).map_err(|_| GfxError("EPNG"))?;
  let src = &buf[..frame.buffer_size()];
  let mut rgba = Vec::with_capacity(pixels * 4);
  match frame.color_type {
    png::ColorType::Rgba => rgba.extend_from_slice(src),
    png::ColorType::Rgb => {
      for p in src.as_chunks::<3>().0 {
        rgba.extend_from_slice(&[p[0], p[1], p[2], 255]);
      }
    }
    png::ColorType::GrayscaleAlpha => {
      for p in src.as_chunks::<2>().0 {
        rgba.extend_from_slice(&[p[0], p[0], p[0], p[1]]);
      }
    }
    png::ColorType::Grayscale => {
      for &g in src {
        rgba.extend_from_slice(&[g, g, g, 255]);
      }
    }
    png::ColorType::Indexed => return Err(GfxError("EPNG")),
  }
  if rgba.len() != pixels * 4 {
    return Err(GfxError("EPNG"));
  }
  Ok(Image {
    width: w as usize,
    height: h as usize,
    rgba: rgba.into(),
  })
}

/// Validate a declared size and return its pixel count.
fn check_dims(w: u32, h: u32) -> Result<usize, GfxError> {
  if w == 0 || h == 0 || w > MAX_DIM || h > MAX_DIM {
    return Err(GfxError("EDIM"));
  }
  (w as usize)
    .checked_mul(h as usize)
    .filter(|&p| p <= MAX_PIXELS)
    .ok_or(GfxError("EDIM"))
}

#[cfg(test)]
#[path = "../../tests/graphics/decode.rs"]
mod tests;
