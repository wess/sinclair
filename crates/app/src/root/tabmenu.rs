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
    /// over the item that was clicked, and the bulk-close entries have to be
    /// hidden when there is nothing for them to close.
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
        // Every bulk close is scoped to the strip the tab was right-clicked in.
        // Tabs live per-pane, so the tabs a menu talks about are the ones beside
        // the tab it was opened from — reaching into the other panes from here
        // would close terminals the user cannot even see from this menu.
        let strip = self.tab_strip_of(item, cx);
        let index = strip.iter().position(|i| *i == item);
        let others = strip.len() > 1;
        let to_right = index.is_some_and(|i| i + 1 < strip.len());
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
        let close_all = move |doomed: Vec<ItemId>| {
            move |_w: &mut Window, cx: &mut App| {
                let doomed = doomed.clone();
                cx.defer(move |cx| {
                    let _ = handle.update(cx, |view, window, cx| {
                        for id in doomed {
                            view.close_item(id, window, cx);
                        }
                    });
                });
            }
        };
        let rest: Vec<ItemId> = strip.iter().copied().filter(|i| *i != item).collect();
        let after: Vec<ItemId> = index.map(|i| strip[i + 1..].to_vec()).unwrap_or_default();

        let menu = cx.new(|cx| {
            let mut menu = guise::ContextMenu::new(cx)
                .item("Rename Tab\u{2026}", run(Action::ChangeTabTitle))
                .divider()
                .item("Split Right", run(Action::NewSplit(SplitDirection::Right)))
                .item("Split Down", run(Action::NewSplit(SplitDirection::Down)))
                .divider();
            if others {
                menu = menu.item("Close Other Tabs", close_all(rest));
            }
            if to_right {
                menu = menu.item("Close Tabs to the Right", close_all(after));
            }
            menu.danger_item("Close Tab", run(Action::CloseTab))
        });
        menu.update(cx, |m, cx| m.show(position, window, cx));
        self.tab_menu = Some(menu);
        cx.notify();
    }

    /// The items in the tab strip `item` sits in, in strip order. Empty if the
    /// item has already left the group.
    fn tab_strip_of(&self, item: ItemId, cx: &App) -> Vec<ItemId> {
        let group = self.group.read(cx);
        group
            .pane_of(item)
            .and_then(|pane| group.pane_items(pane).map(<[_]>::to_vec))
            .unwrap_or_default()
    }
}
