//! A terminal's whole visible state written back out as the bytes that would
//! rebuild it, for a client attaching to a session another process owns.
//!
//! Fed into a fresh terminal of the same size, a snapshot reproduces the
//! scrollback, both screens and which one is showing, the cursor (position,
//! visibility, shape, pen), the scroll region, the input modes a program set
//! (cursor keys, keypad, mouse reporting, bracketed paste, focus reporting,
//! the kitty keyboard flags), the title and working directory, palette and
//! cursor-color overrides, and program status records.
//!
//! It deliberately leaves out what only makes sense in the process that
//! parsed it the first time: images, hyperlink targets, the DECSC saved
//! cursor, the title stack, character sets, and custom tab stops. Unlike
//! [`Terminal::buffer_dump`] it is exact, not re-wrapped: the receiver is the
//! same width, so rows land where they were.

use super::Terminal;
use crate::cursor::CursorStyle;
use crate::grid::Grid;
use crate::mode::Modes;

impl Terminal {
  /// The bytes that rebuild this terminal's state in a fresh terminal of the
  /// same size (see the module docs for what is and isn't carried).
  pub fn snapshot(&mut self) -> Vec<u8> {
    let mut out = String::new();
    let alt = self.inner.modes.contains(Modes::ALT_SCREEN);

    write_screen(&mut self.inner.primary.grid, true, &mut out);
    if alt {
      // The primary cursor goes where 1049 will save it from.
      let c = &self.inner.primary.cursor;
      out.push_str(&format!("\x1b[{};{}H", c.row + 1, c.col + 1));
      kitty(self.inner.primary.kitty.current(), &mut out);
      // 1049 clears the alternate screen but leaves the cursor where it was.
      out.push_str("\x1b[?1049h\x1b[H");
      write_screen(&mut self.inner.alt.grid, false, &mut out);
    }
    let screen = if alt {
      &self.inner.alt
    } else {
      &self.inner.primary
    };
    kitty(screen.kitty.current(), &mut out);

    // Scroll region, then the cursor inside it.
    let rows = screen.grid.rows();
    if screen.scroll_top != 0 || screen.scroll_bottom + 1 != rows {
      out.push_str(&format!(
        "\x1b[{};{}r",
        screen.scroll_top + 1,
        screen.scroll_bottom + 1
      ));
    }
    let modes = self.inner.modes;
    let cursor = &screen.cursor;
    if modes.contains(Modes::ORIGIN) {
      // DECOM homes the cursor and makes CUP region-relative.
      out.push_str("\x1b[?6h");
      out.push_str(&format!(
        "\x1b[{};{}H",
        cursor.row.saturating_sub(screen.scroll_top) + 1,
        cursor.col + 1
      ));
    } else {
      out.push_str(&format!("\x1b[{};{}H", cursor.row + 1, cursor.col + 1));
    }

    write_modes(modes, &mut out);
    out.push_str(&format!("\x1b[{} q", decscusr(self.inner.cursor_style)));

    if !self.inner.title.is_empty() {
      out.push_str(&format!("\x1b]2;{}\x1b\\", self.inner.title));
    }
    if let Some(cwd) = &self.inner.cwd {
      out.push_str(&format!("\x1b]7;{cwd}\x1b\\"));
    }
    for (i, rgb) in self.inner.palette.iter().enumerate() {
      if let Some((r, g, b)) = rgb {
        out.push_str(&format!("\x1b]4;{i};rgb:{r:02x}/{g:02x}/{b:02x}\x1b\\"));
      }
    }
    if let Some((r, g, b)) = self.inner.cursor_color {
      out.push_str(&format!("\x1b]12;rgb:{r:02x}/{g:02x}/{b:02x}\x1b\\"));
    }
    for record in self.inner.status.records() {
      out.push_str(&status_report(record));
    }

    // The pen last: everything above that wrote text reset it.
    let pen = cursor.pen;
    out.push_str("\x1b[0m");
    crate::sgr::emit(&pen, &mut out);
    out.into_bytes()
  }
}

/// Every row of `grid` — scrollback first when `history` — leaving the
/// cursor on the last row, not past it.
fn write_screen(grid: &mut Grid, history: bool, out: &mut String) {
  let sb_len = if history { grid.scrollback().len() } else { 0 };
  let total = sb_len + grid.rows();
  let start = out.len();
  for i in 0..total {
    let row = if i < sb_len {
      grid.scrollback_mut().row(i)
    } else {
      Some(grid.row(i - sb_len))
    };
    match row {
      Some(row) => crate::dump::write_row(row, out),
      None => out.push_str("\r\n"),
    }
  }
  // The last row's newline would scroll the top row away.
  if out[start..].ends_with("\r\n") {
    out.truncate(out.len() - 2);
  }
}

fn kitty(flags: u8, out: &mut String) {
  if flags != 0 {
    out.push_str(&format!("\x1b[={flags};1u"));
  }
}

fn write_modes(modes: Modes, out: &mut String) {
  let mut set = |on: bool, seq: &str| {
    if on {
      out.push_str(seq);
    }
  };
  set(modes.contains(Modes::APP_CURSOR), "\x1b[?1h");
  set(modes.contains(Modes::APP_KEYPAD), "\x1b=");
  set(!modes.contains(Modes::AUTOWRAP), "\x1b[?7l");
  set(!modes.contains(Modes::CURSOR_VISIBLE), "\x1b[?25l");
  set(modes.contains(Modes::INSERT), "\x1b[4h");
  set(modes.contains(Modes::BRACKETED_PASTE), "\x1b[?2004h");
  set(modes.contains(Modes::MOUSE_CLICK), "\x1b[?1000h");
  set(modes.contains(Modes::MOUSE_DRAG), "\x1b[?1002h");
  set(modes.contains(Modes::MOUSE_MOTION), "\x1b[?1003h");
  set(modes.contains(Modes::MOUSE_SGR), "\x1b[?1006h");
  set(!modes.contains(Modes::ALT_SCROLL), "\x1b[?1007l");
  set(modes.contains(Modes::FOCUS_REPORT), "\x1b[?1004h");
  set(modes.contains(Modes::BACKARROW_BS), "\x1b[?67h");
  set(modes.contains(Modes::CLIPBOARD_EVENTS), "\x1b[?5522h");
}

fn decscusr(style: CursorStyle) -> u8 {
  match style {
    CursorStyle::BlinkingBlock => 1,
    CursorStyle::SteadyBlock => 2,
    CursorStyle::BlinkingUnderline => 3,
    CursorStyle::SteadyUnderline => 4,
    CursorStyle::BlinkingBar => 5,
    CursorStyle::SteadyBar => 6,
  }
}

/// One status record as the OSC 7501 report that recreates it.
fn status_report(r: &crate::status::Record) -> String {
  let mut pairs = vec![format!("state={}", r.state.label())];
  if !r.id.is_empty() {
    pairs.push(format!("id={}", r.id));
  }
  if let Some(k) = r.kind {
    pairs.push(format!("kind={}", k.label()));
  }
  if let Some(p) = r.progress {
    pairs.push(format!("progress={p}"));
  }
  if let Some(a) = &r.app {
    pairs.push(format!("app={a}"));
  }
  if let Some(t) = &r.title {
    pairs.push(format!("title={}", base64(t.as_bytes())));
  }
  if let Some(m) = &r.msg {
    pairs.push(format!("msg={}", base64(m.as_bytes())));
  }
  format!("\x1b]7501;{}\x1b\\", pairs.join(":"))
}

fn base64(bytes: &[u8]) -> String {
  const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
  let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
  for chunk in bytes.chunks(3) {
    let n = chunk
      .iter()
      .enumerate()
      .fold(0u32, |acc, (i, &b)| acc | (b as u32) << (16 - 8 * i));
    for i in 0..4 {
      if i <= chunk.len() {
        out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
      } else {
        out.push('=');
      }
    }
  }
  out
}

#[cfg(test)]
#[path = "../../tests/term/snapshot.rs"]
mod tests;
