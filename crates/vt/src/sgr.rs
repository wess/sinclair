//! SGR (Select Graphic Rendition) application onto a pen cell.
//!
//! Accepts the parameter shape produced by `vte::Params`: an iterator of
//! slices, where colon subparameters arrive grouped in one slice
//! (`38:2:r:g:b` -> `[38, 2, r, g, b]`) and semicolon parameters arrive as
//! separate single-element slices (`38;2;r;g;b` -> `[38] [2] [r] [g] [b]`).

use crate::cell::{Cell, CellFlags};
use crate::color::Color;

/// Apply a full SGR parameter list to `pen`. An empty list resets.
pub fn apply<'a, I>(pen: &mut Cell, params: I)
where
    I: IntoIterator<Item = &'a [u16]>,
{
    let mut iter = params.into_iter();
    let mut any = false;
    while let Some(param) = iter.next() {
        any = true;
        let code = param.first().copied().unwrap_or(0);
        match code {
            0 => reset(pen),
            1 => pen.flags.insert(CellFlags::BOLD),
            2 => pen.flags.insert(CellFlags::DIM),
            3 => pen.flags.insert(CellFlags::ITALIC),
            4 => apply_underline(pen, param.get(1).copied()),
            5 | 6 => pen.flags.insert(CellFlags::BLINK),
            7 => pen.flags.insert(CellFlags::INVERSE),
            8 => pen.flags.insert(CellFlags::INVISIBLE),
            9 => pen.flags.insert(CellFlags::STRIKETHROUGH),
            21 => {
                pen.flags.remove(CellFlags::ANY_UNDERLINE);
                pen.flags.insert(CellFlags::DOUBLE_UNDERLINE);
            }
            22 => pen.flags.remove(CellFlags::BOLD | CellFlags::DIM),
            23 => pen.flags.remove(CellFlags::ITALIC),
            24 => pen.flags.remove(CellFlags::ANY_UNDERLINE),
            25 => pen.flags.remove(CellFlags::BLINK),
            27 => pen.flags.remove(CellFlags::INVERSE),
            28 => pen.flags.remove(CellFlags::INVISIBLE),
            29 => pen.flags.remove(CellFlags::STRIKETHROUGH),
            30..=37 => pen.fg = Color::Indexed((code - 30) as u8),
            38 => {
                if let Some(color) = extended_color(param, &mut iter) {
                    pen.fg = color;
                }
            }
            39 => pen.fg = Color::Default,
            40..=47 => pen.bg = Color::Indexed((code - 40) as u8),
            48 => {
                if let Some(color) = extended_color(param, &mut iter) {
                    pen.bg = color;
                }
            }
            49 => pen.bg = Color::Default,
            58 => {
                if let Some(color) = extended_color(param, &mut iter) {
                    pen.underline_color = color;
                }
            }
            59 => pen.underline_color = Color::Default,
            90..=97 => pen.fg = Color::Indexed((code - 90 + 8) as u8),
            100..=107 => pen.bg = Color::Indexed((code - 100 + 8) as u8),
            _ => {}
        }
    }
    if !any {
        reset(pen);
    }
}

/// SGR 0: back to the default pen.
pub fn reset(pen: &mut Cell) {
    let ch = pen.ch;
    *pen = Cell::default();
    pen.ch = ch;
}

fn apply_underline(pen: &mut Cell, sub: Option<u16>) {
    pen.flags.remove(CellFlags::ANY_UNDERLINE);
    let style = match sub {
        None | Some(1) => Some(CellFlags::UNDERLINE),
        Some(0) => None,
        Some(2) => Some(CellFlags::DOUBLE_UNDERLINE),
        Some(3) => Some(CellFlags::CURLY_UNDERLINE),
        Some(4) => Some(CellFlags::DOTTED_UNDERLINE),
        Some(5) => Some(CellFlags::DASHED_UNDERLINE),
        Some(_) => Some(CellFlags::UNDERLINE),
    };
    if let Some(style) = style {
        pen.flags.insert(style);
    }
}

/// Parse the color payload of SGR 38/48/58 in either form.
fn extended_color<'a, I>(param: &[u16], iter: &mut I) -> Option<Color>
where
    I: Iterator<Item = &'a [u16]>,
{
    if param.len() > 1 {
        colon_color(&param[1..])
    } else {
        semicolon_color(iter)
    }
}

/// Colon form: subparams after the 38/48/58, e.g. `[2, r, g, b]`,
/// `[2, colorspace, r, g, b]`, or `[5, idx]`.
fn colon_color(sub: &[u16]) -> Option<Color> {
    match sub.first()? {
        2 => {
            let rest = &sub[1..];
            // A 4+ value payload carries a leading colorspace id; skip it.
            let rgb = if rest.len() >= 4 {
                &rest[1..4]
            } else {
                rest.get(0..3)?
            };
            Some(Color::Rgb(clamp8(rgb[0]), clamp8(rgb[1]), clamp8(rgb[2])))
        }
        5 => Some(Color::Indexed(clamp8(*sub.get(1)?))),
        _ => None,
    }
}

/// Semicolon form: pull `2;r;g;b` or `5;idx` from the following params.
fn semicolon_color<'a, I>(iter: &mut I) -> Option<Color>
where
    I: Iterator<Item = &'a [u16]>,
{
    let kind = iter.next()?.first().copied()?;
    match kind {
        2 => {
            let r = clamp8(iter.next()?.first().copied()?);
            let g = clamp8(iter.next()?.first().copied()?);
            let b = clamp8(iter.next()?.first().copied()?);
            Some(Color::Rgb(r, g, b))
        }
        5 => Some(Color::Indexed(clamp8(iter.next()?.first().copied()?))),
        _ => None,
    }
}

fn clamp8(v: u16) -> u8 {
    v.min(255) as u8
}

#[cfg(test)]
#[path = "../tests/sgr.rs"]
mod tests;
