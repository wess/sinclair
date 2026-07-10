//! Row codec for compressed scrollback blocks.
//!
//! In-process only; nothing here is ever persisted, so the format can change
//! freely. Each row is trimmed of trailing default cells before encoding -
//! that alone removes most of a typical shell line - and the concatenated
//! rows are lz4-compressed as one block.

use crate::cell::{Cell, CellFlags};
use crate::color::Color;
use crate::grid::row::Row;
use crate::hyperlink::HyperlinkId;

/// A run of scrollback rows, encoded and lz4-compressed.
#[derive(Debug, Clone)]
pub(crate) struct Block {
    data: Vec<u8>,
    /// Uncompressed payload length, for decode sizing.
    raw_len: usize,
    /// Column count the rows had when compressed; decode restores it, then
    /// resizes lazily when the grid width has changed since.
    cols: usize,
}

impl Block {
    pub(crate) fn compressed_len(&self) -> usize {
        self.data.len()
    }
}

/// Encode and compress `rows` (all `cols` wide) into one block.
pub(crate) fn encode(rows: &[Row], cols: usize) -> Block {
    let mut raw = Vec::with_capacity(rows.len() * 64);
    for row in rows {
        encode_row(row, &mut raw);
    }
    Block {
        raw_len: raw.len(),
        data: lz4_flex::compress(&raw),
        cols,
    }
}

/// Decompress a block back into rows, resized to the current `cols`.
pub(crate) fn decode(block: &Block, cols: usize) -> Vec<Row> {
    let raw = lz4_flex::decompress(&block.data, block.raw_len)
        .expect("scrollback blocks are self-produced");
    let mut rows = Vec::new();
    let mut pos = 0;
    while pos < raw.len() {
        rows.push(decode_row(&raw, &mut pos, block.cols));
    }
    if cols != block.cols {
        for row in &mut rows {
            row.resize(cols, Cell::default());
        }
    }
    rows
}

fn encode_row(row: &Row, out: &mut Vec<u8>) {
    let default = Cell::default();
    let kept = row
        .cells
        .iter()
        .rposition(|c| *c != default)
        .map_or(0, |i| i + 1);
    out.extend_from_slice(&(kept as u16).to_le_bytes());
    out.push(u8::from(row.wrapped) | (u8::from(row.prompt) << 1));
    for cell in &row.cells[..kept] {
        encode_cell(cell, out);
    }
}

fn decode_row(raw: &[u8], pos: &mut usize, cols: usize) -> Row {
    let kept = read_u16(raw, pos) as usize;
    let flags = read_u8(raw, pos);
    let mut cells = Vec::with_capacity(cols.max(kept));
    for _ in 0..kept {
        cells.push(decode_cell(raw, pos));
    }
    cells.resize(cols.max(kept), Cell::default());
    Row {
        cells,
        wrapped: flags & 1 != 0,
        prompt: flags & 2 != 0,
    }
}

fn encode_cell(cell: &Cell, out: &mut Vec<u8>) {
    out.extend_from_slice(&(cell.ch as u32).to_le_bytes());
    encode_color(cell.fg, out);
    encode_color(cell.bg, out);
    encode_color(cell.underline_color, out);
    out.extend_from_slice(&cell.flags.bits().to_le_bytes());
    out.extend_from_slice(&cell.hyperlink.map_or(0, HyperlinkId::as_u16).to_le_bytes());
    out.extend_from_slice(&(cell.zw as u32).to_le_bytes());
}

fn decode_cell(raw: &[u8], pos: &mut usize) -> Cell {
    Cell {
        ch: char::from_u32(read_u32(raw, pos)).unwrap_or(' '),
        fg: decode_color(raw, pos),
        bg: decode_color(raw, pos),
        underline_color: decode_color(raw, pos),
        flags: CellFlags::from_bits_truncate(read_u16(raw, pos)),
        hyperlink: HyperlinkId::from_u16(read_u16(raw, pos)),
        zw: char::from_u32(read_u32(raw, pos)).unwrap_or('\0'),
    }
}

/// Tag byte plus a fixed 3-byte payload, so cells stay fixed-width.
fn encode_color(color: Color, out: &mut Vec<u8>) {
    match color {
        Color::Default => out.extend_from_slice(&[0, 0, 0, 0]),
        Color::Indexed(i) => out.extend_from_slice(&[1, i, 0, 0]),
        Color::Rgb(r, g, b) => out.extend_from_slice(&[2, r, g, b]),
    }
}

fn decode_color(raw: &[u8], pos: &mut usize) -> Color {
    let tag = read_u8(raw, pos);
    let a = read_u8(raw, pos);
    let b = read_u8(raw, pos);
    let c = read_u8(raw, pos);
    match tag {
        1 => Color::Indexed(a),
        2 => Color::Rgb(a, b, c),
        _ => Color::Default,
    }
}

fn read_u8(raw: &[u8], pos: &mut usize) -> u8 {
    let v = raw[*pos];
    *pos += 1;
    v
}

fn read_u16(raw: &[u8], pos: &mut usize) -> u16 {
    let v = u16::from_le_bytes(raw[*pos..*pos + 2].try_into().expect("2 bytes"));
    *pos += 2;
    v
}

fn read_u32(raw: &[u8], pos: &mut usize) -> u32 {
    let v = u32::from_le_bytes(raw[*pos..*pos + 4].try_into().expect("4 bytes"));
    *pos += 4;
    v
}

#[cfg(test)]
#[path = "../../tests/grid/codec.rs"]
mod tests;
