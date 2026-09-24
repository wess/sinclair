//! Kitty graphics protocol: the `APC _G <control> ; <payload> ST` command
//! block — control-key parsing, payload decoding, media retrieval, and the
//! image/animation store.
//!
//! Mostly pure data; the byte-level APC capture and the placement/response
//! side effects live in `term::apc`. The one exception is [`media`], which
//! reads the files and shared-memory objects `t=f`/`t=t`/`t=s` name: the
//! protocol defines those transports, so a terminal that will not read them
//! does not implement the protocol.

mod decode;
mod media;
mod store;

pub use store::{Frame, Playback};

pub(crate) use decode::decode;
pub(crate) use media::fetch;
pub(crate) use store::{add_frame, compose_frames, AnimState, GfxImage, Store};
#[cfg(test)]
pub(crate) use store::MAX_FRAMES;

/// The parsed `a=` action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Action {
  /// `a=t`: transmit (store) the image only.
  Transmit,
  /// `a=T`: transmit and immediately display at the cursor.
  TransmitAndDisplay,
  /// `a=p`: display a previously transmitted image.
  Display,
  /// `a=d`: delete images and/or placements.
  Delete,
  /// `a=q`: query — validate without storing or displaying.
  Query,
  /// `a=f`: transmit one animation frame.
  Frame,
  /// `a=a`: animation control (play/stop, gaps, loops, current frame).
  Animate,
  /// `a=c`: compose one stored frame onto another.
  Compose,
}

/// The control keys of one graphics command. Unknown keys are ignored, as the
/// spec requires, so a newer client degrades instead of erroring.
///
/// Several keys are overloaded: their meaning depends on `action`. Fields are
/// named for the display meaning — the one they carry in the overwhelmingly
/// common `a=T`/`a=p` case — and the animation actions read them back through
/// the accessors below, so the parser never has to know the action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Control {
  pub action: Action,
  /// `q=`: 0 respond, 1 suppress OK, 2 suppress all.
  pub quiet: u8,

  // transmission
  /// `f=`: 24 (RGB), 32 (RGBA), 100 (PNG).
  pub format: u32,
  /// `t=`: `d` direct, `f` file, `t` temp file (deleted after), `s` POSIX
  /// shared memory (unlinked after).
  pub medium: u8,
  /// `o=z`: payload is zlib-compressed.
  pub compressed: bool,
  /// `m=1`: another chunk follows.
  pub more: bool,
  /// Whether `m=` appeared at all. A chunked transfer is continued only by
  /// commands that carry it, so an unrelated command arriving mid-transfer is
  /// executed as itself instead of being swallowed into the payload.
  pub chunk: bool,
  /// `s=` / `v=`: pixel width / height. Also the frame rectangle's size for
  /// `a=f`.
  pub width: u32,
  pub height: u32,
  /// `S=`: how many bytes to read from the medium (0 = to the end).
  pub size: u32,
  /// `O=`: byte offset to start reading the medium at.
  pub offset: u32,

  // identity
  /// `i=`: client-assigned image id (0 = unset).
  pub image_id: u32,
  /// `I=`: image *number* — the terminal assigns the id and reports it back
  /// (0 = unset). Later commands may name the newest image with this number.
  pub image_number: u32,
  /// `p=`: placement id (0 = unset).
  pub placement_id: u32,

  // display
  /// `x=`/`y=`: top-left of the source rectangle, in pixels. Also the frame
  /// rectangle's origin for `a=f`, the source origin for `a=c`, and the cell
  /// coordinates a `d=p`/`d=q`/`d=r`/`d=x`/`d=y` delete names.
  pub src_x: u32,
  pub src_y: u32,
  /// `w=`/`h=`: source rectangle size in pixels (0 = to the edge).
  pub src_w: u32,
  pub src_h: u32,
  /// `X=`/`Y=`: offset *within* the first cell, in pixels. Also the
  /// composition mode and frame background for `a=f`, and the destination
  /// origin for `a=c`.
  pub cell_x: u32,
  pub cell_y: u32,
  /// `c=`/`r=`: the cell box to scale into (0 = derive from pixel size).
  /// Also the base/source frame for `a=f`/`a=c` and the frame `a=a` edits.
  pub cols: u32,
  pub rows: u32,
  /// `z=`: stacking order. Negative draws under the text; below
  /// [`Z_BELOW_BG`] it also goes under the cell background. Also the frame
  /// gap in milliseconds for `a=f`/`a=a`.
  pub z: i32,
  /// `C=`: 0 moves the cursor below the image, 1 leaves it put. Also
  /// the composition mode for `a=c`.
  pub cursor_move: bool,
  /// `U=1`: a *virtual* placement, drawn wherever the unicode placeholder
  /// character U+10EEEE names it rather than at the cursor.
  pub unicode: bool,
  /// `P=`/`Q=`: the image and placement this one is positioned relative to.
  pub parent_id: u32,
  pub parent_placement: u32,
  /// `H=`/`V=`: cell offset from the parent placement.
  pub h_offset: i32,
  pub v_offset: i32,

  /// `d=`: which placements (and, when uppercase, images) a delete removes.
  pub delete: u8,
}

/// A placement at or below this z-index is drawn under the cell background,
/// not merely under the text. The spec's cut-off.
pub const Z_BELOW_BG: i32 = -1_073_741_824;

impl Default for Control {
  fn default() -> Self {
    Self {
      action: Action::Transmit,
      quiet: 0,
      format: 32,
      medium: b'd',
      compressed: false,
      more: false,
      chunk: false,
      width: 0,
      height: 0,
      size: 0,
      offset: 0,
      image_id: 0,
      image_number: 0,
      placement_id: 0,
      src_x: 0,
      src_y: 0,
      src_w: 0,
      src_h: 0,
      cell_x: 0,
      cell_y: 0,
      cols: 0,
      rows: 0,
      z: 0,
      cursor_move: true,
      unicode: false,
      parent_id: 0,
      parent_placement: 0,
      h_offset: 0,
      v_offset: 0,
      delete: b'a',
    }
  }
}

impl Control {
  /// `r=`: the 1-based frame this command creates or edits (0 = append a new
  /// one for `a=f`, or "leave alone" for `a=a`).
  pub fn frame(&self) -> u32 {
    self.rows
  }

  /// `c=`: the 1-based frame a new one is composed from, or that `a=a` makes
  /// current (0 = unset).
  pub fn base_frame(&self) -> u32 {
    self.cols
  }

  /// `z=`: the frame gap in milliseconds. Negative means "this frame is
  /// skipped when animating"; the spec stores the absolute value and marks it
  /// unrendered, and a gap of 0 becomes the default.
  pub fn gap(&self) -> i32 {
    self.z
  }

  /// `X=` under `a=f`/`a=c`: 0 alpha-blends the source over the destination,
  /// 1 overwrites it (alpha included).
  pub fn compose_overwrite(&self) -> bool {
    match self.action {
      // `a=c` spells the mode with `C`, `a=f` with `X`.
      Action::Compose => !self.cursor_move,
      _ => self.cell_x == 1,
    }
  }

  /// `Y=` under `a=f`: the RGBA background a newly created frame starts from.
  pub fn frame_background(&self) -> [u8; 4] {
    self.cell_y.to_be_bytes()
  }

  /// `X=`/`Y=` under `a=c`: where in the destination frame the source lands.
  pub fn dest_origin(&self) -> (u32, u32) {
    (self.cell_x, self.cell_y)
  }

  /// `s=` under `a=a`: 1 stop, 2 run but wait for a new frame when the loop
  /// ends, 3 run and loop.
  pub fn anim_state(&self) -> u32 {
    self.width
  }

  /// `v=` under `a=a`: how many times the animation repeats (0 = unchanged,
  /// 1 = loop forever, n = loop n-1 times, matching the spec's off-by-one).
  pub fn anim_loops(&self) -> u32 {
    self.height
  }

  /// Whether this command names an image at all. A response with no name
  /// would be meaningless, so those are suppressed.
  pub fn names_image(&self) -> bool {
    self.image_id != 0 || self.image_number != 0
  }
}

/// Parse the comma-separated `key=value` control block (the part before `;`).
pub(crate) fn parse_control(bytes: &[u8]) -> Control {
  let mut c = Control::default();
  for pair in bytes.split(|&b| b == b',') {
    let mut kv = pair.splitn(2, |&b| b == b'=');
    let key = kv.next().unwrap_or(&[]);
    let val = kv.next().unwrap_or(&[]);
    match key {
      b"a" => {
        c.action = match val.first() {
          Some(b'T') => Action::TransmitAndDisplay,
          Some(b'p') => Action::Display,
          Some(b'd') => Action::Delete,
          Some(b'q') => Action::Query,
          Some(b'f') => Action::Frame,
          Some(b'a') => Action::Animate,
          Some(b'c') => Action::Compose,
          _ => Action::Transmit,
        }
      }
      b"q" => c.quiet = uint(val) as u8,
      b"f" => c.format = uint(val),
      b"t" => c.medium = val.first().copied().unwrap_or(b'd'),
      b"o" => c.compressed = val.first() == Some(&b'z'),
      b"m" => {
        c.more = uint(val) == 1;
        c.chunk = true;
      }
      b"s" => c.width = uint(val),
      b"v" => c.height = uint(val),
      b"S" => c.size = uint(val),
      b"O" => c.offset = uint(val),
      b"i" => c.image_id = uint(val),
      b"I" => c.image_number = uint(val),
      b"p" => c.placement_id = uint(val),
      b"x" => c.src_x = uint(val),
      b"y" => c.src_y = uint(val),
      b"w" => c.src_w = uint(val),
      b"h" => c.src_h = uint(val),
      b"X" => c.cell_x = uint(val),
      b"Y" => c.cell_y = uint(val),
      b"c" => c.cols = uint(val),
      b"r" => c.rows = uint(val),
      b"z" => c.z = int(val),
      b"C" => c.cursor_move = uint(val) == 0,
      b"U" => c.unicode = uint(val) == 1,
      b"P" => c.parent_id = uint(val),
      b"Q" => c.parent_placement = uint(val),
      b"H" => c.h_offset = int(val),
      b"V" => c.v_offset = int(val),
      b"d" => c.delete = val.first().copied().unwrap_or(b'a'),
      _ => {}
    }
  }
  c
}

/// Parse an unsigned decimal field, defaulting to 0 on anything unexpected.
fn uint(bytes: &[u8]) -> u32 {
  std::str::from_utf8(bytes)
    .ok()
    .and_then(|s| s.trim().parse().ok())
    .unwrap_or(0)
}

/// Parse a signed decimal field (`z`, `H`, `V`), saturating rather than
/// wrapping so a huge z-index still sorts to the right end.
fn int(bytes: &[u8]) -> i32 {
  let s = match std::str::from_utf8(bytes) {
    Ok(s) => s.trim(),
    Err(_) => return 0,
  };
  match s.parse::<i64>() {
    Ok(v) => v.clamp(i32::MIN as i64, i32::MAX as i64) as i32,
    Err(_) => 0,
  }
}

/// A failure, carrying the kitty error code the response reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GfxError(pub &'static str);

#[cfg(test)]
#[path = "../../tests/graphics/mod.rs"]
mod tests;
