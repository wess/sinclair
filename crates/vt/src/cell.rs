//! A single grid cell: character, colors, and attribute flags.

use crate::color::Color;
use crate::hyperlink::HyperlinkId;

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

/// One terminal cell. Copy, 16 bytes-ish; the grid stores these densely.
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
}

#[cfg(test)]
#[path = "../tests/cell.rs"]
mod tests;
