//! Ordered tabs, each owning a pane tree and a focused pane.

use crate::id::PaneId;
use crate::tree::PaneTree;

#[derive(Debug, Clone, PartialEq)]
pub struct Tab {
    pub tree: PaneTree,
    pub focused: PaneId,
    /// User-set tab label; overrides the focused pane's title when present.
    pub title: Option<String>,
}

impl Tab {
    pub fn new(root: PaneId) -> Self {
        Self {
            tree: PaneTree::new(root),
            focused: root,
            title: None,
        }
    }
}

/// Ordered tabs with one active. Always holds at least one tab.
#[derive(Debug, Clone, PartialEq)]
pub struct Tabs {
    tabs: Vec<Tab>,
    active: usize,
}

impl Tabs {
    pub fn new(root: PaneId) -> Self {
        Self {
            tabs: vec![Tab::new(root)],
            active: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.tabs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    pub fn active_index(&self) -> usize {
        self.active
    }

    pub fn active(&self) -> &Tab {
        &self.tabs[self.active]
    }

    pub fn active_mut(&mut self) -> &mut Tab {
        &mut self.tabs[self.active]
    }

    pub fn get(&self, index: usize) -> Option<&Tab> {
        self.tabs.get(index)
    }

    /// Append a tab rooted at `root` and activate it. Returns its index.
    pub fn new_tab(&mut self, root: PaneId) -> usize {
        self.tabs.push(Tab::new(root));
        self.active = self.tabs.len() - 1;
        self.active
    }

    /// Close the tab at `index`. `false` if out of range or it is the last tab.
    pub fn close_tab(&mut self, index: usize) -> bool {
        if self.tabs.len() <= 1 || index >= self.tabs.len() {
            return false;
        }
        self.tabs.remove(index);
        if index < self.active {
            self.active -= 1;
        } else if self.active >= self.tabs.len() {
            self.active = self.tabs.len() - 1;
        }
        true
    }

    /// `false` if `index` is out of range.
    pub fn activate(&mut self, index: usize) -> bool {
        if index >= self.tabs.len() {
            return false;
        }
        self.active = index;
        true
    }

    pub fn activate_next(&mut self) {
        self.active = (self.active + 1) % self.tabs.len();
    }

    pub fn activate_prev(&mut self) {
        self.active = (self.active + self.tabs.len() - 1) % self.tabs.len();
    }

    /// Move the tab at `from` to position `to`, keeping the active tab active.
    /// `false` if either index is out of range.
    pub fn move_tab(&mut self, from: usize, to: usize) -> bool {
        let len = self.tabs.len();
        if from >= len || to >= len {
            return false;
        }
        if from == to {
            return true;
        }
        let tab = self.tabs.remove(from);
        self.tabs.insert(to, tab);
        if self.active == from {
            self.active = to;
        } else if from < self.active && to >= self.active {
            self.active -= 1;
        } else if from > self.active && to <= self.active {
            self.active += 1;
        }
        true
    }

    /// Focus a pane in the active tab. `false` if the pane is not in its tree.
    pub fn focus(&mut self, pane: PaneId) -> bool {
        if !self.active().tree.contains(pane) {
            return false;
        }
        self.active_mut().focused = pane;
        true
    }

    /// The focused pane of the active tab.
    pub fn focused(&self) -> PaneId {
        self.active().focused
    }

    /// Override the label of the tab at `index` (empty/None reverts to the
    /// focused pane's title). `false` if `index` is out of range.
    pub fn set_title(&mut self, index: usize, title: Option<String>) -> bool {
        match self.tabs.get_mut(index) {
            Some(tab) => {
                tab.title = title;
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
#[path = "../tests/tabs.rs"]
mod tests;
