//! Kitty graphics protocol: parsing the `APC _G <control> ; <payload> ST`
//! control block and decoding its image payload into RGBA.
//!
//! Pure logic, no I/O — the byte-level APC capture and the placement/response
//! side-effects live in `term::apc`. This module covers the common-case
//! transmission the image viewers emit: direct base64 payloads (already decoded
//! by the caller) in RGB (`f=24`), RGBA (`f=32`), or PNG (`f=100`), optionally
//! zlib-compressed (`o=z`). File / shared-memory media, animation, unicode
//! placeholders, and compositing are out of scope (see `docs/parity.md`).

use crate::sixel::Image;

/// Largest image dimension (px) we will decode, and the pixel-count cap that
/// bounds a raw-format allocation. Mirrors `sixel::MAX_DIM` in spirit.
const MAX_DIM: u32 = 10_000;
const MAX_PIXELS: usize = 25_000_000; // 25 MP

/// Ceiling on a decompressed (`o=z`) payload. A valid image never needs more
/// than `MAX_PIXELS` RGBA bytes (a PNG file is smaller still), so anything past
/// this is a decompression bomb and is refused before it is fully allocated.
const MAX_DECODED_BYTES: u64 = (MAX_PIXELS * 4) as u64;

/// The parsed `a=` action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Action {
    /// `a=t`: transmit (store) the image only.
    Transmit,
    /// `a=T`: transmit and immediately display at the cursor.
    TransmitAndDisplay,
    /// `a=p`: display a previously transmitted image (`i=`).
    Display,
    /// `a=d`: delete placements.
    Delete,
    /// `a=q`: query — validate without storing or displaying.
    Query,
}

/// The control keys of one graphics command (recognized subset; unknown keys
/// are ignored). Defaults match the kitty spec.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Control {
    pub action: Action,
    /// Pixel format: 24 (RGB), 32 (RGBA), 100 (PNG).
    pub format: u32,
    /// Transmission medium (`t=`); only `d` (direct) is handled.
    pub medium: u8,
    /// `o=z`: payload is zlib-compressed.
    pub compressed: bool,
    /// `m=1`: another chunk follows.
    pub more: bool,
    /// `s=` / `v=`: pixel width / height (raw formats only).
    pub width: u32,
    pub height: u32,
    /// `i=`: image id (0 = unset).
    pub image_id: u32,
    /// `p=`: placement id (0 = unset).
    pub placement_id: u32,
    /// `q=`: quiet level (0 = respond, 1 = suppress OK, 2 = suppress all).
    pub quiet: u8,
    /// Whether displaying moves the cursor (`C=1` suppresses it).
    pub move_cursor: bool,
    /// `d=`: delete specifier (`a` all, `i` by image id, …); upper/lower.
    pub delete: u8,
}

impl Default for Control {
    fn default() -> Self {
        Self {
            action: Action::Transmit,
            format: 32,
            medium: b'd',
            compressed: false,
            more: false,
            width: 0,
            height: 0,
            image_id: 0,
            placement_id: 0,
            quiet: 0,
            move_cursor: true,
            delete: b'a',
        }
    }
}

/// A decode failure, carrying the kitty error code the response should report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GfxError(pub &'static str);

/// Parse the comma-separated `key=value` control block (the part before `;`).
pub(crate) fn parse_control(bytes: &[u8]) -> Control {
    let mut c = Control::default();
    for pair in bytes.split(|&b| b == b',') {
        let mut kv = pair.splitn(2, |&b| b == b'=');
        let key = kv.next().unwrap_or(&[]);
        let val = kv.next().unwrap_or(&[]);
        match key {
            b"a" => {
                c.action = match val.first() {
                    Some(b'T') => Action::TransmitAndDisplay,
                    Some(b'p') => Action::Display,
                    Some(b'd') => Action::Delete,
                    Some(b'q') => Action::Query,
                    _ => Action::Transmit,
                }
            }
            b"f" => c.format = num(val),
            b"t" => c.medium = val.first().copied().unwrap_or(b'd'),
            b"o" => c.compressed = val.first() == Some(&b'z'),
            b"m" => c.more = num(val) == 1,
            b"s" => c.width = num(val),
            b"v" => c.height = num(val),
            b"i" => c.image_id = num(val),
            b"p" => c.placement_id = num(val),
            b"q" => c.quiet = num(val) as u8,
            b"C" => c.move_cursor = num(val) == 0,
            b"d" => c.delete = val.first().copied().unwrap_or(b'a'),
            _ => {}
        }
    }
    c
}

/// Parse an unsigned decimal field, defaulting to 0 on anything unexpected.
fn num(bytes: &[u8]) -> u32 {
    std::str::from_utf8(bytes)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

/// Decode a (already base64-decoded, possibly chunk-joined) payload into RGBA
/// using `control`'s format/compression. `t=d` (direct) only.
pub(crate) fn decode(control: &Control, raw: &[u8]) -> Result<Image, GfxError> {
    if control.medium != b'd' {
        // File / temp-file / shared-memory transmission not supported.
        return Err(GfxError("EMEDIUM"));
    }
    let data = if control.compressed {
        inflate(raw)?
    } else {
        raw.to_vec()
    };
    match control.format {
        24 => raw_pixels(control, &data, 3),
        32 => raw_pixels(control, &data, 4),
        100 => decode_png(&data),
        _ => Err(GfxError("EFORMAT")),
    }
}

/// zlib-inflate the payload, refusing decompression bombs. `Read::take` bounds
/// how many bytes `read_to_end` will pull, so the output allocation can never
/// exceed the cap even for a hostile stream that expands ~1000x.
fn inflate(raw: &[u8]) -> Result<Vec<u8>, GfxError> {
    use std::io::Read;
    let mut out = Vec::new();
    flate2::read::ZlibDecoder::new(raw)
        .take(MAX_DECODED_BYTES + 1)
        .read_to_end(&mut out)
        .map_err(|_| GfxError("EINFLATE"))?;
    if out.len() as u64 > MAX_DECODED_BYTES {
        return Err(GfxError("EINFLATE"));
    }
    Ok(out)
}

/// Build an RGBA image from raw pixels with `channels` (3 = RGB, 4 = RGBA),
/// using the declared `s`×`v` dimensions.
fn raw_pixels(control: &Control, data: &[u8], channels: usize) -> Result<Image, GfxError> {
    let (w, h) = (control.width, control.height);
    if w == 0 || h == 0 || w > MAX_DIM || h > MAX_DIM {
        return Err(GfxError("EDIM"));
    }
    let pixels = (w as usize)
        .checked_mul(h as usize)
        .filter(|&p| p <= MAX_PIXELS)
        .ok_or(GfxError("EDIM"))?;
    if data.len() < pixels * channels {
        return Err(GfxError("ESIZE"));
    }
    let mut rgba = Vec::with_capacity(pixels * 4);
    for px in data[..pixels * channels].chunks_exact(channels) {
        rgba.extend_from_slice(&[px[0], px[1], px[2], if channels == 4 { px[3] } else { 255 }]);
    }
    Ok(Image {
        width: w as usize,
        height: h as usize,
        rgba,
    })
}

/// Decode a PNG payload to RGBA8. Palette/grayscale/16-bit inputs are
/// normalized to 8-bit RGB(A) by the decoder's transformations, then folded to
/// RGBA here.
fn decode_png(data: &[u8]) -> Result<Image, GfxError> {
    let mut decoder = png::Decoder::new(data);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|_| GfxError("EPNG"))?;
    let info = reader.info();
    let (w, h) = (info.width, info.height);
    if w == 0 || h == 0 || w > MAX_DIM || h > MAX_DIM {
        return Err(GfxError("EDIM"));
    }
    let pixels = (w as usize)
        .checked_mul(h as usize)
        .filter(|&p| p <= MAX_PIXELS)
        .ok_or(GfxError("EDIM"))?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let frame = reader.next_frame(&mut buf).map_err(|_| GfxError("EPNG"))?;
    let src = &buf[..frame.buffer_size()];
    let mut rgba = Vec::with_capacity(pixels * 4);
    match frame.color_type {
        png::ColorType::Rgba => rgba.extend_from_slice(src),
        png::ColorType::Rgb => {
            for p in src.chunks_exact(3) {
                rgba.extend_from_slice(&[p[0], p[1], p[2], 255]);
            }
        }
        png::ColorType::GrayscaleAlpha => {
            for p in src.chunks_exact(2) {
                rgba.extend_from_slice(&[p[0], p[0], p[0], p[1]]);
            }
        }
        png::ColorType::Grayscale => {
            for &g in src {
                rgba.extend_from_slice(&[g, g, g, 255]);
            }
        }
        png::ColorType::Indexed => return Err(GfxError("EPNG")),
    }
    if rgba.len() != pixels * 4 {
        return Err(GfxError("EPNG"));
    }
    Ok(Image {
        width: w as usize,
        height: h as usize,
        rgba,
    })
}

#[cfg(test)]
#[path = "../tests/graphics.rs"]
mod tests;
