//! Encoding a frame stream to an animated GIF, in-process via the `gif` crate.
//!
//! Two things keep the file small and the encode fast: one global palette
//! derived from the theme (see [`crate::quant`]) shared by every frame, and
//! coalescing - runs of identical frames collapse into a single frame whose
//! delay is the run's total duration. Terminal video is mostly still, so both
//! pay off. (Inter-frame transparency was tried and dropped: coalescing already
//! handles the low-motion case, and on scrolling content transparency fragments
//! the compression runs and grows the file.)

use std::borrow::Cow;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use gif::{DisposalMethod, Encoder, Frame, Repeat};

use crate::quant::Quantizer;
use crate::raster::Raster;
use crate::replay::Renderer;
use crate::Error;

pub fn encode<R: Raster>(renderer: &mut Renderer<R>, out: &Path) -> Result<(), Error> {
  let (w, h) = renderer.dimensions();
  // GIF dimensions are u16; anything bigger would silently wrap in the
  // `as u16` casts below and corrupt the file.
  if w > u16::MAX as usize || h > u16::MAX as usize {
    return Err(Error::TooLarge(w, h));
  }
  let dt = 1.0 / renderer.fps() as f64;
  let quant = Quantizer::from_colors(renderer.colors());

  let file = BufWriter::new(File::create(out)?);
  let mut enc = Encoder::new(file, w as u16, h as u16, quant.palette()).map_err(encoding_err)?;
  enc.set_repeat(Repeat::Infinite).map_err(encoding_err)?;

  // Coalesce identical consecutive frames: hold the pending frame's pixels and
  // grow its duration until the image changes, then quantize and write once.
  let mut pending: Option<Vec<u8>> = None;
  let mut held = 0.0f64;
  let mut indices: Vec<u8> = Vec::new();
  let mut err: Option<Error> = None;

  renderer.run(|img| {
    if err.is_some() {
      return;
    }
    match &pending {
      Some(prev) if prev == &img.data => held += dt,
      _ => {
        if let Some(prev) = &pending {
          quant.map(prev, &mut indices);
          if let Err(e) = write_frame(&mut enc, w, h, &indices, held) {
            err = Some(e);
            return;
          }
        }
        pending = Some(img.data.clone());
        held = dt;
      }
    }
  });

  if let Some(e) = err {
    return Err(e);
  }
  if let Some(prev) = &pending {
    quant.map(prev, &mut indices);
    write_frame(&mut enc, w, h, &indices, held)?;
  }
  Ok(())
}

fn write_frame<W: Write>(
  enc: &mut Encoder<W>,
  w: usize,
  h: usize,
  indices: &[u8],
  seconds: f64,
) -> Result<(), Error> {
  let frame = Frame {
    width: w as u16,
    height: h as u16,
    buffer: Cow::Borrowed(indices),
    // GIF delays are centiseconds; many players treat <2 as 100ms, so clamp.
    delay: ((seconds * 100.0).round() as u16).max(2),
    dispose: DisposalMethod::Any,
    ..Frame::default()
  };
  enc.write_frame(&frame).map_err(encoding_err)
}

fn encoding_err(e: gif::EncodingError) -> Error {
  match e {
    gif::EncodingError::Io(e) => Error::Io(e),
    gif::EncodingError::Format(e) => {
      Error::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }
  }
}

#[cfg(test)]
#[path = "../tests/gifenc.rs"]
mod tests;
