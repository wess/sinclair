//! A pane: a tabbed container of items (splits contain tabs).
//! Pure logic over opaque [`ItemId`]s; the host maps them to terminals/webviews.
//! A pane always holds at least one item.

use crate::id::ItemId;

/// An ordered set of tab items with one active. Always non-empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pane {
    items: Vec<ItemId>,
    active: usize,
}

impl Pane {
    /// A new pane holding a single item, active.
    pub fn new(item: ItemId) -> Self {
        Self {
            items: vec![item],
            active: 0,
        }
    }

    pub fn items(&self) -> &[ItemId] {
        &self.items
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn active(&self) -> ItemId {
        self.items[self.active]
    }

    pub fn active_index(&self) -> usize {
        self.active
    }

    pub fn index_of(&self, item: ItemId) -> Option<usize> {
        self.items.iter().position(|&i| i == item)
    }

    pub fn contains(&self, item: ItemId) -> bool {
        self.items.contains(&item)
    }

    /// Activate the item at `index`. `false` if out of range.
    pub fn activate(&mut self, index: usize) -> bool {
        if index >= self.items.len() {
            return false;
        }
        self.active = index;
        true
    }

    /// Activate a specific item. `false` if absent.
    pub fn activate_item(&mut self, item: ItemId) -> bool {
        match self.index_of(item) {
            Some(i) => {
                self.active = i;
                true
            }
            None => false,
        }
    }

    pub fn activate_next(&mut self) {
        self.active = (self.active + 1) % self.items.len();
    }

    pub fn activate_prev(&mut self) {
        self.active = (self.active + self.items.len() - 1) % self.items.len();
    }

    /// Insert `item` at `at` (clamped; appended when `None`) and activate it.
    /// Returns the index it landed at. Ignores items already present (activates
    /// the existing one instead).
    pub fn add(&mut self, item: ItemId, at: Option<usize>) -> usize {
        if let Some(i) = self.index_of(item) {
            self.active = i;
            return i;
        }
        let index = at.unwrap_or(self.items.len()).min(self.items.len());
        self.items.insert(index, item);
        self.active = index;
        index
    }

    /// Remove `item`. Returns `true` if the pane is now empty (the host should
    /// drop the pane). Keeps a valid active index otherwise.
    pub fn remove(&mut self, item: ItemId) -> bool {
        let Some(i) = self.index_of(item) else {
            return self.items.is_empty();
        };
        self.items.remove(i);
        if self.items.is_empty() {
            return true;
        }
        if self.active >= self.items.len() {
            self.active = self.items.len() - 1;
        } else if i < self.active {
            self.active -= 1;
        }
        false
    }

    /// Move the item at `from` to `to` (both clamped), keeping the active item
    /// active. `false` if `from` is out of range.
    pub fn reorder(&mut self, from: usize, to: usize) -> bool {
        if from >= self.items.len() {
            return false;
        }
        let to = to.min(self.items.len() - 1);
        if from == to {
            return true;
        }
        let active_item = self.items[self.active];
        let item = self.items.remove(from);
        self.items.insert(to, item);
        self.active = self.index_of(active_item).unwrap_or(self.active);
        true
    }
}

#[cfg(test)]
#[path = "../tests/pane.rs"]
mod tests;
