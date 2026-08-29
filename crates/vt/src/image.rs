//! Decoded images and the placements that anchor them to the grid.
//!
//! Shared by both image protocols: `sixel` decodes into an [`Image`], and the
//! kitty graphics protocol decodes into one and then describes, in
//! [`Kitty`], exactly which part of it to draw and where.

use std::sync::Arc;

/// A decoded image: tightly-packed RGBA8, `width * height * 4` bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
  pub width: usize,
  pub height: usize,
  /// Shared because the kitty store, its animation frames, and any placement
  /// of the image all point at the same pixels.
  pub rgba: Arc<[u8]>,
}

impl Image {
  /// How many text rows the image spans for cells `cell_h` pixels tall.
  pub fn image_rows(&self, cell_h: usize) -> usize {
    self.height.div_ceil(cell_h.max(1))
  }
}

/// A rectangle in image pixels. A zero `w`/`h` means "to the far edge", which
/// is how the protocol spells an unset `w=`/`h=`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Rect {
  pub x: u32,
  pub y: u32,
  pub w: u32,
  pub h: u32,
}

impl Rect {
  /// The rectangle resolved against a `width`×`height` image, clipped to it.
  pub fn clip(&self, width: usize, height: usize) -> (usize, usize, usize, usize) {
    let x = (self.x as usize).min(width);
    let y = (self.y as usize).min(height);
    let w = match self.w {
      0 => width - x,
      w => (w as usize).min(width - x),
    };
    let h = match self.h {
      0 => height - y,
      h => (h as usize).min(height - y),
    };
    (x, y, w, h)
  }

  /// Whether this rectangle covers the whole of a `width`×`height` image, in
  /// which case a renderer can upload the source untouched.
  pub fn is_whole(&self, width: usize, height: usize) -> bool {
    let (x, y, w, h) = self.clip(width, height);
    x == 0 && y == 0 && w == width && h == height
  }
}

/// Where a placement sits relative to another one (`P=`/`Q=`/`H=`/`V=`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Parent {
  pub image_id: u32,
  pub placement_id: u32,
  /// Cell offset from the parent's own anchor.
  pub h_offset: i32,
  pub v_offset: i32,
}

/// The kitty-specific half of a placement: which part of the image to draw,
/// how large, and in what order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Kitty {
  /// The stored image this draws. Animation frames are looked up live through
  /// [`crate::Terminal::graphics_frames`], so a frame added after the
  /// placement still plays.
  pub image_id: u32,
  /// `p=`, so a later command can target this placement alone.
  pub placement_id: u32,
  /// `x=`/`y=`/`w=`/`h=`: the source rectangle.
  pub src: Rect,
  /// `X=`/`Y=`: where inside the anchor cell the image's top-left lands.
  pub cell_x: u32,
  pub cell_y: u32,
  /// `c=`/`r=`: the cell box to scale into. Zero derives from the pixel size.
  pub cols: u32,
  pub rows: u32,
  /// `z=`: stacking order relative to the text.
  pub z: i32,
  /// `U=1`: drawn wherever placeholder cells name it, not at the anchor.
  pub virtual_placement: bool,
  pub parent: Option<Parent>,
}

/// A decoded image anchored to the grid. `line` is an absolute content line
/// (0 = top of the live grid, negative = scrollback) that shifts as the buffer
/// scrolls, so the image rides along with the text beneath it. `id` is unique
/// per placement so the renderer can cache the GPU texture.
#[derive(Debug, Clone)]
pub struct Placement {
  pub id: u64,
  pub line: isize,
  pub col: usize,
  /// The full source image. Shared with the kitty store when it came from
  /// there, so a placement costs no extra pixels.
  pub image: Image,
  /// Kitty display state; `None` for a sixel placement, which always draws
  /// the whole image at its natural size above the text.
  pub kitty: Option<Kitty>,
}

impl Placement {
  /// The source rectangle to draw.
  pub fn source(&self) -> Rect {
    self.kitty.as_ref().map_or(Rect::default(), |k| k.src)
  }

  /// Stacking order; sixel and unspecified kitty placements sit at 0.
  pub fn z(&self) -> i32 {
    self.kitty.as_ref().map_or(0, |k| k.z)
  }

  /// Whether this placement is drawn only where placeholder cells call for
  /// it, rather than at its own anchor.
  pub fn is_virtual(&self) -> bool {
    self.kitty.as_ref().is_some_and(|k| k.virtual_placement)
  }

  /// The size to draw at, in pixels, given the cell metrics: the `c=`/`r=`
  /// cell box when the client set one, otherwise the source rectangle's own
  /// pixel size.
  pub fn draw_size(&self, cell_w: usize, cell_h: usize) -> (f32, f32) {
    let (_, _, sw, sh) = self.source().clip(self.image.width, self.image.height);
    let Some(k) = &self.kitty else {
      return (sw as f32, sh as f32);
    };
    let w = match k.cols {
      0 => sw as f32,
      c => (c as usize * cell_w) as f32,
    };
    let h = match k.rows {
      0 => sh as f32,
      r => (r as usize * cell_h) as f32,
    };
    (w, h)
  }

  /// How many grid rows the placement covers, for damage and for the cursor
  /// move that follows a display.
  pub fn cell_rows(&self, cell_w: usize, cell_h: usize) -> usize {
    if let Some(k) = &self.kitty {
      if k.rows != 0 {
        return k.rows as usize;
      }
    }
    let (_, h) = self.draw_size(cell_w, cell_h);
    let off = self.kitty.as_ref().map_or(0, |k| k.cell_y) as usize;
    (h as usize + off).div_ceil(cell_h.max(1))
  }

  /// How many grid columns the placement covers.
  pub fn cell_cols(&self, cell_w: usize, cell_h: usize) -> usize {
    if let Some(k) = &self.kitty {
      if k.cols != 0 {
        return k.cols as usize;
      }
    }
    let (w, _) = self.draw_size(cell_w, cell_h);
    let off = self.kitty.as_ref().map_or(0, |k| k.cell_x) as usize;
    (w as usize + off).div_ceil(cell_w.max(1))
  }
}

#[cfg(test)]
#[path = "../tests/image.rs"]
mod tests;
