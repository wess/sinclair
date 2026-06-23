//! Cell metrics: grid <-> pixel conversions for the terminal surface.

/// Pixel size of one terminal cell.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CellSize {
    pub width: f32,
    pub height: f32,
}

/// Inner window padding in pixels, applied on every side.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Padding {
    pub x: f32,
    pub y: f32,
}

/// Line height as a multiple of the font size. Terminal rows are denser
/// than editor lines; 1.3 tracks zed's "standard" terminal line height.
pub const LINE_HEIGHT: f32 = 1.3;

/// Fallback advance-to-font-size ratio when the font cannot be measured.
pub const FALLBACK_ADVANCE: f32 = 0.6;

/// How many whole cells fit in a surface of `width` x `height` pixels.
/// Never returns fewer than 2 columns or 1 row (a 1-column grid breaks
/// wide-character rendering).
pub fn grid_size(width: f32, height: f32, pad: Padding, cell: CellSize) -> (usize, usize) {
    let usable_w = (width - 2.0 * pad.x).max(0.0);
    let usable_h = (height - 2.0 * pad.y).max(0.0);
    let cols = (usable_w / cell.width).floor() as usize;
    let rows = (usable_h / cell.height).floor() as usize;
    (cols.max(2), rows.max(1))
}

/// Pixel size of a window whose content area holds exactly `cols` x `rows`
/// cells plus padding.
pub fn pixel_size(cols: usize, rows: usize, pad: Padding, cell: CellSize) -> (f32, f32) {
    (
        cols as f32 * cell.width + 2.0 * pad.x,
        rows as f32 * cell.height + 2.0 * pad.y,
    )
}

/// Map a window-space position onto a grid cell. `origin` is the element
/// bounds origin; padding is applied here. Positions outside the grid
/// clamp to the nearest cell, so drags past any edge stay valid.
pub fn cell_at(
    pos: (f32, f32),
    origin: (f32, f32),
    pad: Padding,
    cell: CellSize,
    cols: usize,
    rows: usize,
) -> (usize, usize) {
    let col = ((pos.0 - origin.0 - pad.x) / cell.width).floor();
    let row = ((pos.1 - origin.1 - pad.y) / cell.height).floor();
    let col = (col.max(0.0) as usize).min(cols.saturating_sub(1));
    let row = (row.max(0.0) as usize).min(rows.saturating_sub(1));
    (row, col)
}

/// Map a viewport cell to the vt selection coordinate scheme: content line
/// 0 is the top live-grid row, so a viewport rendered at `display_offset`
/// shows line `row - display_offset` (negative lines are scrollback).
pub fn selection_point(row: usize, col: usize, display_offset: usize) -> vt::Point {
    vt::Point::new(row as isize - display_offset as isize, col)
}

/// Measure the cell box for a font: advance width of `M` and the terminal
/// line height. Falls back to a fixed ratio when the glyph is missing.
pub fn measure(
    text_system: &gpui::TextSystem,
    font: &gpui::Font,
    font_size: gpui::Pixels,
) -> CellSize {
    let font_id = text_system.resolve_font(font);
    let width = text_system
        .advance(font_id, font_size, 'M')
        .map(|advance| f32::from(advance.width))
        .unwrap_or_else(|_| f32::from(font_size) * FALLBACK_ADVANCE);
    CellSize {
        width,
        height: f32::from(font_size) * LINE_HEIGHT,
    }
}

#[cfg(test)]
#[path = "../tests/metrics.rs"]
mod tests;
