//! The transmitted-image store: ids, image numbers, and animation frames.
//!
//! A client names an image either by a client-assigned id (`i=`) or by an
//! image *number* (`I=`), in which case the terminal allocates the id and
//! reports it back. Both name the same entry here, and a number always
//! resolves to the newest image that claimed it.

use std::collections::HashMap;

use super::{Control, GfxError};
use crate::image::Image;

/// The gap a frame gets when the client does not set one.
pub(crate) const DEFAULT_GAP_MS: u32 = 40;

/// Most frames one image may hold. Each is a whole canvas, so an unbounded
/// count is an unbounded allocation driven entirely by the stream; the pane
/// budget would eventually evict the image, but only after it had already
/// grown. At 40 ms a frame this is still twenty seconds of animation.
pub(crate) const MAX_FRAMES: usize = 512;

/// One animation frame: a whole canvas, plus how long it stays up.
#[derive(Debug, Clone)]
pub struct Frame {
  pub image: Image,
  /// Milliseconds to display. A frame the client marked with a negative gap
  /// is skipped entirely while animating, which the spec spells as `gap = 0`.
  pub gap: u32,
}

/// An image's playback state, as a renderer needs to see it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Playback {
  /// The frame the image is parked on, 0-based.
  pub current: usize,
  /// Whether frames should advance on their own.
  pub running: bool,
  /// Whether reaching the last frame wraps to the first.
  pub looping: bool,
  /// Loops left; `None` runs forever.
  pub loops: Option<u32>,
  /// Changes whenever the pixels do, so a texture cache can spot an edit.
  pub serial: u64,
}

/// What the animation is doing, set by `a=a,s=`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AnimState {
  /// `s=1`: parked on the current frame.
  Stopped,
  /// `s=2`: playing, but stopping at the last frame until a new one arrives.
  RunWait,
  /// `s=3`: playing and looping.
  RunLoop,
}

/// A stored image: its root frame plus any animation frames and playback
/// state. `frames[0]` is the root — the image as first transmitted.
#[derive(Debug, Clone)]
pub(crate) struct GfxImage {
  /// The `I=` number this image claimed, if any.
  pub number: u32,
  pub frames: Vec<Frame>,
  /// The frame playback is parked on, 0-based.
  pub current: usize,
  /// Remaining loops; `None` runs forever.
  pub loops: Option<u32>,
  pub state: AnimState,
  /// Bumped whenever the pixels change, so a renderer's texture cache can
  /// tell an edited image from the one it already uploaded.
  pub serial: u64,
  /// Decoded bytes this image holds across all its frames, kept alongside
  /// them rather than recounted. The pane budget is checked on every
  /// transmit, and walking every frame of every image to answer it made a
  /// stream of images quadratic in the number retained.
  bytes: usize,
}

impl GfxImage {
  pub(crate) fn new(number: u32, image: Image) -> Self {
    let bytes = image.rgba.len();
    Self {
      number,
      frames: vec![Frame {
        image,
        gap: DEFAULT_GAP_MS,
      }],
      current: 0,
      loops: None,
      state: AnimState::Stopped,
      serial: 0,
      bytes,
    }
  }

  /// Recount after the frames changed.
  fn resync(&mut self) {
    self.bytes = self.frames.iter().map(|f| f.image.rgba.len()).sum();
  }

  /// The root frame — what a still placement displays.
  pub fn root(&self) -> &Image {
    &self.frames[0].image
  }

  /// Whether this image has frames beyond the root.
  pub fn animated(&self) -> bool {
    self.frames.len() > 1
  }

}

/// Every transmitted image, keyed by id, with a number → id index alongside.
#[derive(Debug, Default)]
pub(crate) struct Store {
  images: HashMap<u32, GfxImage>,
  by_number: HashMap<u32, u32>,
  /// Where the search for a free id resumes. Client ids and terminal ids
  /// share one space, so allocation always probes for a vacancy.
  next_id: u32,
  /// Running total of [`GfxImage::bytes`], so the pane budget is an O(1)
  /// question. Every mutation goes through this type, which is what lets the
  /// total be maintained rather than recomputed.
  bytes: usize,
}

impl Store {
  /// The id a command names, whether by `i=` or by `I=`. `None` when it names
  /// nothing, or names a number no image has claimed.
  pub fn resolve(&self, control: &Control) -> Option<u32> {
    if control.image_id != 0 {
      return self.images.contains_key(&control.image_id).then_some(control.image_id);
    }
    if control.image_number != 0 {
      return self.by_number.get(&control.image_number).copied();
    }
    None
  }

  pub fn get(&self, id: u32) -> Option<&GfxImage> {
    self.images.get(&id)
  }

  pub fn get_mut(&mut self, id: u32) -> Option<&mut GfxImage> {
    self.images.get_mut(&id)
  }

  /// Edit an image's frames, keeping the byte total in step. Anything that
  /// changes how many frames an image has, or how big they are, belongs here
  /// rather than going through [`Store::get_mut`].
  pub fn edit<R>(&mut self, id: u32, f: impl FnOnce(&mut GfxImage) -> R) -> Option<R> {
    let image = self.images.get_mut(&id)?;
    let before = image.bytes;
    let out = f(image);
    image.resync();
    self.bytes = self.bytes + image.bytes - before;
    Some(out)
  }

  /// Decoded bytes across every stored image.
  pub fn bytes(&self) -> usize {
    self.bytes
  }

  /// Whether these pixels belong to a stored image, so a placement sharing
  /// them is already counted.
  pub fn holds(&self, id: u32) -> bool {
    self.images.contains_key(&id)
  }

  pub fn len(&self) -> usize {
    self.images.len()
  }

  pub fn ids(&self) -> impl Iterator<Item = u32> + '_ {
    self.images.keys().copied()
  }

  /// Store `image` under the id or number the command asks for, replacing any
  /// image already there. Returns the id it landed on.
  pub fn insert(&mut self, control: &Control, image: Image) -> u32 {
    let id = if control.image_id != 0 {
      control.image_id
    } else if control.image_number != 0 {
      // Reusing the number's existing id keeps a client that re-transmits
      // under one number from leaking a fresh id per frame.
      self
        .by_number
        .get(&control.image_number)
        .copied()
        .unwrap_or_else(|| self.allocate())
    } else {
      self.allocate()
    };
    if let Some(old) = self.images.remove(&id) {
      self.bytes -= old.bytes;
      if old.number != 0 && old.number != control.image_number {
        self.by_number.remove(&old.number);
      }
    }
    if control.image_number != 0 {
      self.by_number.insert(control.image_number, id);
    }
    let entry = GfxImage::new(control.image_number, image);
    self.bytes += entry.bytes;
    self.images.insert(id, entry);
    id
  }

  pub fn remove(&mut self, id: u32) -> Option<GfxImage> {
    let image = self.images.remove(&id)?;
    self.bytes -= image.bytes;
    if image.number != 0 {
      self.by_number.remove(&image.number);
    }
    Some(image)
  }

  pub fn clear(&mut self) {
    self.images.clear();
    self.by_number.clear();
    self.bytes = 0;
  }

  /// Any id not already taken. Terminal-allocated ids climb from 1 and wrap,
  /// probing past ids a client claimed for itself.
  fn allocate(&mut self) -> u32 {
    for _ in 0..=u32::MAX {
      self.next_id = self.next_id.checked_add(1).unwrap_or(1);
      if !self.images.contains_key(&self.next_id) {
        return self.next_id;
      }
    }
    1
  }
}

/// Add or replace one animation frame (`a=f`).
///
/// `data` is the frame *rectangle* — `control`'s `s`×`v` pixels landing at
/// `x`,`y` on the canvas. With `r=` it is composited onto that existing frame;
/// otherwise a new frame is appended, starting from either the `c=` base frame
/// or a solid `Y=` background.
pub(crate) fn add_frame(
  image: &mut GfxImage,
  control: &Control,
  rect: &Image,
) -> Result<(), GfxError> {
  let (cw, ch) = {
    let root = image.root();
    (root.width, root.height)
  };
  let gap = normalize_gap(control.gap());
  let target = control.frame() as usize;

  if target != 0 {
    // Editing an existing frame: composite the rectangle onto it in place.
    let frame = image.frames.get_mut(target - 1).ok_or(GfxError("ENOENT"))?;
    let mut canvas = frame.image.rgba.to_vec();
    blend(
      &mut canvas,
      cw,
      ch,
      rect,
      control.src_x as usize,
      control.src_y as usize,
      control.compose_overwrite(),
    );
    frame.image.rgba = canvas.into();
    if control.z != 0 {
      frame.gap = gap;
    }
    image.serial += 1;
    return Ok(());
  }

  if image.frames.len() >= MAX_FRAMES {
    return Err(GfxError("ENOSPC"));
  }
  let mut canvas = match control.base_frame() {
    0 => {
      let [r, g, b, a] = control.frame_background();
      let mut px = Vec::with_capacity(cw * ch * 4);
      for _ in 0..cw * ch {
        px.extend_from_slice(&[r, g, b, a]);
      }
      px
    }
    n => image
      .frames
      .get(n as usize - 1)
      .ok_or(GfxError("ENOENT"))?
      .image
      .rgba
      .to_vec(),
  };
  blend(
    &mut canvas,
    cw,
    ch,
    rect,
    control.src_x as usize,
    control.src_y as usize,
    control.compose_overwrite(),
  );
  image.frames.push(Frame {
    image: Image {
      width: cw,
      height: ch,
      rgba: canvas.into(),
    },
    gap,
  });
  image.serial += 1;
  Ok(())
}

/// Composite one stored frame's rectangle onto another (`a=c`).
pub(crate) fn compose_frames(image: &mut GfxImage, control: &Control) -> Result<(), GfxError> {
  let (dst_i, src_i) = (control.frame() as usize, control.base_frame() as usize);
  if dst_i == 0 || src_i == 0 || dst_i == src_i {
    return Err(GfxError("EINVAL"));
  }
  let source = image.frames.get(src_i - 1).ok_or(GfxError("ENOENT"))?;
  let (cw, ch) = (source.image.width, source.image.height);
  let (sx, sy) = (control.src_x as usize, control.src_y as usize);
  let sw = if control.src_w == 0 {
    cw.saturating_sub(sx)
  } else {
    control.src_w as usize
  };
  let sh = if control.src_h == 0 {
    ch.saturating_sub(sy)
  } else {
    control.src_h as usize
  };
  let patch = crop(&source.image, sx, sy, sw, sh);
  let (dx, dy) = control.dest_origin();
  let overwrite = control.compose_overwrite();
  let dest = image.frames.get_mut(dst_i - 1).ok_or(GfxError("ENOENT"))?;
  let (dw, dh) = (dest.image.width, dest.image.height);
  let mut canvas = dest.image.rgba.to_vec();
  blend(&mut canvas, dw, dh, &patch, dx as usize, dy as usize, overwrite);
  dest.image.rgba = canvas.into();
  image.serial += 1;
  Ok(())
}

/// Copy a rectangle out of an image, clipped to its bounds.
pub(crate) fn crop(image: &Image, x: usize, y: usize, w: usize, h: usize) -> Image {
  let w = w.min(image.width.saturating_sub(x));
  let h = h.min(image.height.saturating_sub(y));
  if w == 0 || h == 0 {
    return Image {
      width: 0,
      height: 0,
      rgba: Vec::new().into(),
    };
  }
  let mut out = Vec::with_capacity(w * h * 4);
  for row in y..y + h {
    let start = (row * image.width + x) * 4;
    out.extend_from_slice(&image.rgba[start..start + w * 4]);
  }
  Image {
    width: w,
    height: h,
    rgba: out.into(),
  }
}

/// Draw `patch` onto a `width`×`height` RGBA canvas at `(ox, oy)`, either
/// alpha-blending it over what is there or overwriting outright. Anything that
/// would land off-canvas is clipped.
fn blend(
  canvas: &mut [u8],
  width: usize,
  height: usize,
  patch: &Image,
  ox: usize,
  oy: usize,
  overwrite: bool,
) {
  for py in 0..patch.height {
    let y = oy + py;
    if y >= height {
      break;
    }
    for px in 0..patch.width {
      let x = ox + px;
      if x >= width {
        break;
      }
      let s = (py * patch.width + px) * 4;
      let d = (y * width + x) * 4;
      let src = &patch.rgba[s..s + 4];
      if overwrite || src[3] == 255 {
        canvas[d..d + 4].copy_from_slice(src);
      } else if src[3] != 0 {
        let sa = src[3] as u32;
        let da = canvas[d + 3] as u32;
        let out_a = sa + da * (255 - sa) / 255;
        for c in 0..3 {
          let sc = src[c] as u32 * sa;
          let dc = canvas[d + c] as u32 * da * (255 - sa) / 255;
          // A fully transparent result has no color to carry.
          canvas[d + c] = (sc + dc)
            .checked_div(out_a)
            .map_or(0, |v| v.min(255) as u8);
        }
        canvas[d + 3] = out_a.min(255) as u8;
      }
    }
  }
}

/// A frame's gap: unset takes the default, and a negative gap means the frame
/// is skipped while animating.
fn normalize_gap(gap: i32) -> u32 {
  match gap {
    0 => DEFAULT_GAP_MS,
    g if g < 0 => 0,
    g => g as u32,
  }
}

#[cfg(test)]
#[path = "../../tests/graphics/store.rs"]
mod tests;
