//! OSC 8 hyperlinks. Cells carry a small [`HyperlinkId`] interned in a
//! per-terminal [`Hyperlinks`] registry so a link survives scrolling into
//! history without storing the URI on every cell.

use std::collections::HashMap;
use std::num::NonZeroU16;

/// A handle to an interned hyperlink. Stored on cells (2 bytes via the
/// `NonZeroU16` niche inside `Option<HyperlinkId>`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HyperlinkId(NonZeroU16);

/// One hyperlink target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hyperlink {
    /// The optional `id=` parameter, which groups discontiguous spans of
    /// the same logical link (for hover highlighting).
    pub id: Option<String>,
    /// The link target (URI).
    pub uri: String,
}

/// Interns hyperlinks so identical `(id, uri)` pairs share one id.
#[derive(Debug, Default)]
pub struct Hyperlinks {
    /// Indexed by `HyperlinkId` - 1.
    links: Vec<Hyperlink>,
    interned: HashMap<(Option<String>, String), HyperlinkId>,
}

impl Hyperlinks {
    /// Intern a link, returning its id. Identical links return the same id.
    /// `None` only if the registry is full (more than `u16::MAX` links).
    pub fn intern(&mut self, id: Option<String>, uri: String) -> Option<HyperlinkId> {
        let key = (id, uri);
        if let Some(&hid) = self.interned.get(&key) {
            return Some(hid);
        }
        let next = u16::try_from(self.links.len() + 1).ok()?;
        let hid = HyperlinkId(NonZeroU16::new(next)?);
        self.links.push(Hyperlink {
            id: key.0.clone(),
            uri: key.1.clone(),
        });
        self.interned.insert(key, hid);
        Some(hid)
    }

    /// Resolve an id back to its link.
    pub fn get(&self, id: HyperlinkId) -> Option<&Hyperlink> {
        self.links.get(id.0.get() as usize - 1)
    }

    /// Drop every interned link (RIS).
    pub fn clear(&mut self) {
        self.links.clear();
        self.interned.clear();
    }
}

#[cfg(test)]
#[path = "../tests/hyperlink.rs"]
mod tests;
