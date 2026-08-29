//! A single grid cell: character, colors, and attribute flags.

use crate::color::Color;
use crate::hyperlink::HyperlinkId;
use crate::placeholder::{Placeholder, PLACEHOLDER};

bitflags::bitflags! {
    /// Rendering attributes for a cell.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub struct CellFlags: u16 {
        const BOLD             = 1 << 0;
        const DIM              = 1 << 1;
        const ITALIC           = 1 << 2;
        const UNDERLINE        = 1 << 3;
        const DOUBLE_UNDERLINE = 1 << 4;
        const CURLY_UNDERLINE  = 1 << 5;
        const DOTTED_UNDERLINE = 1 << 6;
        const DASHED_UNDERLINE = 1 << 7;
        const STRIKETHROUGH    = 1 << 8;
        const INVERSE          = 1 << 9;
        const INVISIBLE        = 1 << 10;
        const BLINK            = 1 << 11;
        /// First half of a wide (2-column) character.
        const WIDE             = 1 << 12;
        /// Second half of a wide character; renders as nothing.
        const WIDE_SPACER      = 1 << 13;
    }
}

impl CellFlags {
  /// Every underline style; useful for clearing them all at once.
  pub const ANY_UNDERLINE: CellFlags = CellFlags::UNDERLINE
    .union(CellFlags::DOUBLE_UNDERLINE)
    .union(CellFlags::CURLY_UNDERLINE)
    .union(CellFlags::DOTTED_UNDERLINE)
    .union(CellFlags::DASHED_UNDERLINE);
}

/// One terminal cell. Copy; the grid stores these densely.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
  pub ch: char,
  pub fg: Color,
  pub bg: Color,
  pub underline_color: Color,
  pub flags: CellFlags,
  /// OSC 8 hyperlink this cell belongs to, if any (see
  /// [`crate::hyperlink`]). The `NonZeroU16` niche keeps this 2 bytes.
  pub hyperlink: Option<HyperlinkId>,
  /// The zero-width payload that follows `ch`: either one combining mark, or,
  /// for a kitty placeholder cell, the image coordinates its marks spelled.
  /// Inline so the cell stays `Copy`, small, and rides along through scroll
  /// and reflow.
  pub zw: ZeroWidth,
}

/// A cell's zero-width payload, packed into the space one `char` used to take.
///
/// Almost every cell that has one has a single combining mark, and that is
/// stored as its codepoint. A kitty placeholder cell instead stores its
/// *decoded* row, column, and image-id high byte — decoding on write rather
/// than on every frame, and fitting three values where a `char` would only
/// have held one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ZeroWidth(u32);

/// Set for a placeholder payload. Codepoints stop at U+10FFFF, so the top bit
/// is free to tell the two apart.
const PLACEHOLDER_TAG: u32 = 1 << 31;
/// Each of the three placeholder fields is stored biased by one, so zero
/// reads as "unset" and a real zero row survives.
const FIELD_BITS: u32 = 9;
const FIELD_MASK: u32 = (1 << FIELD_BITS) - 1;

impl ZeroWidth {
  /// The combining mark, if this cell holds one.
  pub fn combining(&self) -> Option<char> {
    if self.0 == 0 || self.0 & PLACEHOLDER_TAG != 0 {
      return None;
    }
    char::from_u32(self.0)
  }

  /// The decoded placeholder coordinates, if this is a placeholder cell.
  pub fn placeholder(&self) -> Option<Placeholder> {
    if self.0 & PLACEHOLDER_TAG == 0 {
      return None;
    }
    let field = |shift: u32| match (self.0 >> shift) & FIELD_MASK {
      0 => None,
      n => Some(n - 1),
    };
    Some(Placeholder {
      row: field(0).map(|n| n as u16),
      col: field(FIELD_BITS).map(|n| n as u16),
      id_high: field(FIELD_BITS * 2).map(|n| n as u8),
    })
  }

  /// The raw packed value, for the scrollback codec.
  pub fn bits(&self) -> u32 {
    self.0
  }

  /// Rebuild from a packed value. A codepoint that is not a valid `char`
  /// decodes as empty rather than as a bogus mark.
  pub fn from_bits(bits: u32) -> ZeroWidth {
    if bits & PLACEHOLDER_TAG != 0 || bits == 0 || char::from_u32(bits).is_some() {
      ZeroWidth(bits)
    } else {
      ZeroWidth(0)
    }
  }

  fn pack(p: Placeholder) -> ZeroWidth {
    let field = |v: Option<u32>| v.map_or(0, |n| (n + 1) & FIELD_MASK);
    ZeroWidth(
      PLACEHOLDER_TAG
        | field(p.row.map(u32::from))
        | field(p.col.map(u32::from)) << FIELD_BITS
        | field(p.id_high.map(u32::from)) << (FIELD_BITS * 2),
    )
  }
}

impl Default for Cell {
  fn default() -> Self {
    Cell {
      ch: ' ',
      fg: Color::Default,
      bg: Color::Default,
      underline_color: Color::Default,
      flags: CellFlags::empty(),
      hyperlink: None,
      zw: ZeroWidth::default(),
    }
  }
}

impl Cell {
  /// A blank cell as produced by erase operations: keeps the pen's
  /// background (BCE semantics), drops everything else.
  pub fn erased(pen: Cell) -> Cell {
    Cell {
      bg: pen.bg,
      ..Cell::default()
    }
  }

  pub fn is_wide(&self) -> bool {
    self.flags.contains(CellFlags::WIDE)
  }

  pub fn is_wide_spacer(&self) -> bool {
    self.flags.contains(CellFlags::WIDE_SPACER)
  }

  /// Attach a zero-width mark to this cell. On an ordinary cell the first
  /// mark wins; on a kitty placeholder the marks are coordinates, so each is
  /// decoded and folded in.
  pub fn push_combining(&mut self, c: char) {
    if c == '\0' {
      return;
    }
    if self.ch == PLACEHOLDER {
      let mut p = self.zw.placeholder().unwrap_or_default();
      p.push(c);
      self.zw = ZeroWidth::pack(p);
      return;
    }
    if self.zw == ZeroWidth::default() {
      self.zw = ZeroWidth(c as u32);
    }
  }

  /// The combining mark attached to this cell, if any. A placeholder's marks
  /// were consumed as coordinates and are not text.
  pub fn combining(&self) -> impl Iterator<Item = char> + '_ {
    self.zw.combining().into_iter()
  }

  /// The kitty image coordinates this cell stands for, if it is a placeholder.
  pub fn placeholder(&self) -> Option<Placeholder> {
    (self.ch == PLACEHOLDER).then(|| self.zw.placeholder().unwrap_or_default())
  }

  /// Push `ch` followed by any combining marks onto `out` - the cell's full
  /// grapheme as text.
  pub fn write_grapheme(&self, out: &mut String) {
    out.push(self.ch);
    for c in self.combining() {
      out.push(c);
    }
  }
}

#[cfg(test)]
#[path = "../tests/cell.rs"]
mod tests;
