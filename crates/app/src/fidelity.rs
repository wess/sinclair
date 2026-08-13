//! High-fidelity export rasterizer.
//!
//! Reuses gpui's CoreText text system to shape (ligatures, exact glyph choice)
//! and rasterize glyphs the same way the live app does, compositing the coverage
//! bitmaps into an RGBA buffer entirely offscreen - no window, no GPU surface.
//! It plugs into [`export`] via the [`export::Raster`] trait, so the timeline,
//! replay loop, and GIF/video encoders are shared with the software path.
//!
//! macOS only: it depends on `gpui_macos::MacTextSystem`. Other platforms use
//! the bundled software rasterizer.

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use gpui::{
    point, px, Font, FontId, FontStyle, FontWeight, GlyphId, Pixels, PlatformTextSystem,
    RenderGlyphParams, TextRun, TextSystem, WindowTextSystem,
};
use theme::Rgb;
use vt::{Cell, CellFlags, Terminal};

use export::{Colors, Raster, Rgba};

/// Bundled fonts, added to the text system so a ligature-capable family is
/// always available even if the configured one is not installed.
const REGULAR: &[u8] = include_bytes!("../../export/assets/mono.ttf");
const BOLD: &[u8] = include_bytes!("../../export/assets/mono.bold.ttf");

/// Terminal line height as a multiple of the font size (matches the app).
const LINE_HEIGHT: f32 = 1.3;

/// A rasterized glyph held for reuse across frames: its coverage (or BGRA
/// emoji) bytes and placement relative to the pen. `None` means the glyph has
/// no pixels (e.g. a space), which we also cache so we stop asking.
struct CachedGlyph {
    ox: i32,
    oy: i32,
    w: usize,
    h: usize,
    bytes: Vec<u8>,
    is_emoji: bool,
}

pub struct GpuiRaster {
    platform: Arc<dyn PlatformTextSystem>,
    text: WindowTextSystem,
    base: Font,
    font_size: Pixels,
    /// Device-pixel scale; 2.0 renders at retina resolution.
    scale: f32,
    cell_w_logical: f32,
    cell_h_logical: f32,
    cell_w: usize,
    cell_h: usize,
    /// Rasterized glyphs by `(font, glyph)`. Subpixel position, size, and scale
    /// are fixed, so a glyph rasterizes once and is reused every frame.
    glyphs: RefCell<HashMap<(FontId, GlyphId), Option<CachedGlyph>>>,
}

impl GpuiRaster {
    /// Build a renderer for `base` font at `font_size` logical pixels, producing
    /// output at `scale`x device resolution.
    pub fn new(base: Font, font_size: f32, scale: f32) -> GpuiRaster {
        let platform: Arc<dyn PlatformTextSystem> =
            gpui_platform::current_platform(true).text_system();
        let _ = platform.add_fonts(vec![Cow::Borrowed(REGULAR), Cow::Borrowed(BOLD)]);
        let ts = Arc::new(TextSystem::new(platform.clone()));
        let text = WindowTextSystem::new(ts.clone());

        let font_size = px(font_size.max(4.0));
        let scale = scale.max(1.0);
        let font_id = ts.resolve_font(&base);
        let advance = ts
            .advance(font_id, font_size, 'M')
            .map(|s| f32::from(s.width))
            .unwrap_or_else(|_| f32::from(font_size) * 0.6);
        let cell_w_logical = advance.max(1.0);
        let cell_h_logical = (f32::from(font_size) * LINE_HEIGHT).max(1.0);

        GpuiRaster {
            platform,
            text,
            base,
            font_size,
            scale,
            cell_w_logical,
            cell_h_logical,
            cell_w: (cell_w_logical * scale).round().max(1.0) as usize,
            cell_h: (cell_h_logical * scale).round().max(1.0) as usize,
            glyphs: RefCell::new(HashMap::new()),
        }
    }

    fn font_for(&self, flags: CellFlags) -> Font {
        let mut font = self.base.clone();
        if flags.contains(CellFlags::BOLD) {
            font.weight = FontWeight::BOLD;
        }
        if flags.contains(CellFlags::ITALIC) {
            font.style = FontStyle::Italic;
        }
        font
    }

    /// Shape `text` and composite its glyphs at cell `(row, col)` in `fg`.
    fn paint_text(
        &self,
        out: &mut Rgba,
        row: usize,
        col: usize,
        text: &str,
        flags: CellFlags,
        fg: Rgb,
    ) {
        let run = TextRun {
            len: text.len(),
            font: self.font_for(flags),
            color: gpui::hsla(0.0, 0.0, 0.0, 1.0),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let shaped = self.text.shape_line(
            text.to_string().into(),
            self.font_size,
            &[run],
            Some(px(self.cell_w_logical)),
        );

        let ascent = f32::from(shaped.ascent);
        let descent = f32::from(shaped.descent);
        let padding_top = (self.cell_h_logical - ascent - descent) / 2.0;
        let baseline = padding_top + ascent;
        let origin_x = col as f32 * self.cell_w_logical;
        let origin_y = row as f32 * self.cell_h_logical;

        for glyph_run in shaped.runs.iter() {
            for glyph in glyph_run.glyphs.iter() {
                let pen_x = origin_x + f32::from(glyph.position.x);
                let pen_y = origin_y + baseline + f32::from(glyph.position.y);
                let dev_x = (pen_x * self.scale).round() as i32;
                let dev_y = (pen_y * self.scale).round() as i32;

                let mut cache = self.glyphs.borrow_mut();
                let entry = cache
                    .entry((glyph_run.font_id, glyph.id))
                    .or_insert_with(|| {
                        rasterize(
                            &self.platform,
                            glyph_run.font_id,
                            glyph.id,
                            self.font_size,
                            self.scale,
                            glyph.is_emoji,
                        )
                    });
                if let Some(g) = entry {
                    blit(
                        out,
                        dev_x + g.ox,
                        dev_y + g.oy,
                        g.w,
                        g.h,
                        &g.bytes,
                        fg,
                        g.is_emoji,
                    );
                }
            }
        }
    }

    /// Draw a box-drawing / block-element cell as filled geometry - the app's
    /// box rects, which tile without gaps, rather than the font's glyph.
    fn paint_box(&self, out: &mut Rgba, row: usize, col: usize, ch: char, fg: Rgb) {
        let Some(glyph) = libsinclair::boxdraw::rects(ch, self.cell_w as f32, self.cell_h as f32)
        else {
            return;
        };
        let alpha = (glyph.alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
        let ox = (col * self.cell_w) as i32;
        let oy = (row * self.cell_h) as i32;
        for (x, y, w, h) in glyph.rects {
            let x0 = ox + x.round() as i32;
            let y0 = oy + y.round() as i32;
            let rw = w.round().max(0.0) as i32;
            let rh = h.round().max(0.0) as i32;
            for py in 0..rh {
                for px in 0..rw {
                    out.blend(x0 + px, y0 + py, fg, alpha);
                }
            }
        }
    }

    fn fill_cell_bg(&self, out: &mut Rgba, row: usize, col: usize, cols_wide: usize, color: Rgb) {
        let x = col * self.cell_w;
        let y = row * self.cell_h;
        out.fill_rect(x, y, self.cell_w * cols_wide, self.cell_h, color);
    }
}

impl Raster for GpuiRaster {
    fn cell_size(&self) -> (usize, usize) {
        (self.cell_w, self.cell_h)
    }

    fn frame(&mut self, term: &Terminal, colors: &Colors, out: &mut Rgba) {
        out.fill_bg(colors.bg);
        let rows = term.rows();
        let cols = term.cols();

        // Pass 1: backgrounds. Fill any cell whose resolved background differs
        // from the default, so text composites over the right color.
        for row in 0..rows {
            for col in 0..cols {
                let cell = *term.cell(row, col);
                if cell.flags.contains(CellFlags::WIDE_SPACER) {
                    continue;
                }
                let (_, bg) = resolve(&cell, colors);
                if bg != colors.bg {
                    let wide = if cell.is_wide() { 2 } else { 1 };
                    self.fill_cell_bg(out, row, col, wide, bg);
                }
            }
        }

        // Pass 2: text, merged into same-style runs per row (like the app's
        // snapshot) so shaping sees whole words and ligatures form.
        for row in 0..rows {
            let mut span: Option<(usize, String, CellFlags, Rgb)> = None;
            let flush =
                |out: &mut Rgba,
                 this: &GpuiRaster,
                 span: &mut Option<(usize, String, CellFlags, Rgb)>| {
                    if let Some((col, text, flags, fg)) = span.take() {
                        this.paint_text(out, row, col, &text, flags, fg);
                    }
                };
            for col in 0..cols {
                let cell = *term.cell(row, col);
                if cell.flags.contains(CellFlags::WIDE_SPACER) {
                    continue;
                }
                if cell.flags.contains(CellFlags::INVISIBLE) || cell.ch == ' ' || cell.ch == '\0' {
                    flush(out, self, &mut span);
                    continue;
                }
                if libsinclair::boxdraw::covers(cell.ch) {
                    flush(out, self, &mut span);
                    let (fg, _) = resolve(&cell, colors);
                    self.paint_box(out, row, col, cell.ch, fg);
                    continue;
                }
                let (fg, _) = resolve(&cell, colors);
                let style = cell.flags & STYLE_FLAGS;
                let mut grapheme = String::new();
                cell.write_grapheme(&mut grapheme);
                match &mut span {
                    Some((start, text, flags, sfg))
                        if !cell.is_wide()
                            && *flags == style
                            && *sfg == fg
                            && *start + text.chars().count() == col =>
                    {
                        text.push_str(&grapheme);
                    }
                    _ => {
                        flush(out, self, &mut span);
                        span = Some((col, grapheme, style, fg));
                        if cell.is_wide() {
                            // Wide glyphs shape alone so forced cell-width can't
                            // misplace the run.
                            flush(out, self, &mut span);
                        }
                    }
                }
            }
            flush(out, self, &mut span);
        }

        // Pass 3: cursor block over its cell.
        if term.cursor_visible() {
            let (row, col) = term.cursor_pos();
            if row < rows && col < cols {
                let cell = *term.cell(row, col);
                let wide = if cell.is_wide() { 2 } else { 1 };
                self.fill_cell_bg(out, row, col, wide, colors.cursor);
                if cell.ch != ' ' && cell.ch != '\0' {
                    let mut grapheme = String::new();
                    cell.write_grapheme(&mut grapheme);
                    self.paint_text(
                        out,
                        row,
                        col,
                        &grapheme,
                        cell.flags & STYLE_FLAGS,
                        colors.cursor_text,
                    );
                }
            }
        }
    }
}

/// Style bits that gate span merging (must match to stay in one shaped run).
const STYLE_FLAGS: CellFlags = CellFlags::BOLD
    .union(CellFlags::ITALIC)
    .union(CellFlags::DIM)
    .union(CellFlags::ANY_UNDERLINE)
    .union(CellFlags::STRIKETHROUGH);

/// Resolve a cell's `(fg, bg)` with bold-brighten and inverse, mirroring the
/// software rasterizer and `app::colors::cell_rgb`.
fn resolve(cell: &Cell, colors: &Colors) -> (Rgb, Rgb) {
    let bold = cell.flags.contains(CellFlags::BOLD);
    let mut fg = colors.resolve(cell.fg, colors.fg, bold);
    let mut bg = colors.resolve(cell.bg, colors.bg, false);
    if cell.flags.contains(CellFlags::INVERSE) {
        std::mem::swap(&mut fg, &mut bg);
    }
    (fg, bg)
}

/// Rasterize one glyph via the platform text system at the fixed subpixel
/// origin, size, and scale. `None` when the glyph has no pixels.
fn rasterize(
    platform: &Arc<dyn PlatformTextSystem>,
    font_id: FontId,
    glyph_id: GlyphId,
    font_size: Pixels,
    scale: f32,
    is_emoji: bool,
) -> Option<CachedGlyph> {
    let params = RenderGlyphParams {
        font_id,
        glyph_id,
        font_size,
        subpixel_variant: point(0, 0),
        scale_factor: scale,
        is_emoji,
        subpixel_rendering: false,
        dilation: 0,
    };
    let bounds = platform.glyph_raster_bounds(&params).ok()?;
    if bounds.size.width.0 <= 0 || bounds.size.height.0 <= 0 {
        return None;
    }
    let (size, bytes) = platform.rasterize_glyph(&params, bounds).ok()?;
    Some(CachedGlyph {
        ox: bounds.origin.x.0,
        oy: bounds.origin.y.0,
        w: size.width.0 as usize,
        h: size.height.0 as usize,
        bytes,
        is_emoji,
    })
}

/// Composite a rasterized glyph at device `(x, y)`. Text glyphs are grayscale
/// coverage (1 byte/px) blended as `fg` alpha; emoji are premultiplied BGRA.
#[allow(clippy::too_many_arguments)]
fn blit(out: &mut Rgba, x: i32, y: i32, w: usize, h: usize, bytes: &[u8], fg: Rgb, emoji: bool) {
    if emoji {
        if bytes.len() < w * h * 4 {
            return;
        }
        for row in 0..h {
            for col in 0..w {
                let i = (row * w + col) * 4;
                let (b, g, r, a) = (bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]);
                // Premultiplied BGRA -> straight RGB over the existing pixel.
                out.blend(x + col as i32, y + row as i32, Rgb::new(r, g, b), a);
            }
        }
    } else {
        if bytes.len() < w * h {
            return;
        }
        for row in 0..h {
            for col in 0..w {
                let cov = bytes[row * w + col];
                if cov != 0 {
                    out.blend(x + col as i32, y + row as i32, fg, cov);
                }
            }
        }
    }
}
