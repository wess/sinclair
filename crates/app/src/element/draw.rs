use super::*;

use gpui::{
    DispatchPhase, FontStyle, FontWeight, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    ScrollWheelEvent, StrikethroughStyle, TextRun, UnderlineStyle,
};
use vt::CellFlags;

use crate::pointer::{self, Pointer};

impl TerminalElement {
    pub(crate) fn text_run(&self, span: &Span) -> TextRun {
        let mut font = self.font.clone();
        if span.flags.contains(CellFlags::BOLD) {
            font.weight = FontWeight::BOLD;
        }
        if span.flags.contains(CellFlags::ITALIC) {
            font.style = FontStyle::Italic;
        }
        let mut color = colors::hsla(span.fg);
        if span.flags.contains(CellFlags::DIM) {
            color.a *= 0.7;
        }
        let underline = span
            .flags
            .intersects(CellFlags::ANY_UNDERLINE)
            .then(|| UnderlineStyle {
                thickness: px(1.0),
                color: Some(color),
                wavy: span.flags.contains(CellFlags::CURLY_UNDERLINE),
            });
        let strikethrough =
            span.flags
                .contains(CellFlags::STRIKETHROUGH)
                .then(|| StrikethroughStyle {
                    thickness: px(1.0),
                    color: Some(color),
                });
        TextRun {
            len: span.text.len(),
            font,
            color,
            background_color: None,
            underline,
            strikethrough,
        }
    }

    pub(crate) fn cursor_frame(
        &self,
        snap: &CursorSnap,
        origin: Point<Pixels>,
        window: &mut Window,
    ) -> CursorFrame {
        let cell_w = px(self.cell.width);
        let cell_h = px(self.cell.height);
        let cell_origin = point(
            origin.x + cell_w * snap.col as f32,
            origin.y + cell_h * snap.row as f32,
        );
        let width = if snap.wide { cell_w * 2.0 } else { cell_w };
        let color = colors::hsla(snap.color);
        if !self.focused {
            // Unfocused: a hollow block outline whatever the shape, so the
            // cursor stays findable across splits and windows while the
            // filled cursor still marks the active pane unambiguously.
            return CursorFrame {
                bounds: Bounds::new(cell_origin, size(width, cell_h)),
                color,
                glyph: None,
                hollow: true,
            };
        }
        // Bar/underline thickness scales with the cell so large fonts don't
        // get a hairline cursor.
        let thick = px((self.cell.width / 6.0).max(2.0));
        match cursor_shape(snap.style, self.cursor_default) {
            config::CursorStyle::Bar => CursorFrame {
                bounds: Bounds::new(cell_origin, size(thick, cell_h)),
                color,
                glyph: None,
                hollow: false,
            },
            config::CursorStyle::Underline => CursorFrame {
                bounds: Bounds::new(
                    point(cell_origin.x, cell_origin.y + cell_h - thick),
                    size(width, thick),
                ),
                color,
                glyph: None,
                hollow: false,
            },
            config::CursorStyle::Block => {
                let glyph = (snap.ch != ' ').then(|| {
                    let text = snap.ch.to_string();
                    let run = TextRun {
                        len: text.len(),
                        font: self.font.clone(),
                        color: colors::hsla(snap.text_color),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    };
                    let line = window.text_system().shape_line(
                        text.into(),
                        self.font_size,
                        &[run],
                        Some(cell_w),
                    );
                    (cell_origin, line)
                });
                CursorFrame {
                    bounds: Bounds::new(cell_origin, size(width, cell_h)),
                    color,
                    glyph,
                    hollow: false,
                }
            }
        }
    }

    /// Register this frame's mouse listeners. Down/wheel events are gated
    /// on the bounds; move/up follow drags anywhere in the window.
    pub(crate) fn register_pointer(
        &self,
        bounds: Bounds<Pixels>,
        grid: (usize, usize),
        window: &mut Window,
    ) {
        let pointer = Pointer {
            session: self.session.clone(),
            state: self.mouse.clone(),
            bounds,
            pad: self.pad,
            cell: self.cell,
            cols: grid.0,
            rows: grid.1,
            copy_on_select: self.copy_on_select,
            smart_select: self.smart_select,
            middle_click_paste: self.middle_click_paste,
        };
        window.on_mouse_event({
            let p = pointer.clone();
            move |e: &MouseDownEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble {
                    pointer::down(&p, e, window, cx);
                }
            }
        });
        window.on_mouse_event({
            let p = pointer.clone();
            move |e: &MouseMoveEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble {
                    pointer::moved(&p, e, window, cx);
                }
            }
        });
        window.on_mouse_event({
            let p = pointer.clone();
            move |e: &MouseUpEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble {
                    pointer::up(&p, e, window, cx);
                }
            }
        });
        window.on_mouse_event({
            move |e: &ScrollWheelEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble {
                    pointer::wheel(&pointer, e, window, cx);
                }
            }
        });
    }
}
