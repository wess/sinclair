//! Sixel image decoding: a DCS `q` payload into an RGBA pixel buffer.
//!
//! Supports the commands the common encoders (`img2sixel`, `lsix`) emit:
//! raster attributes (`"`), color definition/selection (`#`, RGB and HLS),
//! run-length repeats (`!`), carriage return (`$`), and band newline (`-`).
//! Each data byte `?`..=`~` paints six vertical pixels in the current color.

/// A decoded image: tightly-packed RGBA8, `width * height * 4` bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}

impl Image {
    /// How many text rows the image spans for cells `cell_h` pixels tall.
    pub fn image_rows(&self, cell_h: usize) -> usize {
        self.height.div_ceil(cell_h.max(1))
    }
}

/// A decoded image anchored to the grid. `line` is an absolute content line
/// (0 = top of the live grid, negative = scrollback) that shifts as the buffer
/// scrolls, so the image rides along with the text beneath it. `id` is unique
/// per placement so the renderer can cache the GPU texture.
#[derive(Debug, Clone)]
pub struct Placement {
    pub id: u64,
    pub line: isize,
    pub col: usize,
    pub image: Image,
}

#[derive(Clone, Copy)]
struct Rgb(u8, u8, u8);

/// The VT340 default 16-color palette, used for color indices a payload
/// selects without defining.
const DEFAULT_PALETTE: [Rgb; 16] = [
    Rgb(0, 0, 0),
    Rgb(51, 51, 204),
    Rgb(204, 51, 51),
    Rgb(51, 204, 51),
    Rgb(204, 51, 204),
    Rgb(51, 204, 204),
    Rgb(204, 204, 51),
    Rgb(135, 135, 135),
    Rgb(66, 66, 66),
    Rgb(84, 84, 148),
    Rgb(148, 84, 84),
    Rgb(84, 148, 84),
    Rgb(148, 84, 148),
    Rgb(84, 148, 148),
    Rgb(148, 148, 84),
    Rgb(204, 204, 204),
];

const MAX_DIM: usize = 10_000;

/// Cap on total decoded pixels (~32 MB of `Option<Rgb>`), so a malicious sixel
/// can't exhaust memory regardless of its declared dimensions.
const MAX_AREA: usize = 8_000_000;

/// Decode a sixel payload (the bytes between `DCS ... q` and `ST`). Returns
/// `None` when nothing was drawn or the geometry is unreasonable.
pub fn decode(data: &[u8]) -> Option<Image> {
    let mut palette: Vec<Rgb> = (0..256)
        .map(|i| DEFAULT_PALETTE[i % 16])
        .collect();
    let mut pixels: Vec<Option<Rgb>> = Vec::new();
    let mut width = 0usize;
    let mut height = 0usize;
    // Cursor: x in pixels, `band` is the current six-pixel row (y = band*6).
    let mut x = 0usize;
    let mut band = 0usize;
    let mut color = 0usize;

    let ensure = |pixels: &mut Vec<Option<Rgb>>,
                      width: &mut usize,
                      height: &mut usize,
                      max_x: usize,
                      max_y: usize|
     -> bool {
        let w = (*width).max(max_x);
        let h = (*height).max(max_y);
        // Bound both dimensions and the total area so one crafted DCS payload
        // can't force a huge (hundreds of MB) allocation.
        if w > MAX_DIM || h > MAX_DIM || w.saturating_mul(h) > MAX_AREA {
            return false;
        }
        if w != *width || h != *height {
            let mut grown = vec![None; w * h];
            for row in 0..*height {
                let (src, dst) = (row * *width, row * w);
                grown[dst..dst + *width].copy_from_slice(&pixels[src..src + *width]);
            }
            *pixels = grown;
            *width = w;
            *height = h;
        }
        true
    };

    let mut i = 0;
    while i < data.len() {
        let b = data[i];
        match b {
            b'"' => {
                // Raster attributes: Pan;Pad;Ph;Pv — reserve Ph x Pv.
                let (params, next) = read_params(data, i + 1);
                i = next;
                if let (Some(&ph), Some(&pv)) = (params.get(2), params.get(3)) {
                    if !ensure(&mut pixels, &mut width, &mut height, ph as usize, pv as usize) {
                        return None;
                    }
                }
                continue;
            }
            b'#' => {
                // Color: #Pc selects; #Pc;Pu;Px;Py;Pz defines.
                let (params, next) = read_params(data, i + 1);
                i = next;
                if let Some(&pc) = params.first() {
                    color = pc as usize % 256;
                    if params.len() >= 5 {
                        let (mode, a, b2, c) = (params[1], params[2], params[3], params[4]);
                        palette[color] = define_color(mode, a, b2, c);
                    }
                }
                continue;
            }
            b'!' => {
                // Run length: !Pn <sixel>
                let (params, next) = read_params(data, i + 1);
                let n = params.first().copied().unwrap_or(0).max(1) as usize;
                i = next;
                if let Some(&s) = data.get(i) {
                    if (0x3f..=0x7e).contains(&s) {
                        let max_x = x + n;
                        if !ensure(&mut pixels, &mut width, &mut height, max_x, (band + 1) * 6) {
                            return None;
                        }
                        for _ in 0..n {
                            paint_sixel(&mut pixels, width, x, band, s, palette[color]);
                            x += 1;
                        }
                        i += 1;
                    }
                }
                continue;
            }
            b'$' => {
                x = 0;
                i += 1;
            }
            b'-' => {
                x = 0;
                band += 1;
                i += 1;
            }
            0x3f..=0x7e => {
                if !ensure(&mut pixels, &mut width, &mut height, x + 1, (band + 1) * 6) {
                    return None;
                }
                paint_sixel(&mut pixels, width, x, band, b, palette[color]);
                x += 1;
                i += 1;
            }
            // Whitespace and anything else: skip.
            _ => i += 1,
        }
    }

    if width == 0 || height == 0 {
        return None;
    }
    let mut rgba = Vec::with_capacity(width * height * 4);
    for p in &pixels {
        match p {
            Some(Rgb(r, g, b)) => rgba.extend_from_slice(&[*r, *g, *b, 255]),
            None => rgba.extend_from_slice(&[0, 0, 0, 0]),
        }
    }
    Some(Image {
        width,
        height,
        rgba,
    })
}

/// Paint the six vertical pixels a sixel byte encodes at column `x`, starting
/// at `band * 6`. Bit 0 is the topmost pixel.
fn paint_sixel(pixels: &mut [Option<Rgb>], width: usize, x: usize, band: usize, byte: u8, color: Rgb) {
    let bits = byte - 0x3f;
    for row in 0..6 {
        if bits & (1 << row) != 0 {
            let y = band * 6 + row;
            let idx = y * width + x;
            if idx < pixels.len() {
                pixels[idx] = Some(color);
            }
        }
    }
}

/// Define a palette color: mode 2 is RGB (0-100%), mode 1 is HLS.
fn define_color(mode: u16, a: u16, b: u16, c: u16) -> Rgb {
    match mode {
        2 => Rgb(pct(a), pct(b), pct(c)),
        1 => hls_to_rgb(a, b, c),
        _ => Rgb(0, 0, 0),
    }
}

/// Scale a 0-100 percentage to 0-255.
fn pct(v: u16) -> u8 {
    ((v.min(100) as u32 * 255 + 50) / 100) as u8
}

/// Sixel HLS (hue 0-360, lightness/saturation 0-100) to RGB.
fn hls_to_rgb(h: u16, l: u16, s: u16) -> Rgb {
    let h = (h % 360) as f32;
    let l = (l.min(100) as f32) / 100.0;
    let s = (s.min(100) as f32) / 100.0;
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    // Sixel hue 0 is blue; offset by 240 degrees to map onto standard HSL.
    let hp = ((h + 240.0) % 360.0) / 60.0;
    let xx = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r1, g1, b1) = match hp as u32 {
        0 => (c, xx, 0.0),
        1 => (xx, c, 0.0),
        2 => (0.0, c, xx),
        3 => (0.0, xx, c),
        4 => (xx, 0.0, c),
        _ => (c, 0.0, xx),
    };
    let m = l - c / 2.0;
    let to = |v: f32| (((v + m) * 255.0).round().clamp(0.0, 255.0)) as u8;
    Rgb(to(r1), to(g1), to(b1))
}

/// Read a `;`-separated run of decimal parameters starting at `start`,
/// returning the values and the index just past them.
fn read_params(data: &[u8], start: usize) -> (Vec<u16>, usize) {
    let mut params = Vec::new();
    let mut cur: u32 = 0;
    let mut any = false;
    let mut i = start;
    while i < data.len() {
        match data[i] {
            b'0'..=b'9' => {
                cur = (cur * 10 + (data[i] - b'0') as u32).min(65535);
                any = true;
            }
            b';' => {
                params.push(cur as u16);
                cur = 0;
                any = true;
            }
            _ => break,
        }
        i += 1;
    }
    if any {
        params.push(cur as u16);
    }
    (params, i)
}

#[cfg(test)]
#[path = "../tests/sixel.rs"]
mod tests;
