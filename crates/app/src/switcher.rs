//! The tab switcher: a held-modifier overlay that walks the window's items in
//! most-recently-used order.
//!
//! `next_tab` walks the strip, which is the order tabs happen to sit in; this
//! walks the order they were last looked at, so the tab you came from is always
//! one press away no matter where it sits or which pane it is in. That is the
//! whole reason the list exists — a switcher over strip order would just be
//! `next_tab` with a picture.
//!
//! The gesture is the alt-tab one: the first press opens the overlay already
//! pointing at the *previous* item (so tap-and-release is a straight there-and-
//! back), each further press advances the pointer, and letting go of the
//! modifier commits. Nothing is activated while cycling — a preview would
//! reorder the very list being walked, and every terminal it passed over would
//! take a focus change it did not ask for.

use gpui::{Modifiers, SharedString};
use guise::panegroup::ItemId;

/// The state behind an open switcher.
#[derive(Debug)]
pub struct Switcher {
    /// The MRU order snapshotted when the switcher opened. Held rather than
    /// re-read so the list cannot reorder under the pointer mid-cycle.
    pub order: Vec<ItemId>,
    /// Where the pointer sits in `order`.
    pub index: usize,
    /// The modifiers held when the switcher opened. Releasing them commits;
    /// empty when the switcher was opened by something that holds nothing down
    /// (the command palette), which is what [`Self::held`] is asked about.
    pub holding: Modifiers,
}

impl Switcher {
    /// Open over `order`, pointing at the item `step` places along. `holding`
    /// is what was down when the gesture started.
    pub fn open(order: Vec<ItemId>, step: i32, holding: Modifiers) -> Option<Self> {
        // One item is not a thing to switch between, and an empty window has
        // nothing to show.
        if order.len() < 2 {
            return None;
        }
        let mut s = Self {
            order,
            index: 0,
            holding,
        };
        s.advance(step);
        Some(s)
    }

    /// Move the pointer `step` places, wrapping at both ends.
    pub fn advance(&mut self, step: i32) {
        let len = self.order.len() as i32;
        if len == 0 {
            return;
        }
        self.index = (self.index as i32 + step).rem_euclid(len) as usize;
    }

    /// The item the switcher would commit to right now.
    pub fn selected(&self) -> ItemId {
        self.order[self.index]
    }

    /// Whether the switcher is being held open by a modifier at all. A switcher
    /// opened from the palette has nothing to release, so it stays up until it
    /// is committed or dismissed by hand.
    pub fn held(&self) -> bool {
        let m = self.holding;
        m.control || m.alt || m.shift || m.platform || m.function
    }

    /// Whether `now` still holds the switcher open: every modifier the gesture
    /// started with has to stay down. Shift is deliberately excluded — cycling
    /// backwards is shift+the same binding, so letting shift go mid-cycle is a
    /// direction change, not a release.
    pub fn still_held(&self, now: Modifiers) -> bool {
        let want = self.holding;
        (!want.control || now.control)
            && (!want.alt || now.alt)
            && (!want.platform || now.platform)
            && (!want.function || now.function)
    }
}

/// One row of the switcher overlay.
pub struct Row {
    pub item: ItemId,
    pub title: SharedString,
    /// The pane this item lives in, 1-based in the group's traversal order, so
    /// two same-named shells in different splits can be told apart.
    pub pane: usize,
    pub selected: bool,
}

#[cfg(test)]
#[path = "../tests/switcher.rs"]
mod tests;
