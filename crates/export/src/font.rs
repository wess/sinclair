//! Glyph rasterization from the bundled monospace font (JetBrains Mono, OFL).
//!
//! We embed a regular and a bold weight and rasterize on demand with `fontdue`,
//! caching coverage bitmaps per `(char, bold)`. The cell size is derived once
//! from the font's line metrics and the advance of `M`, giving a fixed grid the
//! rasterizer lays cells onto.

use std::collections::HashMap;
use std::rc::Rc;

use fontdue::{Font as FdFont, FontSettings};

const REGULAR: &[u8] = include_bytes!("../assets/mono.ttf");
const BOLD: &[u8] = include_bytes!("../assets/mono.bold.ttf");

/// A rasterized glyph: an 8-bit coverage bitmap plus its placement relative to
/// the pen origin on the baseline.
pub struct Glyph {
  pub width: usize,
  pub height: usize,
  /// Left bearing: pixels from the pen x to the bitmap's left edge.
  pub xmin: i32,
  /// Bottom offset: pixels from the baseline up to the bitmap's bottom edge.
  pub ymin: i32,
  /// Row-major coverage, `width * height` bytes, top row first.
  pub coverage: Vec<u8>,
}

pub struct Font {
  regular: FdFont,
  bold: FdFont,
  px: f32,
  /// Cell width in pixels (monospace advance).
  pub cell_w: usize,
  /// Cell height in pixels (line height).
  pub cell_h: usize,
  /// Pixels from the top of a cell down to the baseline.
  pub baseline: i32,
  cache: HashMap<(char, bool), Rc<Glyph>>,
}

impl Font {
  /// Load the bundled font at `px` pixels and derive the cell grid.
  pub fn bundled(px: f32) -> Font {
    let px = px.max(4.0);
    let regular =
      FdFont::from_bytes(REGULAR, FontSettings::default()).expect("bundled regular font parses");
    let bold = FdFont::from_bytes(BOLD, FontSettings::default()).expect("bundled bold font parses");
    let lm = regular
      .horizontal_line_metrics(px)
      .expect("font has horizontal line metrics");
    let cell_h = lm.new_line_size.ceil().max(1.0) as usize;
    let baseline = lm.ascent.ceil() as i32;
    let (m, _) = regular.rasterize('M', px);
    let cell_w = m.advance_width.ceil().max(1.0) as usize;
    Font {
      regular,
      bold,
      px,
      cell_w,
      cell_h,
      baseline,
      cache: HashMap::new(),
    }
  }

  /// The coverage bitmap for `ch`, rasterized once and cached.
  pub fn glyph(&mut self, ch: char, bold: bool) -> Rc<Glyph> {
    if let Some(g) = self.cache.get(&(ch, bold)) {
      return g.clone();
    }
    let font = if bold { &self.bold } else { &self.regular };
    let (m, coverage) = font.rasterize(ch, self.px);
    let glyph = Rc::new(Glyph {
      width: m.width,
      height: m.height,
      xmin: m.xmin,
      ymin: m.ymin,
      coverage,
    });
    self.cache.insert((ch, bold), glyph.clone());
    glyph
  }
}
