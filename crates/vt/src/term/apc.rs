//! Byte-level APC pre-parser for the kitty graphics protocol.
//!
//! vte (0.15) routes `APC` strings (`ESC _ … ST`) into a discarded internal
//! state — it never surfaces them to `Perform` — so [`Terminal::feed`] runs this
//! scanner first: normal byte runs are forwarded to vte unchanged, while an
//! `ESC _ … ST/BEL` block is captured and, when it is a graphics command
//! (`_G…`), handled here. Because `0x1b` (ESC) is always an escape introducer
//! and never a payload byte in a well-formed stream, `ESC _` is an unambiguous
//! APC start, so the scanner needs no knowledge of vte's own state.
//!
//! [`Terminal::feed`]: super::Terminal::feed

use super::Inner;
use crate::graphics::{self, Action, Control};

const ESC: u8 = 0x1b;
const BEL: u8 = 0x07;

/// Largest APC body buffered; a graphics control block plus one base64 chunk is
/// far smaller. An unterminated APC is dropped past this cap.
const MAX_APC: usize = 4 * 1024 * 1024;

/// Ceiling on the bytes a chunked (`m=1`) transfer may accumulate before it is
/// abandoned. Without it a hostile stream can send `m=1` chunks forever and
/// grow `gfx_pending` without bound. Sized to admit the largest valid raw
/// transfer (`MAX_PIXELS` * 4 channels).
const MAX_GFX_PENDING: usize = 128 * 1024 * 1024;

/// Ceiling on the total bytes retained across all transmitted-but-not-displayed
/// images. Past it, other stored entries are evicted, so a flood of `a=t`
/// transmissions with distinct ids can't hold unbounded memory.
const MAX_GFX_STORE: usize = 320 * 1024 * 1024;

/// Scanner state, carried on [`Inner`] across `feed` calls — an APC block or a
/// bare trailing ESC can straddle pty reads.
#[derive(Debug, Default)]
pub(crate) struct Apc {
    state: State,
    buf: Vec<u8>,
}

#[derive(Debug, Default, PartialEq, Eq)]
enum State {
    #[default]
    Ground,
    Esc,
    Body,
    BodyEsc,
}

/// Drive `bytes` through the scanner: forward non-APC runs to `parser`, capture
/// and dispatch APC blocks. Plain output — the overwhelmingly common case — is
/// handed to vte as sub-slices of `bytes` with no copy, and `memchr` skips to
/// the next `ESC` in one SIMD pass, so a graphics-free stream costs a scan and
/// a single `advance`.
pub(crate) fn advance(parser: &mut vte::Parser, inner: &mut Inner, bytes: &[u8]) {
    // Start of the pending run of plain bytes bound for vte; forwarded whole.
    let mut run_start = 0;
    let mut i = 0;

    // An ESC held from the end of the previous feed is not part of `bytes`, so
    // resolve it against the first byte before the main scan.
    if inner.apc.state == State::Esc {
        match bytes.first() {
            None => return, // nothing new yet; keep holding the ESC
            Some(&b'_') => {
                // `ESC _` straddling the feed boundary: the APC begins here.
                inner.apc.buf.clear();
                inner.apc.state = State::Body;
                i = 1;
                run_start = 1;
            }
            Some(_) => {
                // The held ESC introduced something else; forward it on its own,
                // then scan `bytes` from Ground (the byte is re-read below).
                parser.advance(inner, &[ESC]);
                inner.apc.state = State::Ground;
            }
        }
    }

    while i < bytes.len() {
        match inner.apc.state {
            State::Ground => match memchr::memchr(ESC, &bytes[i..]) {
                // Everything up to and including the ESC stays in the pending
                // run; only the ESC's role (APC start or not) is still unknown.
                Some(off) => {
                    i += off + 1; // past the ESC
                    inner.apc.state = State::Esc;
                }
                None => i = bytes.len(), // rest of the buffer is plain
            },
            State::Esc => {
                // The ESC sits at `i - 1`, still inside the pending run.
                if bytes[i] == b'_' {
                    // APC begins. Forward the run up to (not including) the ESC
                    // first, so the image anchors after any preceding text.
                    let esc = i - 1;
                    if run_start < esc {
                        parser.advance(inner, &bytes[run_start..esc]);
                    }
                    inner.apc.buf.clear();
                    inner.apc.state = State::Body;
                    i += 1;
                    run_start = i;
                } else {
                    // A real ESC for some other sequence: it stays in the run
                    // and we re-read this byte in Ground.
                    inner.apc.state = State::Ground;
                }
            }
            State::Body => {
                match bytes[i] {
                    ESC => inner.apc.state = State::BodyEsc,
                    BEL => {
                        finish(inner); // BEL terminates the APC
                        run_start = i + 1;
                    }
                    b => push_body(inner, b),
                }
                i += 1;
            }
            State::BodyEsc => {
                if bytes[i] == b'\\' {
                    finish(inner); // ST terminates the APC
                    run_start = i + 1;
                    i += 1;
                } else {
                    // An ESC in the body that isn't ST: keep it as data and
                    // re-read this byte in the body state.
                    push_body(inner, ESC);
                    inner.apc.state = State::Body;
                }
            }
        }
    }

    // Forward the trailing plain run. A still-held ESC is the last byte and must
    // wait for the next feed, so it is excluded; inside an APC body nothing is
    // pending (those bytes were captured, not forwarded).
    let end = match inner.apc.state {
        State::Ground => bytes.len(),
        State::Esc => bytes.len() - 1,
        State::Body | State::BodyEsc => run_start,
    };
    if run_start < end {
        parser.advance(inner, &bytes[run_start..end]);
    }
}

fn push_body(inner: &mut Inner, b: u8) {
    if inner.apc.buf.len() < MAX_APC {
        inner.apc.buf.push(b);
    }
}

/// Terminate the current APC block and dispatch it. Graphics commands start
/// with `G`; any other APC is discarded (as vte would).
fn finish(inner: &mut Inner) {
    inner.apc.state = State::Ground;
    let buf = std::mem::take(&mut inner.apc.buf);
    if buf.first() == Some(&b'G') {
        inner.kitty_graphics(&buf[1..]);
    }
}

impl Inner {
    /// Handle one graphics command body — `<control> ; <base64 payload>`, the
    /// part after `_G`. Reassembles chunked transfers (`m=1`) before decoding.
    fn kitty_graphics(&mut self, data: &[u8]) {
        let mut parts = data.splitn(2, |&b| b == b';');
        let control = graphics::parse_control(parts.next().unwrap_or(&[]));
        let raw = super::report::base64_decode(parts.next().unwrap_or(&[])).unwrap_or_default();

        // Chunked: the first chunk carries the full control, the rest only
        // `m=` (+ payload). Accumulate the base64-decoded bytes until `m=0`,
        // abandoning any transfer that runs past the pending-size cap.
        if control.more {
            match &mut self.gfx_pending {
                Some((_, buf)) => {
                    if buf.len().saturating_add(raw.len()) > MAX_GFX_PENDING {
                        self.gfx_pending = None; // oversized transfer: drop it
                    } else {
                        buf.extend_from_slice(&raw);
                    }
                }
                None if raw.len() <= MAX_GFX_PENDING => {
                    self.gfx_pending = Some((control, raw))
                }
                None => {} // first chunk already over budget: ignore
            }
            return;
        }
        let (control, raw) = match self.gfx_pending.take() {
            Some((first, mut buf)) => {
                buf.extend_from_slice(&raw);
                (first, buf)
            }
            None => (control, raw),
        };
        self.apply_graphics(control, raw);
    }

    /// Act on a fully-assembled graphics command.
    fn apply_graphics(&mut self, control: Control, raw: Vec<u8>) {
        match control.action {
            Action::Delete => {
                self.delete_graphics(&control);
                self.gfx_respond(&control, Ok(()));
            }
            Action::Query => {
                let result = graphics::decode(&control, &raw).map(|_| ());
                self.gfx_respond(&control, result);
            }
            Action::Transmit => match graphics::decode(&control, &raw) {
                Ok(img) => {
                    if control.image_id != 0 {
                        self.store_image(control.image_id, img);
                    }
                    self.gfx_respond(&control, Ok(()));
                }
                Err(e) => self.gfx_respond(&control, Err(e)),
            },
            Action::TransmitAndDisplay => match graphics::decode(&control, &raw) {
                Ok(img) => {
                    if control.image_id != 0 {
                        self.store_image(control.image_id, img.clone());
                    }
                    self.place_image(img, control.image_id, control.move_cursor);
                    self.gfx_respond(&control, Ok(()));
                }
                Err(e) => self.gfx_respond(&control, Err(e)),
            },
            Action::Display => match self.gfx_store.get(&control.image_id).cloned() {
                Some(img) => {
                    self.place_image(img, control.image_id, control.move_cursor);
                    self.gfx_respond(&control, Ok(()));
                }
                None => self.gfx_respond(&control, Err(graphics::GfxError("ENOENT"))),
            },
        }
    }

    /// Retain a transmitted image under `id`, evicting other stored entries once
    /// the total retained bytes would exceed [`MAX_GFX_STORE`]. Bounds memory
    /// against a flood of `a=t` transmissions carrying distinct ids. Eviction
    /// order is arbitrary — the goal is to cap memory, not to keep any one
    /// image; an app whose image was dropped simply re-transmits it.
    fn store_image(&mut self, id: u32, img: crate::sixel::Image) {
        if img.rgba.len() > MAX_GFX_STORE {
            return; // a single image over the whole budget is never retained
        }
        self.gfx_store.insert(id, img);
        let mut total: usize = self.gfx_store.values().map(|i| i.rgba.len()).sum();
        if total <= MAX_GFX_STORE {
            return;
        }
        let victims: Vec<u32> = self
            .gfx_store
            .keys()
            .copied()
            .filter(|&k| k != id)
            .collect();
        for k in victims {
            if total <= MAX_GFX_STORE {
                break;
            }
            if let Some(evicted) = self.gfx_store.remove(&k) {
                total -= evicted.rgba.len();
            }
        }
    }

    /// Remove placements per an `a=d` command. Uppercase specifiers also free
    /// the stored image data; lowercase keep it for later re-display.
    fn delete_graphics(&mut self, control: &Control) {
        let free = control.delete.is_ascii_uppercase();
        match control.delete.to_ascii_lowercase() {
            b'i' => {
                let id = control.image_id;
                self.screen_mut().images.retain(|p| p.kitty_id != Some(id));
                if free {
                    self.gfx_store.remove(&id);
                }
            }
            // `a` (all) or any unrecognized specifier: clear every placement.
            _ => {
                self.screen_mut().images.clear();
                if free {
                    self.gfx_store.clear();
                }
            }
        }
        self.full_damage = true;
    }

    /// Emit the kitty response for a command, honoring the quiet level. Nothing
    /// is sent when the request carried no image id (there is nothing to name).
    fn gfx_respond(&mut self, control: &Control, result: Result<(), graphics::GfxError>) {
        if control.image_id == 0 {
            return;
        }
        let suppress = match (&result, control.quiet) {
            (_, q) if q >= 2 => true,          // suppress all
            (Ok(()), q) if q >= 1 => true,     // suppress success
            _ => false,
        };
        if suppress {
            return;
        }
        let status = match result {
            Ok(()) => "OK",
            Err(graphics::GfxError(code)) => code,
        };
        self.output.extend_from_slice(b"\x1b_G");
        self.output
            .extend_from_slice(format!("i={}", control.image_id).as_bytes());
        if control.placement_id != 0 {
            self.output
                .extend_from_slice(format!(",p={}", control.placement_id).as_bytes());
        }
        self.output.push(b';');
        self.output.extend_from_slice(status.as_bytes());
        self.output.extend_from_slice(b"\x1b\\");
    }
}

#[cfg(test)]
#[path = "../../tests/term/apc.rs"]
mod tests;
