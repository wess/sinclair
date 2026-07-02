//! Tile layouts: named pane arrangements. Built-in presets generate a binary
//! split tree for N panes; custom layouts are captured from the live tab with
//! "Save Current Layout" and stored as JSON. The realizing side (spawning panes
//! and splitting) lives in `root.rs`; this module is pure logic.

use guise::panegroup::Node;
use guise::SplitDirection;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Copy, Serialize, Deserialize)]
pub enum Ax {
    H,
    V,
}

impl Ax {
    pub fn axis(self) -> SplitDirection {
        match self {
            Ax::H => SplitDirection::Horizontal,
            Ax::V => SplitDirection::Vertical,
        }
    }
    fn flip(self) -> Ax {
        match self {
            Ax::H => Ax::V,
            Ax::V => Ax::H,
        }
    }
}

/// A pane arrangement: a binary split tree whose leaves are slots.
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "lowercase")]
pub enum Layout {
    Leaf,
    Split {
        axis: Ax,
        ratio: f32,
        first: Box<Layout>,
        second: Box<Layout>,
    },
}

impl Layout {
    pub fn leaves(&self) -> usize {
        match self {
            Layout::Leaf => 1,
            Layout::Split { first, second, .. } => first.leaves() + second.leaves(),
        }
    }
}

/// Built-in presets: `(id, label, shape, pane-count)`.
pub fn presets() -> &'static [(&'static str, &'static str, &'static str, usize)] {
    &[
        ("columns-2", "Two Columns", "columns", 2),
        ("columns-3", "Three Columns", "columns", 3),
        ("rows-2", "Two Rows", "rows", 2),
        ("grid-4", "Grid (4)", "grid", 4),
        ("main-bottom-4", "Main + Bottom Row", "main-bottom", 4),
        ("main-right-3", "Main + Right Stack", "main-right", 3),
    ]
}

/// Generate the layout for a named shape and pane count.
pub fn generate(shape: &str, n: usize) -> Layout {
    let n = n.max(1);
    match shape {
        "rows" => stack(Ax::V, n),
        "grid" => grid(n),
        "main-bottom" => main(Ax::V, n),
        "main-right" => main(Ax::H, n),
        _ => stack(Ax::H, n),
    }
}

/// N evenly-sized panes along one axis (right/bottom-leaning tree).
fn stack(axis: Ax, n: usize) -> Layout {
    if n <= 1 {
        return Layout::Leaf;
    }
    Layout::Split {
        axis,
        ratio: 1.0 / n as f32,
        first: Box::new(Layout::Leaf),
        second: Box::new(stack(axis, n - 1)),
    }
}

/// One big pane, the rest evenly stacked along the opposite axis.
fn main(axis: Ax, n: usize) -> Layout {
    if n <= 1 {
        return Layout::Leaf;
    }
    Layout::Split {
        axis,
        ratio: 0.6,
        first: Box::new(Layout::Leaf),
        second: Box::new(stack(axis.flip(), n - 1)),
    }
}

fn grid(n: usize) -> Layout {
    if n <= 1 {
        return Layout::Leaf;
    }
    let cols = (n as f64).sqrt().ceil() as usize;
    let rows = n.div_ceil(cols);
    let base = n / rows;
    let rem = n % rows;
    let counts: Vec<usize> = (0..rows)
        .map(|i| if i < rem { base + 1 } else { base })
        .collect();
    rows_of(&counts)
}

/// A vertical stack of rows, each row being evenly-sized columns.
fn rows_of(counts: &[usize]) -> Layout {
    match counts {
        [] => Layout::Leaf,
        [only] => stack(Ax::H, *only),
        [head, tail @ ..] => Layout::Split {
            axis: Ax::V,
            ratio: 1.0 / counts.len() as f32,
            first: Box::new(stack(Ax::H, *head)),
            second: Box::new(rows_of(tail)),
        },
    }
}

/// Capture the current pane tree as a layout (structure + ratios, no content).
pub fn from_tree(node: &Node) -> Layout {
    match node {
        Node::Leaf(_) => Layout::Leaf,
        Node::Split {
            axis,
            ratio,
            first,
            second,
            ..
        } => Layout::Split {
            axis: match axis {
                SplitDirection::Horizontal => Ax::H,
                SplitDirection::Vertical => Ax::V,
            },
            ratio: *ratio,
            first: Box::new(from_tree(first)),
            second: Box::new(from_tree(second)),
        },
    }
}

/// Resolve a tile id (preset or saved custom) to its layout and pane count.
pub fn resolve(id: &str) -> Option<(Layout, usize)> {
    if let Some((_, _, shape, count)) = presets().iter().find(|p| p.0 == id) {
        return Some((generate(shape, *count), *count));
    }
    let text = std::fs::read_to_string(layouts_dir().join(format!("{id}.json"))).ok()?;
    let layout: Layout = serde_json::from_str(&text).ok()?;
    let n = layout.leaves();
    Some((layout, n))
}

pub fn layouts_dir() -> PathBuf {
    config::default_path()
        .and_then(|p| p.parent().map(|d| d.join("layouts")))
        .unwrap_or_else(|| PathBuf::from("layouts"))
}

pub fn list_custom() -> Vec<String> {
    let mut names = Vec::new();
    if let Ok(entries) = std::fs::read_dir(layouts_dir()) {
        for e in entries.flatten() {
            let path = e.path();
            if path.extension().and_then(|x| x.to_str()) == Some("json") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    names.push(stem.to_string());
                }
            }
        }
    }
    names.sort();
    names
}

/// Turn a display name into a filesystem-safe id.
pub fn slug(name: &str) -> String {
    let s: String = name
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '.' { c } else { '-' })
        .collect();
    s.trim_matches('-').to_string()
}

pub fn save(name: &str, layout: &Layout) -> std::io::Result<PathBuf> {
    let dir = layouts_dir();
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{}.json", slug(name)));
    std::fs::write(&path, serde_json::to_string_pretty(layout).unwrap_or_default())?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_match() {
        assert_eq!(generate("columns", 3).leaves(), 3);
        assert_eq!(generate("rows", 4).leaves(), 4);
        assert_eq!(generate("grid", 4).leaves(), 4);
        assert_eq!(generate("grid", 5).leaves(), 5);
        assert_eq!(generate("main-bottom", 4).leaves(), 4);
        assert_eq!(generate("main-right", 3).leaves(), 3);
        assert_eq!(generate("columns", 1).leaves(), 1);
    }

    #[test]
    fn main_bottom_is_top_over_row() {
        match generate("main-bottom", 4) {
            Layout::Split { axis, first, second, .. } => {
                assert!(matches!(axis, Ax::V));
                assert_eq!(first.leaves(), 1);
                assert_eq!(second.leaves(), 3);
            }
            _ => panic!("expected a split"),
        }
    }

    #[test]
    fn presets_generate() {
        for (id, _, _, count) in presets() {
            let (layout, n) = resolve(id).expect("preset resolves");
            assert_eq!(n, *count);
            assert_eq!(layout.leaves(), *count);
        }
    }

    #[test]
    fn slugify() {
        assert_eq!(slug("My Cool Layout"), "my-cool-layout");
        assert_eq!(slug("  spaced  "), "spaced");
    }
}
