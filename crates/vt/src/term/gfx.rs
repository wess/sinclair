//! Kitty graphics command dispatch: everything the APC scanner hands over.
//!
//! One command is `<control> ; <payload>`. This module reassembles chunked
//! transfers, pulls the bytes off whichever medium the command names, decodes
//! them, and then does what the action asks — store, display, delete, edit an
//! animation frame, or answer a query — writing the protocol's response back
//! to the pty.

use crate::graphics::{self, Action, Control, GfxError};
use crate::image::{Kitty, Parent, Placement, Rect};

use super::Inner;

/// Ceiling on the bytes a chunked (`m=1`) transfer may accumulate before it is
/// abandoned. Without it a hostile stream can send `m=1` chunks forever and
/// grow `gfx_pending` without bound. Sized to admit the largest valid raw
/// transfer (25 MP * 4 channels).
const MAX_GFX_PENDING: usize = 128 * 1024 * 1024;

impl Inner {
  /// Handle one graphics command body — the part after `_G`. Reassembles
  /// chunked transfers (`m=1`) before acting.
  pub(crate) fn kitty_graphics(&mut self, data: &[u8]) {
    let mut parts = data.splitn(2, |&b| b == b';');
    let control = graphics::parse_control(parts.next().unwrap_or(&[]));
    let raw = super::report::base64_decode(parts.next().unwrap_or(&[])).unwrap_or_default();

    // Chunked: the first chunk carries the full control, the rest only `m=`
    // (plus payload). Accumulate until `m=0`, abandoning any transfer that
    // runs past the pending-size cap.
    if control.more {
      match &mut self.gfx_pending {
        Some((_, buf)) => {
          if buf.len().saturating_add(raw.len()) > MAX_GFX_PENDING {
            self.gfx_pending = None; // oversized transfer: drop it
          } else {
            buf.extend_from_slice(&raw);
          }
        }
        None if raw.len() <= MAX_GFX_PENDING => self.gfx_pending = Some((control, raw)),
        None => {} // first chunk already over budget: ignore
      }
      return;
    }
    // Only a command that carries `m=` continues a transfer. One that does
    // not is a command in its own right — running it with the *first*
    // chunk's control block, as joining blindly would, would silently turn
    // (say) a delete into the transmit that was in flight.
    let (control, raw) = match self.gfx_pending.take() {
      Some((first, mut buf)) if control.chunk => {
        buf.extend_from_slice(&raw);
        (first, buf)
      }
      _ => (control, raw),
    };
    let result = self.apply_graphics(&control, raw);
    self.gfx_respond(&control, result);
  }

  /// Act on a fully-assembled command, returning the id to name in the
  /// response (0 when the command named no image).
  fn apply_graphics(&mut self, control: &Control, raw: Vec<u8>) -> Result<u32, GfxError> {
    match control.action {
      Action::Query => {
        // A query validates without storing: clients use it to detect
        // support, so it must exercise the same decode path.
        let bytes = graphics::fetch(control, &raw)?;
        graphics::decode(control, &bytes)?;
        Ok(control.image_id)
      }
      Action::Transmit | Action::TransmitAndDisplay => {
        let bytes = graphics::fetch(control, &raw)?;
        let image = graphics::decode(control, &bytes)?;
        let id = self.gfx_store.insert(control, image);
        self.enforce_graphics_budget();
        if control.action == Action::TransmitAndDisplay {
          self.place_kitty(id, control)?;
        }
        Ok(id)
      }
      Action::Display => {
        let id = self.gfx_store.resolve(control).ok_or(GfxError("ENOENT"))?;
        self.place_kitty(id, control)?;
        Ok(id)
      }
      Action::Delete => {
        self.delete_graphics(control);
        Ok(control.image_id)
      }
      Action::Frame => {
        let id = self.gfx_store.resolve(control).ok_or(GfxError("ENOENT"))?;
        let bytes = graphics::fetch(control, &raw)?;
        let rect = graphics::decode(control, &bytes)?;
        self
          .gfx_store
          .edit(id, |image| graphics::add_frame(image, control, &rect))
          .ok_or(GfxError("ENOENT"))??;
        self.enforce_graphics_budget();
        self.damage_image(id);
        Ok(id)
      }
      Action::Compose => {
        let id = self.gfx_store.resolve(control).ok_or(GfxError("ENOENT"))?;
        self
          .gfx_store
          .edit(id, |image| graphics::compose_frames(image, control))
          .ok_or(GfxError("ENOENT"))??;
        self.damage_image(id);
        Ok(id)
      }
      Action::Animate => {
        let id = self.gfx_store.resolve(control).ok_or(GfxError("ENOENT"))?;
        let image = self.gfx_store.get_mut(id).ok_or(GfxError("ENOENT"))?;
        animate(image, control)?;
        self.damage_image(id);
        Ok(id)
      }
    }
  }

  /// Anchor a stored image on the grid per the command's display keys, or —
  /// for `U=1` — register it as a virtual placement the unicode placeholder
  /// character can call up anywhere.
  fn place_kitty(&mut self, id: u32, control: &Control) -> Result<(), GfxError> {
    let stored = self.gfx_store.get(id).ok_or(GfxError("ENOENT"))?;
    let image = stored.root().clone();
    let kitty = Kitty {
      image_id: id,
      placement_id: control.placement_id,
      src: Rect {
        x: control.src_x,
        y: control.src_y,
        w: control.src_w,
        h: control.src_h,
      },
      cell_x: control.cell_x,
      cell_y: control.cell_y,
      cols: control.cols,
      rows: control.rows,
      z: control.z,
      virtual_placement: control.unicode,
      parent: (control.parent_id != 0).then_some(Parent {
        image_id: control.parent_id,
        placement_id: control.parent_placement,
        h_offset: control.h_offset,
        v_offset: control.v_offset,
      }),
    };
    let seq = self.image_seq;
    self.image_seq += 1;
    let placement = Placement {
      id: seq,
      line: self.screen().cursor.row as isize,
      col: self.screen().cursor.col,
      image,
      kitty: Some(kitty),
    };

    // A placement id replaces the one it names rather than stacking on it.
    self.remove_placement(id, control.placement_id);

    if control.unicode {
      // Virtual placements have no anchor: placeholder cells decide where
      // they land, so they live apart from the grid-anchored list. They are
      // still charged to the pane budget — nothing about being unanchored
      // makes them free, and a client can ask for them in a loop.
      self.virt.push(placement);
      self.full_damage = true;
      self.enforce_graphics_budget();
      return Ok(());
    }

    let (cw, ch) = (self.cell_px.0 as usize, self.cell_px.1 as usize);
    let rows = placement.cell_rows(cw, ch);
    let cols = placement.cell_cols(cw, ch);
    {
      let scr = self.screen_mut();
      let start = scr.cursor.row;
      let end = (start + rows).min(scr.grid.rows());
      for r in start..end {
        scr.grid.damage_row(r);
      }
      scr.images.push(placement);
    }
    self.enforce_graphics_budget();
    if control.cursor_move {
      self.advance_past_image(rows, cols);
    }
    Ok(())
  }

  /// Put the cursor just past the image's bottom-right cell, which is where
  /// the protocol says a display leaves it. The rows are walked with real
  /// line feeds so the buffer scrolls (and the anchors ride up) exactly as if
  /// the image had been printed.
  fn advance_past_image(&mut self, rows: usize, cols: usize) {
    let start_col = self.screen().cursor.col;
    for _ in 0..rows.saturating_sub(1) {
      self.linefeed();
    }
    let width = self.screen().grid.cols();
    let target = start_col + cols;
    if target >= width {
      self.carriage_return();
      self.linefeed();
    } else {
      self.screen_mut().cursor.col = target;
      self.screen_mut().cursor.pending_wrap = false;
    }
  }

  /// Drop the placement a `p=` names, on either screen and among the virtual
  /// ones. A zero placement id names nothing, so nothing is removed.
  fn remove_placement(&mut self, image_id: u32, placement_id: u32) {
    if placement_id == 0 {
      return;
    }
    let matches = |p: &Placement| {
      p.kitty
        .as_ref()
        .is_some_and(|k| k.image_id == image_id && k.placement_id == placement_id)
    };
    self.primary.images.retain(|p| !matches(p));
    self.alt.images.retain(|p| !matches(p));
    self.virt.retain(|p| !matches(p));
    self.full_damage = true;
  }

  /// Damage every row an image's placements cover, after its pixels changed.
  fn damage_image(&mut self, id: u32) {
    let touches = self
      .screen()
      .images
      .iter()
      .any(|p| p.kitty.as_ref().is_some_and(|k| k.image_id == id));
    if touches || self.virt.iter().any(|p| p.kitty.as_ref().is_some_and(|k| k.image_id == id)) {
      self.full_damage = true;
    }
  }

  /// Remove placements (and, for an uppercase specifier, image data) per an
  /// `a=d` command. The full specifier table from the spec.
  fn delete_graphics(&mut self, control: &Control) {
    let free = control.delete.is_ascii_uppercase();
    let spec = control.delete.to_ascii_lowercase();
    let (cur_row, cur_col) = (self.screen().cursor.row, self.screen().cursor.col);
    let (cw, ch) = (self.cell_px.0 as usize, self.cell_px.1 as usize);
    let offset = self.display_offset;

    // `f` edits animation data rather than placements: it drops every frame
    // past the root, leaving the still image in place.
    if spec == b'f' {
      if let Some(id) = self.gfx_store.resolve(control) {
        self.gfx_store.edit(id, |image| {
          image.frames.truncate(1);
          image.current = 0;
          image.state = graphics::AnimState::Stopped;
          image.serial += 1;
        });
        self.damage_image(id);
      }
      return;
    }

    let by_id = self.gfx_store.resolve(control);
    let placement_id = control.placement_id;
    let keep = |p: &Placement| {
      let Some(k) = p.kitty.as_ref() else {
        // Sixel placements answer only to "delete everything".
        return spec != b'a';
      };
      if placement_id != 0 && k.placement_id != placement_id && matches!(spec, b'i' | b'n') {
        return true;
      }
      let rows = p.cell_rows(cw, ch) as isize;
      let cols = p.cell_cols(cw, ch) as isize;
      let top = p.line;
      let bottom = top + rows - 1;
      let left = p.col as isize;
      let right = left + cols - 1;
      // Delete coordinates are 1-based screen cells; convert to the
      // content-line space anchors live in.
      let cell_row = |y: u32| y.max(1) as isize - 1 - offset as isize;
      let cell_col = |x: u32| x.max(1) as isize - 1;
      let hit = match spec {
        b'a' => true,
        b'i' | b'n' => Some(k.image_id) == by_id,
        b'c' => {
          let r = cur_row as isize;
          let c = cur_col as isize;
          (top..=bottom).contains(&r) && (left..=right).contains(&c)
        }
        b'p' => {
          let r = cell_row(control.src_y);
          let c = cell_col(control.src_x);
          (top..=bottom).contains(&r) && (left..=right).contains(&c)
        }
        b'q' => {
          let r = cell_row(control.src_y);
          let c = cell_col(control.src_x);
          k.z == control.z && (top..=bottom).contains(&r) && (left..=right).contains(&c)
        }
        b'r' => {
          let (a, b) = (cell_row(control.src_x), cell_row(control.src_y));
          let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
          top <= hi && bottom >= lo
        }
        b'x' => {
          let c = cell_col(control.src_x);
          (left..=right).contains(&c)
        }
        b'y' => {
          let r = cell_row(control.src_y);
          (top..=bottom).contains(&r)
        }
        b'z' => k.z == control.z,
        _ => false,
      };
      !hit
    };

    let before = self.primary.images.len() + self.alt.images.len() + self.virt.len();
    self.primary.images.retain(keep);
    self.alt.images.retain(keep);
    self.virt.retain(keep);
    let removed = before > self.primary.images.len() + self.alt.images.len() + self.virt.len();

    if free {
      match spec {
        b'a' => self.gfx_store.clear(),
        b'i' | b'n' => {
          if let Some(id) = by_id {
            // An uppercase delete frees the data only once nothing draws it
            // any more, so a second placement keeps the image alive.
            if !self.image_is_placed(id) {
              self.gfx_store.remove(id);
            }
          }
        }
        _ => {
          let orphans: Vec<u32> = self
            .gfx_store
            .ids()
            .filter(|&id| !self.image_is_placed(id))
            .collect();
          for id in orphans {
            self.gfx_store.remove(id);
          }
        }
      }
    }
    if removed || free {
      self.full_damage = true;
    }
  }

  /// Whether any placement anywhere still draws this image.
  fn image_is_placed(&self, id: u32) -> bool {
    self
      .primary
      .images
      .iter()
      .chain(&self.alt.images)
      .chain(&self.virt)
      .any(|p| p.kitty.as_ref().is_some_and(|k| k.image_id == id))
  }

  /// Emit the response for a command, honouring the quiet level. A command
  /// that named no image gets none: the client did not ask to be told, and
  /// there would be nothing to name the reply by.
  fn gfx_respond(&mut self, control: &Control, result: Result<u32, GfxError>) {
    if !control.names_image() {
      return;
    }
    let id = match result {
      Ok(id) => id,
      Err(_) => control.image_id,
    };
    let suppress = match (&result, control.quiet) {
      (_, q) if q >= 2 => true,      // suppress all
      (Ok(_), q) if q >= 1 => true,  // suppress success
      _ => false,
    };
    if suppress {
      return;
    }
    let mut head = format!("i={id}");
    if control.image_number != 0 {
      head.push_str(&format!(",I={}", control.image_number));
    }
    if control.placement_id != 0 {
      head.push_str(&format!(",p={}", control.placement_id));
    }
    let status = match result {
      Ok(_) => "OK",
      Err(GfxError(code)) => code,
    };
    self.output.extend_from_slice(b"\x1b_G");
    self.output.extend_from_slice(head.as_bytes());
    self.output.push(b';');
    self.output.extend_from_slice(status.as_bytes());
    self.output.extend_from_slice(b"\x1b\\");
  }
}

/// Apply an `a=a` animation-control command: playback state, loop count, the
/// current frame, and a single frame's gap.
fn animate(image: &mut graphics::GfxImage, control: &Control) -> Result<(), GfxError> {
  match control.anim_state() {
    1 => image.state = graphics::AnimState::Stopped,
    2 => image.state = graphics::AnimState::RunWait,
    3 => image.state = graphics::AnimState::RunLoop,
    _ => {}
  }
  match control.anim_loops() {
    0 => {}
    1 => image.loops = None,
    n => image.loops = Some(n - 1),
  }
  if control.base_frame() != 0 {
    let idx = control.base_frame() as usize - 1;
    if idx >= image.frames.len() {
      return Err(GfxError("ENOENT"));
    }
    image.current = idx;
  }
  // `r` with a gap sets that one frame's gap; without one it is just a
  // selector for a future command, so it is left alone.
  if control.frame() != 0 && control.gap() != 0 {
    let idx = control.frame() as usize - 1;
    let frame = image.frames.get_mut(idx).ok_or(GfxError("ENOENT"))?;
    frame.gap = match control.gap() {
      g if g < 0 => 0,
      g => g as u32,
    };
  }
  image.serial += 1;
  Ok(())
}

#[cfg(test)]
#[path = "../../tests/term/gfx.rs"]
mod tests;
