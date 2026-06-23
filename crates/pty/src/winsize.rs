//! Terminal window dimensions in cells and pixels.

/// Terminal window size: grid dimensions plus the pixel size of one cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Winsize {
    /// Number of columns in the grid.
    pub cols: u16,
    /// Number of rows in the grid.
    pub rows: u16,
    /// Pixel width of a single cell (0 if unknown).
    pub cell_width: u16,
    /// Pixel height of a single cell (0 if unknown).
    pub cell_height: u16,
}

impl Winsize {
    /// Size with the given grid and unknown cell pixel dimensions.
    pub fn new(cols: u16, rows: u16) -> Self {
        Self {
            cols,
            rows,
            cell_width: 0,
            cell_height: 0,
        }
    }

    /// Size with grid and per-cell pixel dimensions.
    pub fn with_cell_size(cols: u16, rows: u16, cell_width: u16, cell_height: u16) -> Self {
        Self {
            cols,
            rows,
            cell_width,
            cell_height,
        }
    }

    /// Convert to the kernel `struct winsize`. Pixel fields are the total
    /// window pixel size (grid * cell), saturating on overflow.
    pub fn to_termios(self) -> rustix::termios::Winsize {
        rustix::termios::Winsize {
            ws_row: self.rows,
            ws_col: self.cols,
            ws_xpixel: self.cols.saturating_mul(self.cell_width),
            ws_ypixel: self.rows.saturating_mul(self.cell_height),
        }
    }
}

impl Default for Winsize {
    fn default() -> Self {
        Self::new(80, 24)
    }
}

#[cfg(all(test, unix))]
#[path = "../tests/winsize.rs"]
mod tests;
