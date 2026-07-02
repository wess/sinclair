//! Vault core: a vault is a folder of markdown files. Pure `std::fs`, no DB.
//! Paths in the API are vault-relative POSIX strings (`""` is the root); they
//! are resolved against the vault root with traversal guards. Ported from the
//! original Bun `vault.ts`.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct Node {
    pub path: String,
    pub name: String,
    pub kind: &'static str, // "file" | "dir"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<Node>>,
}

#[derive(Serialize)]
pub struct VaultInfo {
    pub root: String,
    pub name: String,
}

#[derive(Serialize, Deserialize)]
pub struct Recent {
    pub path: String,
    pub name: String,
    pub opened: u64,
}

/// Directories never shown in the tree.
const HIDDEN: &[&str] = &[".git", "node_modules", ".obsidian", ".DS_Store"];

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default()
}

fn config_dir() -> PathBuf {
    home().join(".config").join("prompt").join("notes")
}

fn recents_file() -> PathBuf {
    config_dir().join("vaults.json")
}

fn current_file() -> PathBuf {
    config_dir().join("current.json")
}

fn base_name(p: &Path) -> String {
    p.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn read_json<T: for<'de> Deserialize<'de>>(file: &Path, fallback: T) -> T {
    fs::read_to_string(file)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(fallback)
}

/// The open vault, plus its recents/current persistence.
#[derive(Default)]
pub struct Vault {
    root: Option<PathBuf>,
}

impl Vault {
    pub fn new() -> Self {
        Self::default()
    }

    // --- recents --------------------------------------------------------

    pub fn recents(&self) -> Vec<Recent> {
        read_json::<Vec<Recent>>(&recents_file(), Vec::new())
            .into_iter()
            .filter(|r| Path::new(&r.path).exists())
            .collect()
    }

    fn remember_recent(&self, dir: &Path) {
        let _ = fs::create_dir_all(config_dir());
        let dir_s = dir.to_string_lossy().into_owned();
        let mut list: Vec<Recent> = self.recents().into_iter().filter(|r| r.path != dir_s).collect();
        list.insert(
            0,
            Recent {
                path: dir_s,
                name: base_name(dir),
                opened: now_ms(),
            },
        );
        list.truncate(20);
        let _ = fs::write(recents_file(), serde_json::to_vec(&list).unwrap_or_default());
    }

    pub fn forget_recent(&self, dir: &str) {
        let kept: Vec<Recent> = self.recents().into_iter().filter(|r| r.path != dir).collect();
        let _ = fs::write(recents_file(), serde_json::to_vec(&kept).unwrap_or_default());
    }

    // --- open / current -------------------------------------------------

    pub fn current(&mut self) -> Option<VaultInfo> {
        if self.root.is_none() {
            // Restore the last-opened vault on a cold start.
            #[derive(Deserialize)]
            struct Saved {
                root: Option<String>,
            }
            let saved: Saved = read_json(&current_file(), Saved { root: None });
            if let Some(r) = saved.root {
                if Path::new(&r).exists() {
                    self.root = Some(PathBuf::from(r));
                }
            }
        }
        self.root.as_ref().map(|r| VaultInfo {
            root: r.to_string_lossy().into_owned(),
            name: base_name(r),
        })
    }

    pub fn open(&mut self, dir: &str) -> Result<VaultInfo, String> {
        let p = PathBuf::from(dir);
        if !p.is_dir() {
            return Err(format!("not a folder: {dir}"));
        }
        self.root = Some(p.clone());
        let _ = fs::create_dir_all(config_dir());
        let _ = fs::write(
            current_file(),
            serde_json::json!({ "root": p.to_string_lossy() }).to_string(),
        );
        self.remember_recent(&p);
        Ok(VaultInfo {
            root: p.to_string_lossy().into_owned(),
            name: base_name(&p),
        })
    }

    pub fn create(&mut self, dir: &str) -> Result<VaultInfo, String> {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        self.open(dir)
    }

    // --- path safety ----------------------------------------------------

    fn abs(&self, rel: &str) -> Result<PathBuf, String> {
        let root = self.root.as_ref().ok_or("no vault open")?;
        // Reject absolute paths and any `..` component (traversal guard).
        let rel = rel.trim_start_matches('/');
        for comp in rel.split('/') {
            if comp == ".." {
                return Err("path escapes vault".into());
            }
        }
        Ok(root.join(rel))
    }

    // --- tree -----------------------------------------------------------

    pub fn tree(&self) -> Result<Vec<Node>, String> {
        let root = self.root.as_ref().ok_or("no vault open")?;
        Ok(walk(root, root))
    }

    // --- file ops -------------------------------------------------------

    pub fn read(&self, rel: &str) -> Result<String, String> {
        fs::read_to_string(self.abs(rel)?).map_err(|e| e.to_string())
    }

    pub fn write(&self, rel: &str, content: &str) -> Result<(), String> {
        let p = self.abs(rel)?;
        if let Some(parent) = p.parent() {
            let _ = fs::create_dir_all(parent);
        }
        fs::write(p, content).map_err(|e| e.to_string())
    }

    fn unique_path(&self, dir_rel: &str, base: &str, ext: &str) -> Result<String, String> {
        let mut n = 0;
        loop {
            let name = if n == 0 {
                format!("{base}{ext}")
            } else {
                format!("{base} {n}{ext}")
            };
            let rel = if dir_rel.is_empty() {
                name
            } else {
                format!("{dir_rel}/{name}")
            };
            if !self.abs(&rel)?.exists() {
                return Ok(rel);
            }
            n += 1;
        }
    }

    pub fn create_file(&self, parent_rel: &str, kind: &str) -> Result<String, String> {
        if kind == "dir" {
            let rel = self.unique_path(parent_rel, "New Folder", "")?;
            fs::create_dir_all(self.abs(&rel)?).map_err(|e| e.to_string())?;
            return Ok(rel);
        }
        let rel = self.unique_path(parent_rel, "Untitled", ".md")?;
        self.write(&rel, "# Untitled\n\n")?;
        Ok(rel)
    }

    pub fn remove(&self, rel: &str) -> Result<(), String> {
        let p = self.abs(rel)?;
        let res = if p.is_dir() {
            fs::remove_dir_all(&p)
        } else {
            fs::remove_file(&p)
        };
        // `force`-style: ignore "not found".
        match res {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }

    pub fn rename(&self, rel: &str, title: &str) -> Result<String, String> {
        let p = self.abs(rel)?;
        let is_dir = p.is_dir();
        let ext = if is_dir { "" } else { ".md" };
        let clean = title.replace(['/', '\\'], "-");
        let clean = clean.trim_end_matches(".md").trim_end_matches(".MD").trim();
        let clean = if clean.is_empty() { "Untitled" } else { clean };
        let parent = Path::new(rel).parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
        let dest = if parent.is_empty() {
            format!("{clean}{ext}")
        } else {
            format!("{parent}/{clean}{ext}")
        };
        fs::rename(&p, self.abs(&dest)?).map_err(|e| e.to_string())?;
        Ok(dest)
    }

    pub fn move_to(&self, from_rel: &str, to_dir_rel: &str) -> Result<String, String> {
        let name = base_name(&self.abs(from_rel)?);
        let dest = if to_dir_rel.is_empty() {
            name
        } else {
            format!("{to_dir_rel}/{name}")
        };
        fs::rename(self.abs(from_rel)?, self.abs(&dest)?).map_err(|e| e.to_string())?;
        Ok(dest)
    }

    /// Resolve a `[[wiki-link]]` target to a vault path, creating it if missing.
    pub fn resolve(&self, title: &str) -> Result<String, String> {
        let want = title.trim_end_matches(".md").trim_end_matches(".MD").to_lowercase();
        let mut flat = Vec::new();
        collect_files(&self.tree()?, &mut flat);
        if let Some(hit) = flat.into_iter().find(|(_, name)| name.to_lowercase() == want) {
            return Ok(hit.0);
        }
        let cleaned = title.replace(['/', '\\'], "-");
        let cleaned = cleaned.trim();
        let base = if cleaned.is_empty() { "Untitled" } else { cleaned };
        let rel = self.unique_path("", base, ".md")?;
        self.write(&rel, &format!("# {title}\n\n"))?;
        Ok(rel)
    }
}

/// Recursively build the tree under `dir`, folders first then files, each
/// alphabetical; only `.md` files and non-hidden folders.
fn walk(root: &Path, dir: &Path) -> Vec<Node> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut nodes: Vec<Node> = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || HIDDEN.contains(&name.as_str()) {
            continue;
        }
        let child = entry.path();
        let Ok(meta) = entry.metadata() else { continue };
        let rel = child
            .strip_prefix(root)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        if meta.is_dir() {
            nodes.push(Node {
                path: rel,
                name,
                kind: "dir",
                children: Some(walk(root, &child)),
            });
        } else if name.to_lowercase().ends_with(".md") {
            let stem = name[..name.len() - 3].to_string();
            nodes.push(Node {
                path: rel,
                name: stem,
                kind: "file",
                children: None,
            });
        }
    }
    nodes.sort_by(|a, b| match (a.kind, b.kind) {
        ("dir", "file") => std::cmp::Ordering::Less,
        ("file", "dir") => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });
    nodes
}

/// Flatten the tree to `(path, name)` for file nodes only.
fn collect_files(nodes: &[Node], out: &mut Vec<(String, String)>) {
    for n in nodes {
        match &n.children {
            Some(children) => collect_files(children, out),
            None => out.push((n.path.clone(), n.name.clone())),
        }
    }
}
