//! Dirty-row tracking so renderers can repaint only what changed.

use std::collections::HashSet;

/// Damage accumulated since the last `take`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Damage {
    /// Everything must be redrawn.
    Full,
    /// Only these visible-row indices changed, sorted ascending.
    /// An empty list means nothing changed.
    Rows(Vec<usize>),
}

impl Damage {
    /// `true` when nothing needs repainting.
    pub fn is_empty(&self) -> bool {
        matches!(self, Damage::Rows(rows) if rows.is_empty())
    }
}

/// Collects dirty rows; whole-screen events escalate to full damage.
///
/// A fresh tracker starts fully damaged so the first frame always paints.
#[derive(Debug, Clone)]
pub struct DamageTracker {
    full: bool,
    rows: HashSet<usize>,
}

impl DamageTracker {
    pub fn new() -> DamageTracker {
        DamageTracker {
            full: true,
            rows: HashSet::new(),
        }
    }

    /// Mark one visible row dirty. No-op while already fully damaged.
    pub fn mark_row(&mut self, row: usize) {
        if !self.full {
            self.rows.insert(row);
        }
    }

    /// Escalate to full damage, dropping per-row records.
    pub fn mark_full(&mut self) {
        self.full = true;
        self.rows.clear();
    }

    pub fn is_full(&self) -> bool {
        self.full
    }

    /// Return the accumulated damage and reset to clean.
    pub fn take(&mut self) -> Damage {
        if self.full {
            self.full = false;
            self.rows.clear();
            Damage::Full
        } else {
            let mut rows: Vec<usize> = self.rows.drain().collect();
            rows.sort_unstable();
            Damage::Rows(rows)
        }
    }
}

impl Default for DamageTracker {
    fn default() -> DamageTracker {
        DamageTracker::new()
    }
}

#[cfg(test)]
#[path = "../../tests/grid/damage.rs"]
mod tests;
