//! The tab context menu.
//!
//! Renaming a tab, splitting from it, and closing it were all reachable only
//! from the View menu or the command palette — the two places nobody looks
//! when their hand is already on a tab. Right-click is where people expect
//! these, so this builds the menu the pane group asks for when a tab is
//! right-clicked (`PaneGroupEvent::ContextMenu`).
//!
//! The group reports the gesture and nothing else: what a tab's menu offers is
//! the host's business, which is why the entries are assembled here.

use super::*;
use gpui::prelude::*;

impl WorkspaceView {
    /// Open the context menu for `item` at the pointer.
    ///
    /// The menu is rebuilt each time rather than kept around: its entries close
    /// over the item that was clicked, and "Close Other Tabs" has to be hidden
    /// when there are no others.
    pub(crate) fn open_tab_menu(
        &mut self,
        item: ItemId,
        position: gpui::Point<gpui::Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(handle) = window.window_handle().downcast::<Self>() else {
            return;
        };
        let others = self.group.read(cx).items().len() > 1;
        // Item handlers run *inside* this window's update, so the action is
        // deferred: dispatching straight into the same window re-enters it,
        // the update is refused, and the menu closes having done nothing.
        let run = move |action: Action| {
            move |_w: &mut Window, cx: &mut App| {
                let action = action.clone();
                cx.defer(move |cx| {
                    let _ = handle.update(cx, |view, w, cx| view.dispatch(action, w, cx));
                });
            }
        };
        let close_others = move |_w: &mut Window, cx: &mut App| {
            cx.defer(move |cx| {
                let _ = handle
                    .update(cx, |view, window, cx| view.close_other_items(item, window, cx));
            });
        };

        let menu = cx.new(|cx| {
            let mut menu = guise::ContextMenu::new(cx)
                .item("Rename Tab\u{2026}", run(Action::ChangeTabTitle))
                .divider()
                .item("Split Right", run(Action::NewSplit(SplitDirection::Right)))
                .item("Split Down", run(Action::NewSplit(SplitDirection::Down)))
                .divider();
            if others {
                menu = menu.item("Close Other Tabs", close_others);
            }
            menu.danger_item("Close Tab", run(Action::CloseTab))
        });
        menu.update(cx, |m, cx| m.show(position, window, cx));
        self.tab_menu = Some(menu);
        cx.notify();
    }

    /// Close every item except `keep`. Collected first: closing mutates the
    /// group, so iterating it while closing would walk a list being rewritten.
    pub(crate) fn close_other_items(
        &mut self,
        keep: ItemId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let doomed: Vec<ItemId> = self
            .group
            .read(cx)
            .items()
            .into_iter()
            .filter(|id| *id != keep)
            .collect();
        for id in doomed {
            self.close_item(id, window, cx);
        }
    }
}
