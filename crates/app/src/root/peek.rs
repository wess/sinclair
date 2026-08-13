//! The tab peek: a strip of live miniatures of every tab in the window,
//! dropped under the tab bar.
//!
//! The strip is drawn *over* the splits rather than above them, so opening it
//! never changes a pane's size. A resize would reflow every scrollback line in
//! every tab and hand the programs inside a new width — a heavy, visible price
//! for a glance. Covering the top few rows costs nothing and is undone by
//! closing the strip.
//!
//! It opens two ways, and they behave differently on purpose. The keybinding
//! and the menu *pin* it: it stays until dismissed and takes the arrow keys,
//! because you asked for it and your hands are on the keyboard. Resting the
//! pointer on the tab bar opens the same strip unpinned: it follows the
//! pointer out again and never touches a key, because you did not ask for it
//! and the shell below is still where your typing goes.

use std::time::Duration;

use super::*;
use gpui::prelude::*;
use libsinclair::preview::Preview;

/// The title row above each miniature, in px.
const CARD_HEADER: f32 = 18.0;

/// Padding around the strip's row of cards, in px.
const STRIP_PAD: f32 = 8.0;

/// Card width bounds. A very wide grid would otherwise make one card the whole
/// strip, and a very tall one would shrink to a sliver.
const CARD_MIN_W: f32 = 90.0;
const CARD_MAX_W: f32 = 340.0;

/// Width of a card with nothing to preview (a plugin web view).
const CARD_PLAIN_W: f32 = 150.0;

/// How long a hover-opened strip waits, after the pointer has left both it and
/// the tab bar, before closing. Without the grace the strip would vanish in
/// the gap the pointer crosses on its way from the tab bar into the strip.
const CLOSE_GRACE: Duration = Duration::from_millis(180);

/// How many frames a scroll request is repeated for. Two: the frame that
/// drops it, and the one that can act on it (see [`Peek::scroll_ticks`]).
const SCROLL_TICKS: u8 = 2;

/// The open peek.
pub(crate) struct Peek {
    /// Opened by the keybinding or the menu: it stays until dismissed, and it
    /// takes the arrow keys. A hover-opened peek does neither.
    pub(crate) pinned: bool,
    /// The card the keyboard is on, as an index into [`WorkspaceView::peek_order`].
    index: usize,
    /// The strip's horizontal scroll. A window with more tabs than fit opens
    /// on the one you are in, and the arrow keys pull the rest into view.
    scroll: gpui::ScrollHandle,
    /// Frames left to keep asking for that scroll. gpui consumes a
    /// `scroll_to_item` during prepaint but can only act on it once the
    /// container's overflow has been recorded, which happens later in the same
    /// frame — so the first request after the strip appears is always dropped.
    /// Asking again on the next frame is what makes an opening strip land on
    /// the tab you are in. Counted down rather than held, so a request never
    /// outlives its moment and fights the pointer scrolling the strip.
    scroll_ticks: u8,
}

/// The strip's palette, resolved once per frame. Cards say three things with
/// it: which tab you are in, which one the keyboard is on, and which one the
/// pointer is over — and they have to say them while the miniature inside is
/// doing all the talking, so the marks are edges and a title fill rather than
/// anything that competes with the content.
struct CardPaint {
    fg: gpui::Hsla,
    dim: gpui::Hsla,
    /// An unmarked card.
    edge: gpui::Hsla,
    /// The tab you would return to by closing the strip.
    current: gpui::Hsla,
    /// The card the keyboard is on (also the hover edge).
    selected: gpui::Hsla,
    /// Fill behind a marked card's title.
    header: gpui::Hsla,
}

impl CardPaint {
    fn new(fg: gpui::Hsla) -> Self {
        let alpha = |a: f32| gpui::Hsla { a, ..fg };
        Self {
            fg,
            dim: alpha(0.6),
            edge: alpha(0.14),
            current: alpha(0.45),
            selected: alpha(0.9),
            header: alpha(0.1),
        }
    }
}

impl Peek {
    fn open(pinned: bool, index: usize) -> Self {
        Self {
            pinned,
            index,
            scroll: gpui::ScrollHandle::new(),
            scroll_ticks: SCROLL_TICKS,
        }
    }
}

impl WorkspaceView {
    /// The tabs the strip shows, in the order the window lays them out —
    /// left-to-right, pane by pane. Deliberately *not* most-recently-used
    /// order: the whole point is to look at the shape of the window, and a
    /// list that reshuffles itself between glances has no shape to learn.
    pub(crate) fn peek_order(&self, cx: &App) -> Vec<ItemId> {
        self.group.read(cx).items()
    }

    /// Where the tab you are in sits in that order: what a fresh strip opens
    /// on, whichever way it was opened.
    fn peek_active_index(&self, cx: &App) -> usize {
        let active = self.active_item(cx);
        self.peek_order(cx)
            .iter()
            .position(|i| *i == active)
            .unwrap_or(0)
    }

    /// Run the `tab_peek` binding: open the strip pinned, or close it.
    pub(crate) fn toggle_peek(&mut self, cx: &mut Context<Self>) {
        if !self.opts.tab_peek {
            return;
        }
        match self.peek.take() {
            Some(peek) if peek.pinned => {}
            // A hover-opened strip is already on screen; the binding pins it
            // rather than closing what it just asked to see.
            _ => {
                let index = self.peek_active_index(cx);
                self.peek = Some(Peek::open(true, index));
            }
        }
        cx.notify();
    }

    /// Close the strip, however it was opened.
    pub(crate) fn close_peek(&mut self, cx: &mut Context<Self>) {
        if self.peek.take().is_some() {
            cx.notify();
        }
    }

    /// Move the keyboard selection by `delta` cards, wrapping.
    pub(crate) fn step_peek(&mut self, delta: i32, cx: &mut Context<Self>) {
        let len = self.peek_order(cx).len();
        let Some(peek) = self.peek.as_mut() else {
            return;
        };
        if len == 0 {
            return;
        }
        let next = (peek.index as i32 + delta).rem_euclid(len as i32);
        peek.index = next as usize;
        peek.scroll_ticks = SCROLL_TICKS;
        cx.notify();
    }

    /// Activate the selected card and close the strip.
    pub(crate) fn commit_peek(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(peek) = self.peek.take() else {
            return;
        };
        if let Some(item) = self.peek_order(cx).get(peek.index).copied() {
            self.activate_item(item, window, cx);
        }
        cx.notify();
    }

    /// The pointer entered or left the tab bar.
    pub(crate) fn peek_hover_bar(&mut self, hovered: bool, cx: &mut Context<Self>) {
        self.peek_over_bar = hovered;
        if hovered {
            self.arm_peek_open(cx);
        } else {
            self.arm_peek_close(cx);
        }
    }

    /// The pointer entered or left the open strip.
    pub(crate) fn peek_hover_strip(&mut self, hovered: bool, cx: &mut Context<Self>) {
        self.peek_over_strip = hovered;
        if !hovered {
            self.arm_peek_close(cx);
        }
    }

    /// Schedule a hover open, if hover opening is configured and the strip is
    /// not already up.
    fn arm_peek_open(&mut self, cx: &mut Context<Self>) {
        let delay = self.opts.tab_peek_hover;
        if !self.opts.tab_peek || delay == 0 || self.peek.is_some() {
            return;
        }
        self.peek_gen = self.peek_gen.wrapping_add(1);
        let generation = self.peek_gen;
        let timer = cx
            .background_executor()
            .timer(Duration::from_millis(delay as u64));
        cx.spawn(async move |this, cx| {
            timer.await;
            let _ = this.update(cx, |this, cx| {
                // The pointer moved on (or something else opened the strip)
                // while the delay ran: whatever the state is now, it is not
                // the one this open was asked for.
                if this.peek_gen != generation || !this.peek_over_bar || this.peek.is_some() {
                    return;
                }
                let index = this.peek_active_index(cx);
                this.peek = Some(Peek::open(false, index));
                cx.notify();
            });
        })
        .detach();
    }

    /// Schedule the close check that runs when the pointer leaves either half
    /// of the hover region.
    fn arm_peek_close(&mut self, cx: &mut Context<Self>) {
        self.peek_gen = self.peek_gen.wrapping_add(1);
        let generation = self.peek_gen;
        let timer = cx.background_executor().timer(CLOSE_GRACE);
        cx.spawn(async move |this, cx| {
            timer.await;
            let _ = this.update(cx, |this, cx| {
                if this.peek_gen != generation || this.peek_over_bar || this.peek_over_strip {
                    return;
                }
                if this.peek.as_ref().is_some_and(|p| !p.pinned) {
                    this.peek = None;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// The invisible band over the tab bar that opens the strip on hover.
    /// Deliberately not occluding: it sits on top of the real tabs, and they
    /// still have to be clickable through it.
    pub(crate) fn peek_hover_band(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.opts.tab_peek || self.opts.tab_peek_hover == 0 {
            return None;
        }
        Some(
            div()
                .id("peek-band")
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .h(px(TAB_HEIGHT))
                .on_hover(cx.listener(|this, hovered: &bool, _window, cx| {
                    this.peek_hover_bar(*hovered, cx)
                }))
                .into_any_element(),
        )
    }

    /// The strip itself, or nothing while it is closed.
    pub(crate) fn peek_strip(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.peek.as_ref()?;
        let order = self.peek_order(cx);
        if order.is_empty() {
            return None;
        }
        // Every card is a live terminal; nothing else in the window notifies
        // this view when one of them paints. While the strip is up it simply
        // draws every frame, which is what "live" costs and why it is only
        // ever up for a moment.
        window.request_animation_frame();

        let active = self.active_item(cx);
        let active_index = self.peek_active_index(cx);
        let peek = self.peek.as_mut()?;
        let selected = peek.pinned.then(|| peek.index.min(order.len() - 1));
        if peek.scroll_ticks > 0 {
            peek.scroll_ticks -= 1;
            peek.scroll.scroll_to_item(selected.unwrap_or(active_index));
        }
        let scroll = peek.scroll.clone();
        let card_h = self.opts.tab_peek_height as f32;

        let fg = colors::hsla(self.colors.fg);
        let mut bg = colors::hsla(self.colors.bg);
        bg.a = 0.98;
        let paint = CardPaint::new(fg);

        let cards: Vec<AnyElement> = order
            .iter()
            .enumerate()
            .map(|(i, item)| {
                self.peek_card(
                    *item,
                    i,
                    card_h,
                    *item == active,
                    selected == Some(i),
                    &paint,
                    cx,
                )
            })
            .collect();

        Some(
            div()
                .id("peek-strip")
                .absolute()
                .top(px(TAB_HEIGHT))
                .left_0()
                .right_0()
                .flex()
                .flex_row()
                .gap(px(STRIP_PAD))
                .p(px(STRIP_PAD))
                .overflow_x_scroll()
                .track_scroll(&scroll)
                .bg(bg)
                .border_b_1()
                .border_color(paint.edge)
                .shadow_lg()
                // Unlike the hover band, the strip is a surface: a click on a
                // card must not also land in the terminal behind it.
                .occlude()
                .on_hover(cx.listener(|this, hovered: &bool, _window, cx| {
                    this.peek_hover_strip(*hovered, cx)
                }))
                // Going back to work is a dismissal: a pinned strip would
                // otherwise sit over the terminal you just clicked into.
                .on_mouse_down_out(cx.listener(|this, _ev, _window, cx| this.close_peek(cx)))
                .children(cards)
                .into_any_element(),
        )
    }

    /// One card: the tab's title over a live miniature of its screen.
    #[allow(clippy::too_many_arguments)]
    fn peek_card(
        &self,
        item: ItemId,
        index: usize,
        card_h: f32,
        active: bool,
        selected: bool,
        paint: &CardPaint,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let body_h = (card_h - CARD_HEADER).max(20.0);
        let items = self.items.borrow();
        let entry = items.get(&item);
        let title = entry
            .map(|it| it.content.title(cx))
            .unwrap_or_else(|| "untitled".to_string());
        let title = match entry {
            Some(it) if !it.content.has_title_override(cx) && !self.opts.tab_title_show_host => {
                crate::tabbar::strip_host(&title).to_string()
            }
            _ => title,
        };
        let dot = entry.and_then(|it| it.agent).map(|st| colors::hsla(st.color()));
        let session = entry
            .and_then(|it| it.content.as_terminal())
            .map(|v| v.read(cx).session().clone());
        drop(items);

        let width = match &session {
            Some(session) => (body_h * libsinclair::preview::aspect(session, self.cell))
                .clamp(CARD_MIN_W, CARD_MAX_W),
            None => CARD_PLAIN_W,
        };
        let body = match session {
            Some(session) => div()
                .size_full()
                .child(Preview::new(session, self.colors.clone(), self.cell))
                .into_any_element(),
            // A web view has no grid to reduce; the card keeps its place in
            // the strip and says what it is.
            None => div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(10.0))
                .text_color(paint.dim)
                .child(SharedString::from("web view"))
                .into_any_element(),
        };

        let group = SharedString::from(format!("peek-card-{}", index));
        let marked = active || selected;
        let border = if selected {
            paint.selected
        } else if active {
            paint.current
        } else {
            paint.edge
        };
        div()
            .id(("peek-card", index))
            .group(group.clone())
            .flex_none()
            .w(px(width))
            .h(px(card_h))
            .flex()
            .flex_col()
            .rounded(px(8.0))
            .overflow_hidden()
            .border_1()
            .border_color(border)
            .hover(|s| s.border_color(paint.selected))
            .on_click(cx.listener(move |this, _ev, window, cx| {
                this.peek = None;
                this.activate_item(item, window, cx);
            }))
            .child(
                div()
                    .flex_none()
                    .h(px(CARD_HEADER))
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .px(px(5.0))
                    .text_size(px(10.0))
                    .when(marked, |d| d.bg(paint.header))
                    .text_color(if marked { paint.fg } else { paint.dim })
                    .children(dot.map(|color| {
                        div()
                            .flex_none()
                            .size(px(5.0))
                            .rounded_full()
                            .bg(color)
                            .into_any_element()
                    }))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(SharedString::from(title)),
                    )
                    .child(
                        div()
                            .id(("peek-close", index))
                            .flex_none()
                            .px(px(2.0))
                            .invisible()
                            .group_hover(group, |s| s.visible())
                            .text_color(paint.dim)
                            .child(SharedString::from("\u{00d7}"))
                            .on_click(cx.listener(move |this, _ev, window, cx| {
                                cx.stop_propagation();
                                this.close_item(item, window, cx);
                            })),
                    ),
            )
            .child(div().flex_1().min_h(px(0.0)).child(body))
            .into_any_element()
    }
}
