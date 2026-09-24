//! Painting a terminal grid snapshot into an RGBA pixel buffer.
//!
//! The [`Raster`] trait abstracts the per-frame rasterizer so the replay loop
//! can drive either the bundled software renderer here or a higher-fidelity one
//! supplied by the host (e.g. the app's gpui text system).

use theme::Rgb;
use vt::{CellFlags, Terminal};

use crate::colors::Colors;
use crate::font::Font;

/// An RGBA8 image, row-major, 4 bytes per pixel, fully opaque.
pub struct Rgba {
  pub width: usize,
  pub height: usize,
  pub data: Vec<u8>,
}

impl Rgba {
  /// A `width` x `height` buffer filled with `bg`.
  pub fn filled(width: usize, height: usize, bg: Rgb) -> Rgba {
    let mut img = Rgba {
      width,
      height,
      data: vec![0u8; width * height * 4],
    };
    img.fill_bg(bg);
    img
  }

  /// Reset every pixel to opaque `bg`.
  pub fn fill_bg(&mut self, bg: Rgb) {
    for px in self.data.as_chunks_mut::<4>().0 {
      px[0] = bg.r;
      px[1] = bg.g;
      px[2] = bg.b;
      px[3] = 255;
    }
  }

  /// Alpha-blend `color` at `alpha` over the pixel at `(x, y)`; out-of-bounds
  /// coordinates are ignored so overhanging glyphs clip cleanly.
  pub fn blend(&mut self, x: i32, y: i32, color: Rgb, alpha: u8) {
    if x < 0 || y < 0 || x as usize >= self.width || y as usize >= self.height {
      return;
    }
    let i = (y as usize * self.width + x as usize) * 4;
    let a = alpha as u32;
    let inv = 255 - a;
    let px = &mut self.data[i..i + 4];
    px[0] = ((color.r as u32 * a + px[0] as u32 * inv) / 255) as u8;
    px[1] = ((color.g as u32 * a + px[1] as u32 * inv) / 255) as u8;
    px[2] = ((color.b as u32 * a + px[2] as u32 * inv) / 255) as u8;
  }

  /// Fill an opaque rectangle, clipped to the image bounds.
  pub fn fill_rect(&mut self, x0: usize, y0: usize, w: usize, h: usize, color: Rgb) {
    for y in y0..(y0 + h).min(self.height) {
      for x in x0..(x0 + w).min(self.width) {
        let i = (y * self.width + x) * 4;
        self.data[i] = color.r;
        self.data[i + 1] = color.g;
        self.data[i + 2] = color.b;
        self.data[i + 3] = 255;
      }
    }
  }
}

/// A per-frame rasterizer: knows its cell box and paints a terminal grid.
pub trait Raster {
  /// Pixel size of one cell, `(width, height)`.
  fn cell_size(&self) -> (usize, usize);
  /// Paint the terminal's visible grid into `out` (already sized to
  /// `cols*cell_w` x `rows*cell_h`).
  fn frame(&mut self, term: &Terminal, colors: &Colors, out: &mut Rgba);
}

/// The bundled, dependency-free rasterizer: `fontdue` over the embedded
/// monospace font. Portable and headless, at the cost of per-cell (non-shaped)
/// glyphs - no ligatures.
pub struct SoftwareRaster {
  font: Font,
}

impl SoftwareRaster {
  pub fn new(font_px: f32) -> SoftwareRaster {
    SoftwareRaster {
      font: Font::bundled(font_px),
    }
  }
}

impl Raster for SoftwareRaster {
  fn cell_size(&self) -> (usize, usize) {
    (self.font.cell_w, self.font.cell_h)
  }

  fn frame(&mut self, term: &Terminal, colors: &Colors, out: &mut Rgba) {
    paint(out, term, colors, &mut self.font);
  }
}

/// Paint `term` into `out` with the software `font`.
pub fn paint(out: &mut Rgba, term: &Terminal, colors: &Colors, font: &mut Font) {
  out.fill_bg(colors.bg);
  let cols = term.cols();
  let rows = term.rows();
  let cw = font.cell_w;
  let ch = font.cell_h;

  for row in 0..rows {
    for col in 0..cols {
      let cell = *term.cell(row, col);
      if cell.flags.contains(CellFlags::WIDE_SPACER) {
        continue;
      }
      paint_cell(out, colors, font, &cell, row, col, cw, ch, false);
    }
  }

  if term.cursor_visible() {
    let (row, col) = term.cursor_pos();
    if row < rows && col < cols {
      let cell = *term.cell(row, col);
      paint_cell(out, colors, font, &cell, row, col, cw, ch, true);
    }
  }
}

#[allow(clippy::too_many_arguments)]
fn paint_cell(
  img: &mut Rgba,
  colors: &Colors,
  font: &mut Font,
  cell: &vt::Cell,
  row: usize,
  col: usize,
  cw: usize,
  ch: usize,
  cursor: bool,
) {
  let bold = cell.flags.contains(CellFlags::BOLD);
  let mut fg = colors.resolve(cell.fg, colors.fg, bold);
  let mut bg = colors.resolve(cell.bg, colors.bg, false);
  if cell.flags.contains(CellFlags::INVERSE) {
    std::mem::swap(&mut fg, &mut bg);
  }
  if cursor {
    bg = colors.cursor;
    fg = colors.cursor_text;
  }
  if cell.flags.contains(CellFlags::INVISIBLE) {
    fg = bg;
  }

  let x0 = col * cw;
  let y0 = row * ch;
  let width = if cell.is_wide() { cw * 2 } else { cw };
  img.fill_rect(x0, y0, width, ch, bg);

  if cell.ch == ' ' || cell.ch == '\0' {
    return;
  }
  let glyph = font.glyph(cell.ch, bold);
  if glyph.width == 0 || glyph.height == 0 {
    return;
  }
  let pen_x = x0 as i32;
  let baseline_y = y0 as i32 + font.baseline;
  let gx = pen_x + glyph.xmin;
  let gy = baseline_y - glyph.ymin - glyph.height as i32;
  for (i, &cov) in glyph.coverage.iter().enumerate() {
    if cov == 0 {
      continue;
    }
    let dx = (i % glyph.width) as i32;
    let dy = (i / glyph.width) as i32;
    img.blend(gx + dx, gy + dy, fg, cov);
  }
}

#[cfg(test)]
#[path = "../tests/raster.rs"]
mod tests;
