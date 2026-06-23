//! Custom geometry for box-drawing and block-element glyphs, so lines join
//! seamlessly and blocks tile exactly regardless of the font. Covers the
//! light line set (U+2500–254B subset), block elements and shades
//! (U+2580–2593), and eighth blocks. Characters not handled here fall back
//! to font glyphs.
//!
//! `rects` returns axis-aligned fills in pixels relative to the cell's
//! top-left, plus an alpha (used by the shade characters).

/// Filled rectangles for one cell glyph, in cell-relative pixels.
#[derive(Debug, Clone, PartialEq)]
pub struct BoxGlyph {
    pub rects: Vec<(f32, f32, f32, f32)>,
    /// Fill alpha; 1.0 except for the shade characters.
    pub alpha: f32,
}

/// Whether `ch` is drawn by this module rather than the font.
pub fn covers(ch: char) -> bool {
    rects(ch, 1.0, 1.0).is_some()
}

/// Geometry for a box-drawing/block char in a `w` x `h` cell, or `None` if
/// the char should be rendered with the font instead.
pub fn rects(ch: char, w: f32, h: f32) -> Option<BoxGlyph> {
    if let Some(rects) = lines(ch, w, h) {
        return Some(BoxGlyph { rects, alpha: 1.0 });
    }
    blocks(ch, w, h)
}

/// Line thickness: ~1/8 cell height, at least one pixel, snapped.
fn thickness(h: f32) -> f32 {
    (h / 8.0).round().max(1.0)
}

/// Light box-drawing lines and junctions. Each glyph is the union of arms
/// from the centre crossbars to the relevant edges.
fn lines(ch: char, w: f32, h: f32) -> Option<Vec<(f32, f32, f32, f32)>> {
    let t = thickness(h);
    // Centre bar offsets (snapped so the 1px-odd case stays crisp).
    let vx = ((w - t) / 2.0).round();
    let vy = ((h - t) / 2.0).round();
    let h_full = (0.0, vy, w, t);
    let v_full = (vx, 0.0, t, h);
    let arm_left = (0.0, vy, vx + t, t);
    let arm_right = (vx, vy, w - vx, t);
    let arm_top = (vx, 0.0, t, vy + t);
    let arm_bottom = (vx, vy, t, h - vy);
    let set: &[(f32, f32, f32, f32)] = match ch {
        '\u{2500}' => &[h_full],                // ─
        '\u{2502}' => &[v_full],                // │
        '\u{250C}' => &[arm_right, arm_bottom], // ┌
        '\u{2510}' => &[arm_left, arm_bottom],  // ┐
        '\u{2514}' => &[arm_right, arm_top],    // └
        '\u{2518}' => &[arm_left, arm_top],     // ┘
        '\u{251C}' => &[v_full, arm_right],     // ├
        '\u{2524}' => &[v_full, arm_left],      // ┤
        '\u{252C}' => &[h_full, arm_bottom],    // ┬
        '\u{2534}' => &[h_full, arm_top],       // ┴
        '\u{253C}' => &[h_full, v_full],        // ┼
        _ => return None,
    };
    Some(set.to_vec())
}

/// Block elements, shades and eighth blocks.
fn blocks(ch: char, w: f32, h: f32) -> Option<BoxGlyph> {
    let solid = |rects: Vec<(f32, f32, f32, f32)>| BoxGlyph { rects, alpha: 1.0 };
    let full = (0.0, 0.0, w, h);
    Some(match ch {
        '\u{2588}' => solid(vec![full]),                       // █
        '\u{2580}' => solid(vec![(0.0, 0.0, w, h / 2.0)]),     // ▀ upper half
        '\u{2584}' => solid(vec![(0.0, h / 2.0, w, h / 2.0)]), // ▄ lower half
        '\u{258C}' => solid(vec![(0.0, 0.0, w / 2.0, h)]),     // ▌ left half
        '\u{2590}' => solid(vec![(w / 2.0, 0.0, w / 2.0, h)]), // ▐ right half
        // Shades: full fill at reduced alpha.
        '\u{2591}' => BoxGlyph {
            rects: vec![full],
            alpha: 0.25,
        }, // ░
        '\u{2592}' => BoxGlyph {
            rects: vec![full],
            alpha: 0.5,
        }, // ▒
        '\u{2593}' => BoxGlyph {
            rects: vec![full],
            alpha: 0.75,
        }, // ▓
        // Lower eighths ▁..▇ (1/8..7/8 from the bottom).
        '\u{2581}'..='\u{2587}' => {
            let n = (ch as u32 - 0x2580) as f32; // 1..7
            let bh = h * n / 8.0;
            solid(vec![(0.0, h - bh, w, bh)])
        }
        // Left eighths ▉..▏ (▉=7/8 down to ▏=1/8).
        '\u{2589}'..='\u{258F}' => {
            let n = (0x2590 - ch as u32) as f32; // ▉->7 .. ▏->1
            let bw = w * n / 8.0;
            solid(vec![(0.0, 0.0, bw, h)])
        }
        _ => return None,
    })
}

#[cfg(test)]
#[path = "../tests/boxdraw.rs"]
mod tests;
