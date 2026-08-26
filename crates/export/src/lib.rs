//! Rendering `.cast` recordings to shareable GIF and video files.
//!
//! The pipeline is: parse the recording ([`cast`]), replay its output through a
//! [`vt`] terminal, sample the reconstructed grid to fixed-rate frames
//! ([`replay`]), rasterize each with the bundled monospace font ([`raster`]),
//! and hand the frames to an encoder - GIF in-process ([`gifenc`]) or video via
//! ffmpeg ([`video`]). [`export`] ties it together, choosing the encoder from
//! the output extension.

pub mod colors;
pub mod font;
pub mod gifenc;
pub mod options;
pub mod quant;
pub mod raster;
pub mod replay;
pub mod timeline;
pub mod video;

use std::fmt;
use std::path::Path;

use cast::Cast;

pub use colors::Colors;
pub use options::{Format, Options};
pub use raster::{Raster, Rgba, SoftwareRaster};
pub use replay::Renderer;

/// Anything that can go wrong producing an export.
#[derive(Debug)]
pub enum Error {
  Io(std::io::Error),
  /// The output path has no extension we recognize.
  UnknownFormat(String),
  /// The recording has no output to render.
  Empty,
  /// Frame pixel dimensions exceed what the GIF format can hold (u16).
  TooLarge(usize, usize),
  /// A video format was requested but ffmpeg is not on `PATH`.
  FfmpegMissing,
  /// ffmpeg ran but failed; carries its trailing log output.
  Ffmpeg(String),
}

impl fmt::Display for Error {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Error::Io(e) => write!(f, "{e}"),
      Error::UnknownFormat(p) => {
        write!(
          f,
          "unrecognized output format for {p} (use .gif, .mp4, .mov, or .webm)"
        )
      }
      Error::Empty => write!(f, "recording has no output to render"),
      Error::TooLarge(w, h) => {
        write!(
          f,
          "{w}x{h} px is larger than a GIF can hold (65535 max); reduce cols/rows or the font size"
        )
      }
      Error::FfmpegMissing => {
        write!(
          f,
          "ffmpeg not found on PATH (required for video; GIF needs no tools)"
        )
      }
      Error::Ffmpeg(log) => write!(f, "ffmpeg failed:\n{log}"),
    }
  }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
  fn from(e: std::io::Error) -> Error {
    Error::Io(e)
  }
}

/// Render the recording at `cast_path` to `out_path` with the bundled software
/// rasterizer, choosing GIF or video by the output file's extension.
pub fn export(cast_path: &Path, out_path: &Path, opts: &Options) -> Result<(), Error> {
  let format = Format::from_path(out_path)
    .ok_or_else(|| Error::UnknownFormat(out_path.display().to_string()))?;
  let cast = Cast::read(cast_path)?;
  if cast.output().next().is_none() {
    return Err(Error::Empty);
  }
  let mut renderer = Renderer::software(&cast, opts);
  encode(&mut renderer, out_path, format)
}

/// Encode a prepared renderer to `out_path` in `format`. Lets a host supply its
/// own [`Raster`] (e.g. a gpui-backed one) and reuse the GIF/video encoders.
pub fn encode<R: Raster>(
  renderer: &mut Renderer<R>,
  out_path: &Path,
  format: Format,
) -> Result<(), Error> {
  match format {
    Format::Gif => gifenc::encode(renderer, out_path),
    other => video::encode(renderer, out_path, other),
  }
}

/// Render `cast_path` to `out_path` using a caller-supplied [`Raster`]. Same as
/// [`export`], but with a custom rasterizer instead of the bundled software one.
pub fn render_file<R: Raster>(
  cast_path: &Path,
  out_path: &Path,
  opts: &Options,
  raster: R,
) -> Result<(), Error> {
  let format = Format::from_path(out_path)
    .ok_or_else(|| Error::UnknownFormat(out_path.display().to_string()))?;
  let cast = Cast::read(cast_path)?;
  if cast.output().next().is_none() {
    return Err(Error::Empty);
  }
  let mut renderer = Renderer::new(&cast, opts, raster);
  encode(&mut renderer, out_path, format)
}
