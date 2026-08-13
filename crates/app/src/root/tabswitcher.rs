//! Driving the tab switcher: keeping the most-recently-used order, running the
//! held-modifier gesture, and drawing the overlay. The state machine itself is
//! in [`crate::switcher`], which knows nothing about gpui.

use super::*;
use crate::switcher::{Row, Switcher};
use gpui::prelude::*;

/// How wide the overlay card is allowed to get. Titles are paths, so without a
/// ceiling one deep `cwd` sets the width of the whole list.
const CARD_W: f32 = 420.0;

impl WorkspaceView {
    /// Record `item` as the most recently used. Called wherever the active item
    /// settles rather than from each activation path, so nothing that focuses a
    /// terminal can quietly skip it.
    ///
    /// A no-op while the switcher is open: cycling would otherwise rewrite the
    /// order being walked, and the list would reshuffle under the pointer.
    pub(crate) fn touch_recent(&mut self, item: ItemId) {
        if self.switcher.is_some() {
            return;
        }
        self.recent.retain(|&i| i != item);
        self.recent.insert(0, item);
    }

    /// Drop a closed item from the order. The switcher holds its own snapshot,
    /// so an item closed while it is open is dropped from that too.
    pub(crate) fn forget_recent(&mut self, item: ItemId) {
        self.recent.retain(|&i| i != item);
        if let Some(s) = self.switcher.as_mut() {
            s.order.retain(|&i| i != item);
            if s.order.len() < 2 {
                self.switcher = None;
            } else if s.index >= s.order.len() {
                s.index = 0;
            }
        }
    }

    /// The window's items in most-recently-used order. The stored order is the
    /// truth for everything it covers; anything it has not seen yet (a window
    /// restored from a session, items adopted from a tear-off) is appended in
    /// layout order so the list is always the whole window.
    fn recent_order(&self, cx: &App) -> Vec<ItemId> {
        let live = self.group.read(cx).items();
        let mut order: Vec<ItemId> = self
            .recent
            .iter()
            .copied()
            .filter(|i| live.contains(i))
            .collect();
        let unseen: Vec<ItemId> = live.into_iter().filter(|i| !order.contains(i)).collect();
        order.extend(unseen);
        order
    }

    /// Run the `tab_switcher` binding: open the overlay if it is closed, else
    /// step it along. `step` is the binding's signed parameter.
    pub(crate) fn tab_switcher(&mut self, step: i32, window: &mut Window, cx: &mut Context<Self>) {
        match self.switcher.as_mut() {
            Some(s) => s.advance(step),
            None => {
                let order = self.recent_order(cx);
                // The modifiers still down are what the gesture is held by.
                // Read at open, because by the first repaint the key that
                // triggered this is already on its way up.
                self.switcher = Switcher::open(order, step, window.modifiers());
                if self.switcher.is_none() {
                    return;
                }
            }
        }
        cx.notify();
    }

    /// Commit the switcher: activate what it points at and close it.
    pub(crate) fn commit_switcher(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(s) = self.switcher.take() else {
            return;
        };
        let item = s.selected();
        // Taken above, so the activation below records the item it lands on
        // rather than being suppressed as a cycle step.
        self.activate_item(item, window, cx);
        cx.notify();
    }

    /// Close the switcher without switching (escape, or a click elsewhere).
    pub(crate) fn cancel_switcher(&mut self, cx: &mut Context<Self>) {
        if self.switcher.take().is_some() {
            cx.notify();
        }
    }

    /// A modifier change while the switcher is open: releasing what held it
    /// open is the commit.
    pub(crate) fn switcher_modifiers(
        &mut self,
        now: gpui::Modifiers,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let done = self
            .switcher
            .as_ref()
            .is_some_and(|s| s.held() && !s.still_held(now));
        if done {
            self.commit_switcher(window, cx);
        }
    }

    /// The rows to draw, or `None` while the switcher is closed.
    fn switcher_rows(&self, cx: &App) -> Option<Vec<Row>> {
        let s = self.switcher.as_ref()?;
        // Pane numbers come from the group's traversal order, which is the
        // order the panes are laid out in — so "2" is the second pane you would
        // read, not an internal id.
        let panes = self.group.read(cx).panes_with_items();
        let items = self.items.borrow();
        Some(
            s.order
                .iter()
                .enumerate()
                .map(|(i, id)| Row {
                    item: *id,
                    title: items
                        .get(id)
                        .map(|it| SharedString::from(it.content.title(cx)))
                        .unwrap_or_else(|| SharedString::from("untitled")),
                    pane: panes
                        .iter()
                        .position(|(_, list, _)| list.contains(id))
                        .map_or(0, |p| p + 1),
                    selected: i == s.index,
                })
                .collect(),
        )
    }

    /// The switcher overlay, or nothing while it is closed.
    pub(crate) fn switcher_overlay(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let rows = self.switcher_rows(cx)?;
        let fg = colors::hsla(self.colors.fg);
        let mut bg = colors::hsla(self.colors.bg);
        bg.a = 0.96;
        let mut dim = fg;
        dim.a = 0.55;
        let mut sel = colors::hsla(self.colors.fg);
        sel.a = 0.12;
        let mut edge = fg;
        edge.a = 0.18;
        // Numbered only when there is more than one pane: in a single-pane
        // window every row would carry the same "1", which is noise.
        let multipane = rows.iter().any(|r| r.pane > 1);

        Some(
            div()
                .id("switcher")
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                // A switcher no modifier is holding open (opened from the
                // palette) needs a way out that is not a keystroke.
                .on_click(cx.listener(|this, _ev, _w, cx| this.cancel_switcher(cx)))
                .child(
                    div()
                        .w(px(CARD_W))
                        .max_h(gpui::relative(0.7))
                        .overflow_hidden()
                        .flex()
                        .flex_col()
                        .py(px(6.0))
                        .rounded(px(10.0))
                        .bg(bg)
                        .border_1()
                        .border_color(edge)
                        .shadow_lg()
                        .text_color(fg)
                        .children(rows.into_iter().enumerate().map(|(i, row)| {
                            let item = row.item;
                            div()
                                .id(("switcher-row", i))
                                .flex()
                                .items_center()
                                .gap_2()
                                .mx(px(6.0))
                                .px_2()
                                .py(px(4.0))
                                .rounded(px(6.0))
                                .when(row.selected, |d| d.bg(sel))
                                .hover(|s| s.bg(sel))
                                .on_click(cx.listener(move |this, _ev, window, cx| {
                                    this.switcher = None;
                                    this.activate_item(item, window, cx);
                                }))
                                .when(multipane, |d| {
                                    d.child(
                                        div()
                                            .flex_none()
                                            .text_size(px(10.0))
                                            .text_color(dim)
                                            .child(SharedString::from(row.pane.to_string())),
                                    )
                                })
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .text_size(px(13.0))
                                        .whitespace_nowrap()
                                        .text_ellipsis()
                                        .child(row.title),
                                )
                        })),
                )
                .into_any_element(),
        )
    }
}
