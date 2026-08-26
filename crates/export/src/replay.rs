//! Driving a recording through `vt` and emitting one image per frame tick.
//!
//! A [`Renderer`] holds the reconstructed terminal, the output byte stream, the
//! adjusted playback timeline, and a pluggable [`Raster`]. [`Renderer::run`]
//! steps a virtual clock at a fixed frame rate, feeding all bytes due by each
//! tick and handing the freshly rendered frame to a callback. Building it up
//! front lets callers query the pixel [`Renderer::dimensions`] (needed to spawn
//! an encoder) before rendering.

use cast::Cast;
use vt::Terminal;

use crate::colors::Colors;
use crate::options::Options;
use crate::raster::{Raster, Rgba, SoftwareRaster};
use crate::timeline;

pub struct Renderer<R: Raster> {
  term: Terminal,
  colors: Colors,
  raster: R,
  /// Output chunks as raw bytes, one per recorded output event.
  chunks: Vec<Vec<u8>>,
  /// Playback time of each chunk, same length as `chunks`.
  times: Vec<f64>,
  fps: u32,
  /// Seconds to hold the final frame.
  tail: f64,
  /// Reused frame buffer, sized once to the grid.
  buf: Rgba,
}

impl<R: Raster> Renderer<R> {
  /// Prepare a renderer for `cast` under `opts`, rasterizing with `raster`.
  pub fn new(cast: &Cast, opts: &Options, raster: R) -> Renderer<R> {
    let cols = opts.cols.unwrap_or(cast.header.width).clamp(1, 1000);
    let rows = opts.rows.unwrap_or(cast.header.height).clamp(1, 1000);

    let scheme = opts
      .theme
      .as_deref()
      .and_then(theme::builtin)
      .unwrap_or_else(|| theme::default_scheme());
    let colors = Colors::from_scheme(scheme);

    let chunks: Vec<Vec<u8>> = cast.output().map(|e| e.data.clone().into_bytes()).collect();
    let raw: Vec<f64> = cast.output().map(|e| e.time).collect();
    let times = timeline::adjust(&raw, opts.speed, opts.idle_cap);

    let (cw, ch) = raster.cell_size();
    let buf = Rgba::filled(cols * cw, rows * ch, colors.bg);

    Renderer {
      term: Terminal::new(cols, rows, 0),
      colors,
      raster,
      chunks,
      times,
      fps: opts.fps.max(1),
      tail: opts.tail.max(0.0),
      buf,
    }
  }

  /// Pixel dimensions of every emitted frame.
  pub fn dimensions(&self) -> (usize, usize) {
    (self.buf.width, self.buf.height)
  }

  /// The resolved color context, for encoders that build a palette from it.
  pub fn colors(&self) -> &Colors {
    &self.colors
  }

  pub fn fps(&self) -> u32 {
    self.fps
  }

  /// Number of frames [`run`](Self::run) will emit.
  pub fn frame_count(&self) -> usize {
    let dt = 1.0 / self.fps as f64;
    let end = self.times.last().copied().unwrap_or(0.0) + self.tail;
    (end / dt).floor() as usize + 1
  }

  /// Render every frame in order, handing each to `on_frame`. Frames arrive at
  /// a constant `1/fps` cadence; the callback decides how to encode them.
  pub fn run<F: FnMut(&Rgba)>(&mut self, mut on_frame: F) {
    let dt = 1.0 / self.fps as f64;
    let frames = self.frame_count();
    let mut fed = 0;
    for f in 0..frames {
      let t = f as f64 * dt;
      while fed < self.times.len() && self.times[fed] <= t {
        self.term.feed(&self.chunks[fed]);
        fed += 1;
      }
      self.raster.frame(&self.term, &self.colors, &mut self.buf);
      on_frame(&self.buf);
    }
  }
}

impl Renderer<SoftwareRaster> {
  /// A renderer using the bundled software rasterizer at `opts.font_px`.
  pub fn software(cast: &Cast, opts: &Options) -> Renderer<SoftwareRaster> {
    Renderer::new(cast, opts, SoftwareRaster::new(opts.font_px))
  }
}

#[cfg(test)]
#[path = "../tests/replay.rs"]
mod tests;
