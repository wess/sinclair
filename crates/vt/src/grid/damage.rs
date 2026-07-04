//! Dirty-row tracking so renderers can repaint only what changed.

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
///
/// Rows are tracked with a row-indexed bitmap rather than a hashed set:
/// `mark_row` fires at least once per printed cell, but the distinct dirty
/// rows are bounded by the terminal height, so a dense bitmap avoids hashing
/// the same row index thousands of times per frame.
#[derive(Debug, Clone)]
pub struct DamageTracker {
    full: bool,
    rows: Vec<bool>,
}

impl DamageTracker {
    pub fn new() -> DamageTracker {
        DamageTracker {
            full: true,
            rows: Vec::new(),
        }
    }

    /// Mark one visible row dirty. No-op while already fully damaged.
    pub fn mark_row(&mut self, row: usize) {
        if !self.full {
            if row >= self.rows.len() {
                self.rows.resize(row + 1, false);
            }
            self.rows[row] = true;
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
            // Bitmap scan yields indices already ascending — no sort needed.
            let rows: Vec<usize> = self
                .rows
                .iter()
                .enumerate()
                .filter_map(|(i, &dirty)| dirty.then_some(i))
                .collect();
            self.rows.clear();
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
