//! Byte-level APC pre-parser for the kitty graphics protocol.
//!
//! vte (0.15) routes `APC` strings (`ESC _ … ST`) into a discarded internal
//! state — it never surfaces them to `Perform` — so [`Terminal::feed`] runs this
//! scanner first: normal byte runs are forwarded to vte unchanged, while an
//! `ESC _ … ST/BEL` block is captured and, when it is a graphics command
//! (`_G…`), handed to `term::graphics`. Because `0x1b` (ESC) is always an
//! escape introducer
//! and never a payload byte in a well-formed stream, `ESC _` is an unambiguous
//! APC start, so the scanner needs no knowledge of vte's own state.
//!
//! [`Terminal::feed`]: super::Terminal::feed

use super::Inner;

const ESC: u8 = 0x1b;
const BEL: u8 = 0x07;

/// Largest APC body buffered; a graphics control block plus one base64 chunk is
/// far smaller. An unterminated APC is dropped past this cap.
const MAX_APC: usize = 4 * 1024 * 1024;

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
      Some(&ESC) => {
        // A second ESC supersedes the held one — vte discards a restarted
        // escape too — and this one may itself introduce an APC. Handing
        // the held ESC to vte here would leave it mid-escape with the APC
        // body withheld, so it would eat the byte after the block (see
        // `trim_dangling_esc`). Drop it and rescan from Ground.
        inner.apc.state = State::Ground;
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
          let end = trim_dangling_esc(bytes, run_start, esc);
          if run_start < end {
            parser.advance(inner, &bytes[run_start..end]);
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

/// End of the run to forward before an APC that starts at `esc`, with any
/// trailing ESCs dropped.
///
/// vte only ever resolves an ESC by reading the byte after it — and the APC's
/// bytes are captured here, never forwarded. So handing vte a run ending in ESC
/// parks it mid-escape, and the next byte it sees (the first one *after* the
/// APC block) is consumed as that escape's final byte: `…ESC ESC _X…ST H`
/// silently loses the `H` to an `ESC H`. Those ESCs are superseded by the APC's
/// own ESC, which vte discards anyway, so dropping them matches what vte would
/// have done had it seen the whole stream.
fn trim_dangling_esc(bytes: &[u8], run_start: usize, esc: usize) -> usize {
  let mut end = esc;
  while end > run_start && bytes[end - 1] == ESC {
    end -= 1;
  }
  end
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

#[cfg(test)]
#[path = "../../tests/term/apc.rs"]
mod tests;
