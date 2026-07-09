//! The custom gpui element that paints the terminal grid.
//!
//! Painting happens in three layers: per-cell background rects
//! (merged into horizontal runs), shaped text runs grouped by style, then
//! the cursor. Cell data is snapshotted quickly under the terminal lock;
//! shaping happens after the lock is released.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    fill, point, px, relative, size, App, Bounds, ContentMask, Corners, Element, ElementId,
    GlobalElementId, Hsla, InspectorElementId, IntoElement, LayoutId, Pixels, Point, RenderImage,
    ShapedLine, Style, TextAlign, Window,
};
use terminal::Session;

use crate::colors::{self, Colors};
use crate::metrics::{self, CellSize, Padding};
use crate::mouse::MouseState;

#[cfg(test)]
use theme::Rgb;
#[cfg(test)]
use vt::CellFlags;

mod draw;
mod snapshot;

pub(crate) use snapshot::*;

/// Active search overlay query, recomputed against the live buffer each
/// frame so highlights never go stale.
#[derive(Clone)]
pub struct SearchQuery {
    pub query: String,
    pub current: usize,
    /// Precomputed (and view-cached) matches, so the renderer never re-scans
    /// the buffer itself.
    pub matches: Vec<vt::Match>,
}

pub struct TerminalElement {
    session: Arc<Session>,
    colors: Rc<Colors>,
    font: gpui::Font,
    font_size: Pixels,
    cell: CellSize,
    pad: Padding,
    cursor_default: config::CursorStyle,
    mouse: Rc<RefCell<MouseState>>,
    copy_on_select: bool,
    smart_select: bool,
    middle_click_paste: bool,
    /// Whether this pane holds keyboard focus; an unfocused pane paints a
    /// hollow cursor, so the active pane is unambiguous in a split.
    focused: bool,
    search: Option<SearchQuery>,
    /// Autosuggestion ghost suffix drawn dimmed at the cursor, if any.
    ghost: Option<String>,
    /// GPU textures for decoded sixel images, keyed by placement id and shared
    /// with the view so they survive across frames.
    image_cache: Rc<RefCell<HashMap<u64, Arc<RenderImage>>>>,
}

impl TerminalElement {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        session: Arc<Session>,
        colors: Rc<Colors>,
        font: gpui::Font,
        font_size: Pixels,
        cell: CellSize,
        pad: Padding,
        cursor_default: config::CursorStyle,
        mouse: Rc<RefCell<MouseState>>,
        copy_on_select: bool,
        smart_select: bool,
        middle_click_paste: bool,
        focused: bool,
        search: Option<SearchQuery>,
        ghost: Option<String>,
        image_cache: Rc<RefCell<HashMap<u64, Arc<RenderImage>>>>,
    ) -> Self {
        Self {
            session,
            colors,
            font,
            font_size,
            cell,
            pad,
            cursor_default,
            mouse,
            copy_on_select,
            smart_select,
            middle_click_paste,
            focused,
            search,
            ghost,
            image_cache,
        }
    }
}

pub struct Frame {
    bg_quads: Vec<(Bounds<Pixels>, Hsla)>,
    box_quads: Vec<(Bounds<Pixels>, Hsla)>,
    lines: Vec<(Point<Pixels>, ShapedLine)>,
    cursor: Option<CursorFrame>,
    /// Dimmed autosuggestion ghost text at the cursor.
    ghost: Option<(Point<Pixels>, ShapedLine)>,
    indicator: Option<Bounds<Pixels>>,
    /// Sixel images, as positioned pixel bounds plus their texture.
    images: Vec<(Bounds<Pixels>, Arc<RenderImage>)>,
    /// Grid size at prepaint, for pointer hit testing.
    grid: (usize, usize),
}

pub(crate) struct CursorFrame {
    bounds: Bounds<Pixels>,
    color: Hsla,
    glyph: Option<(Point<Pixels>, ShapedLine)>,
    /// Outline only (unfocused pane): the glyph beneath shows through.
    hollow: bool,
}

impl Element for TerminalElement {
    type RequestLayoutState = ();
    type PrepaintState = Frame;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = relative(1.0).into();
        style.size.height = relative(1.0).into();
        (window.request_layout(style, None, cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut (),
        window: &mut Window,
        _cx: &mut App,
    ) -> Frame {
        let origin = point(
            bounds.origin.x + px(self.pad.x),
            bounds.origin.y + px(self.pad.y),
        );
        let (cols, rows) = metrics::grid_size(
            f32::from(bounds.size.width),
            f32::from(bounds.size.height),
            self.pad,
            self.cell,
        );

        let current = self.session.with_term(|term| (term.cols(), term.rows()));
        if current != (cols, rows) {
            let _ = self.session.resize(cols, rows);
        }

        let hover_link = self.mouse.borrow().hover_link;
        let snap = {
            let mut cache = self.image_cache.borrow_mut();
            self.session.with_term(|term| {
                snapshot(term, &self.colors, self.search.as_ref(), self.cell, &mut cache, hover_link)
            })
        };

        let cell_w = px(self.cell.width);
        let cell_h = px(self.cell.height);

        let images = snap
            .images
            .iter()
            .filter_map(|img| {
                let row = img.line + snap.offset as isize;
                if row < 0 || row as usize >= rows {
                    return None;
                }
                let pos = point(
                    origin.x + cell_w * img.col as f32,
                    origin.y + cell_h * row as f32,
                );
                let bounds = Bounds::new(pos, size(px(img.width), px(img.height)));
                Some((bounds, img.image.clone()))
            })
            .collect();
        let bg_quads = snap
            .bg_runs
            .iter()
            .map(|run| {
                let pos = point(
                    origin.x + cell_w * run.col as f32,
                    origin.y + cell_h * run.row as f32,
                );
                (
                    Bounds::new(pos, size(cell_w * run.len as f32, cell_h)),
                    colors::hsla(run.color),
                )
            })
            .collect();

        let box_quads = snap
            .boxes
            .iter()
            .flat_map(|b| {
                let glyph = crate::boxdraw::rects(b.ch, self.cell.width, self.cell.height);
                let cell_origin = point(
                    origin.x + cell_w * b.col as f32,
                    origin.y + cell_h * b.row as f32,
                );
                let mut color = colors::hsla(b.fg);
                glyph
                    .map(move |g| {
                        color.a *= g.alpha;
                        g.rects.into_iter().map(move |(x, y, w, h)| {
                            (
                                Bounds::new(
                                    point(cell_origin.x + px(x), cell_origin.y + px(y)),
                                    size(px(w), px(h)),
                                ),
                                color,
                            )
                        })
                    })
                    .into_iter()
                    .flatten()
            })
            .collect();

        let lines = snap
            .spans
            .iter()
            .map(|span| {
                let run = self.text_run(span);
                let line = window.text_system().shape_line(
                    span.text.clone().into(),
                    self.font_size,
                    &[run],
                    Some(cell_w),
                );
                let pos = point(
                    origin.x + cell_w * span.col as f32,
                    origin.y + cell_h * span.row as f32,
                );
                (pos, line)
            })
            .collect();

        // The focused pane draws a filled cursor; an unfocused pane keeps a
        // hollow outline so the cursor stays findable after a focus switch.
        let cursor = snap
            .cursor
            .as_ref()
            .filter(|c| c.row < rows && c.col < cols)
            .map(|c| self.cursor_frame(c, origin, window));

        // Ghost text: dimmed suggestion suffix starting at the cursor cell.
        let ghost = self.ghost.as_ref().filter(|g| !g.is_empty()).and_then(|g| {
            let c = snap.cursor.as_ref().filter(|c| c.row < rows && c.col < cols)?;
            let mut color = colors::hsla(self.colors.fg);
            color.a *= 0.4;
            let run = gpui::TextRun {
                len: g.len(),
                font: self.font.clone(),
                color,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let line =
                window
                    .text_system()
                    .shape_line(g.clone().into(), self.font_size, &[run], None);
            let pos = point(origin.x + cell_w * c.col as f32, origin.y + cell_h * c.row as f32);
            Some((pos, line))
        });

        Frame {
            bg_quads,
            box_quads,
            lines,
            cursor,
            ghost,
            indicator: scroll_indicator(&bounds, rows, snap.offset, snap.scrollback),
            images,
            grid: (cols, rows),
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut (),
        frame: &mut Frame,
        window: &mut Window,
        cx: &mut App,
    ) {
        let line_height = px(self.cell.height);
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            window.paint_quad(fill(bounds, colors::hsla(self.colors.bg)));
            for (quad, color) in &frame.bg_quads {
                window.paint_quad(fill(*quad, *color));
            }
            for (quad, color) in &frame.box_quads {
                window.paint_quad(fill(*quad, *color));
            }
            for (bounds, image) in &frame.images {
                window
                    .paint_image(*bounds, Corners::default(), image.clone(), 0, false)
                    .ok();
            }
            for (pos, line) in &frame.lines {
                line.paint(*pos, line_height, TextAlign::Left, None, window, cx)
                    .ok();
            }
            if let Some((pos, line)) = &frame.ghost {
                line.paint(*pos, line_height, TextAlign::Left, None, window, cx)
                    .ok();
            }
            if let Some(cursor) = &frame.cursor {
                if cursor.hollow {
                    window.paint_quad(gpui::quad(
                        cursor.bounds,
                        Corners::default(),
                        gpui::transparent_black(),
                        gpui::Edges::all(px(1.5)),
                        cursor.color,
                        gpui::BorderStyle::default(),
                    ));
                } else {
                    window.paint_quad(fill(cursor.bounds, cursor.color));
                    if let Some((pos, glyph)) = &cursor.glyph {
                        glyph
                            .paint(*pos, line_height, TextAlign::Left, None, window, cx)
                            .ok();
                    }
                }
            }
            if let Some(indicator) = frame.indicator {
                let mut color = colors::hsla(self.colors.fg);
                color.a = 0.4;
                window.paint_quad(fill(indicator, color));
            }
        });

        self.register_pointer(bounds, frame.grid, window);
    }
}

impl IntoElement for TerminalElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

#[cfg(test)]
#[path = "../../tests/element.rs"]
mod tests;
