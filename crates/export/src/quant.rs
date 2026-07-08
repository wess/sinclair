//! A fixed 256-color palette derived from the theme, plus a fast RGB->index map.
//!
//! Per-frame NeuQuant is slow and gives every GIF frame its own local palette,
//! which bloats the file and defeats cross-frame compression. Terminal output
//! draws from a small, known set of colors (the theme background, foreground,
//! and ANSI slots) plus the anti-aliased blends between text and background, so
//! we build one global palette up front and map every frame against it. A
//! precomputed 15-bit-RGB lookup table makes the mapping a single array read per
//! pixel.

use theme::Rgb;

use crate::colors::Colors;

pub struct Quantizer {
    /// 256 RGB triples, flat; unused tail slots are black padding so the color
    /// table is always a full power-of-two size.
    palette: Vec<u8>,
    /// 15-bit `(r5, g5, b5)` -> nearest palette index.
    lut: Vec<u8>,
}

impl Quantizer {
    /// Build a palette covering the theme's colors and the anti-aliasing ramps
    /// between the background and each text color.
    pub fn from_colors(colors: &Colors) -> Quantizer {
        let mut entries: Vec<[u8; 3]> = Vec::with_capacity(256);
        let push = |c: Rgb, entries: &mut Vec<[u8; 3]>| {
            let e = [c.r, c.g, c.b];
            if entries.len() < 256 && !entries.contains(&e) {
                entries.push(e);
            }
        };

        for c in [colors.bg, colors.fg, colors.cursor, colors.cursor_text] {
            push(c, &mut entries);
        }
        for i in 0..16u8 {
            push(colors.palette.get(i), &mut entries);
        }

        // Anti-alias ramps: glyph edges blend a text color toward the
        // background, so include six steps between them.
        let mut text_colors = vec![colors.fg];
        for i in 0..16u8 {
            text_colors.push(colors.palette.get(i));
        }
        for fg in text_colors {
            for step in 1..7 {
                push(mix(colors.bg, fg, step as f32 / 7.0), &mut entries);
            }
        }

        // Fill any remaining slots from the 256-color cube so 256-color and
        // truecolor content still maps somewhere reasonable.
        let mut i = 16u16;
        while entries.len() < 256 && i < 256 {
            push(colors.palette.get(i as u8), &mut entries);
            i += 1;
        }

        let lut = build_lut(&entries);
        let mut palette = Vec::with_capacity(256 * 3);
        for e in &entries {
            palette.extend_from_slice(e);
        }
        palette.resize(256 * 3, 0);

        Quantizer { palette, lut }
    }

    /// The flat RGB global palette for `gif::Encoder::new`.
    pub fn palette(&self) -> &[u8] {
        &self.palette
    }

    /// Map an opaque RGBA image to palette indices, reusing `out`.
    pub fn map(&self, rgba: &[u8], out: &mut Vec<u8>) {
        out.clear();
        out.reserve(rgba.len() / 4);
        for px in rgba.chunks_exact(4) {
            let key = ((px[0] as usize >> 3) << 10)
                | ((px[1] as usize >> 3) << 5)
                | (px[2] as usize >> 3);
            out.push(self.lut[key]);
        }
    }
}

/// Linearly blend `a` toward `b` by `t` in `0.0..=1.0`.
fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let c = |x: u8, y: u8| {
        (x as f32 * (1.0 - t) + y as f32 * t)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    Rgb::new(c(a.r, b.r), c(a.g, b.g), c(a.b, b.b))
}

/// For every 15-bit RGB value, find the nearest palette entry once.
fn build_lut(entries: &[[u8; 3]]) -> Vec<u8> {
    let mut lut = vec![0u8; 32768];
    for r5 in 0..32u32 {
        // Expand 5-bit channels back to 8-bit, replicating high bits.
        let r = ((r5 << 3) | (r5 >> 2)) as i32;
        for g5 in 0..32u32 {
            let g = ((g5 << 3) | (g5 >> 2)) as i32;
            for b5 in 0..32u32 {
                let b = ((b5 << 3) | (b5 >> 2)) as i32;
                let mut best = 0usize;
                let mut best_dist = i32::MAX;
                for (i, e) in entries.iter().enumerate() {
                    let (dr, dg, db) = (r - e[0] as i32, g - e[1] as i32, b - e[2] as i32);
                    let dist = dr * dr + dg * dg + db * db;
                    if dist < best_dist {
                        best_dist = dist;
                        best = i;
                    }
                }
                lut[((r5 << 10) | (g5 << 5) | b5) as usize] = best as u8;
            }
        }
    }
    lut
}

#[cfg(test)]
#[path = "../tests/quant.rs"]
mod tests;
