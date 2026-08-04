//! OSC 8 hyperlinks. Cells carry a small [`HyperlinkId`] interned in a
//! per-terminal [`Hyperlinks`] registry so a link survives scrolling into
//! history without storing the URI on every cell.

use std::collections::HashMap;
use std::num::NonZeroU16;

/// Individual OSC 8 fields and the whole registry are bounded so hostile or
/// long-lived output cannot retain arbitrary strings after their rows leave
/// scrollback.
pub(crate) const MAX_URI_BYTES: usize = 8 * 1024;
pub(crate) const MAX_ID_BYTES: usize = 1024;
const MAX_REGISTRY_BYTES: usize = 4 * 1024 * 1024;

/// A handle to an interned hyperlink. Stored on cells (2 bytes via the
/// `NonZeroU16` niche inside `Option<HyperlinkId>`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HyperlinkId(NonZeroU16);

impl HyperlinkId {
    /// Raw id for the scrollback codec; never 0.
    pub(crate) fn as_u16(self) -> u16 {
        self.0.get()
    }

    /// Rebuild from a codec id; 0 means no link.
    pub(crate) fn from_u16(raw: u16) -> Option<HyperlinkId> {
        NonZeroU16::new(raw).map(HyperlinkId)
    }
}

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
    /// String payload retained by both `links` and `interned`.
    bytes: usize,
}

impl Hyperlinks {
    /// Intern a link, returning its id. Identical links return the same id.
    /// `None` only if the registry is full (more than `u16::MAX` links).
    pub fn intern(&mut self, id: Option<String>, uri: String) -> Option<HyperlinkId> {
        if uri.len() > MAX_URI_BYTES || id.as_ref().is_some_and(|id| id.len() > MAX_ID_BYTES) {
            return None;
        }
        let key = (id, uri);
        if let Some(&hid) = self.interned.get(&key) {
            return Some(hid);
        }
        let retained = (key.0.as_ref().map_or(0, String::len) + key.1.len()).saturating_mul(2);
        if self.bytes.saturating_add(retained) > MAX_REGISTRY_BYTES {
            return None;
        }
        let next = u16::try_from(self.links.len() + 1).ok()?;
        let hid = HyperlinkId(NonZeroU16::new(next)?);
        self.links.push(Hyperlink {
            id: key.0.clone(),
            uri: key.1.clone(),
        });
        self.interned.insert(key, hid);
        self.bytes += retained;
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
        self.bytes = 0;
    }
}

#[cfg(test)]
#[path = "../tests/hyperlink.rs"]
mod tests;
