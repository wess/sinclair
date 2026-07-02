// Vault core: a vault is a folder of markdown files. Pure Bun/node:fs, no DB.
// Paths in the API are vault-relative POSIX strings ("" is the root); they are
// resolved against the vault root with traversal guards.

import { join, dirname, basename, relative, sep } from "node:path";
import { homedir } from "node:os";
import {
  mkdirSync,
  existsSync,
  readFileSync,
  writeFileSync,
  readdirSync,
  statSync,
  rmSync,
  renameSync,
} from "node:fs";

const CONFIG_DIR = join(homedir(), ".config", "prompt", "notes");
const RECENTS = join(CONFIG_DIR, "vaults.json");
const CURRENT = join(CONFIG_DIR, "current.json");

export type Node = {
  path: string; // vault-relative
  name: string;
  kind: "file" | "dir";
  children?: Node[];
};

export type VaultInfo = { root: string; name: string };

let root: string | null = null;

// --- recents ------------------------------------------------------------

type Recent = { path: string; name: string; opened: number };

function readJson<T>(file: string, fallback: T): T {
  try {
    return JSON.parse(readFileSync(file, "utf8")) as T;
  } catch {
    return fallback;
  }
}

export function recents(): Recent[] {
  return readJson<Recent[]>(RECENTS, []).filter((r) => existsSync(r.path));
}

function rememberRecent(dir: string): void {
  mkdirSync(CONFIG_DIR, { recursive: true });
  const list = recents().filter((r) => r.path !== dir);
  list.unshift({ path: dir, name: basename(dir), opened: Date.now() });
  writeFileSync(RECENTS, JSON.stringify(list.slice(0, 20)));
}

export function forgetRecent(dir: string): void {
  writeFileSync(RECENTS, JSON.stringify(recents().filter((r) => r.path !== dir)));
}

// --- open / current -----------------------------------------------------

export function current(): VaultInfo | null {
  if (!root) {
    // Restore the last-opened vault on a cold server start.
    const saved = readJson<{ root?: string }>(CURRENT, {});
    if (saved.root && existsSync(saved.root)) root = saved.root;
  }
  return root ? { root, name: basename(root) } : null;
}

export function open(dir: string): VaultInfo {
  if (!existsSync(dir) || !statSync(dir).isDirectory()) {
    throw new Error(`not a folder: ${dir}`);
  }
  root = dir;
  mkdirSync(CONFIG_DIR, { recursive: true });
  writeFileSync(CURRENT, JSON.stringify({ root }));
  rememberRecent(dir);
  return { root, name: basename(dir) };
}

export function create(dir: string): VaultInfo {
  mkdirSync(dir, { recursive: true });
  return open(dir);
}

// --- path safety --------------------------------------------------------

function abs(rel: string): string {
  if (!root) throw new Error("no vault open");
  const p = join(root, rel);
  const rp = relative(root, p);
  if (rp.startsWith("..") || rp.startsWith(sep + "..") || rp === "..") {
    throw new Error("path escapes vault");
  }
  return p;
}

const HIDDEN = new Set([".git", "node_modules", ".obsidian", ".DS_Store"]);

// --- tree ---------------------------------------------------------------

export function tree(): Node[] {
  if (!root) throw new Error("no vault open");
  const walk = (dirAbs: string): Node[] => {
    let entries: string[];
    try {
      entries = readdirSync(dirAbs);
    } catch {
      return [];
    }
    const nodes: Node[] = [];
    for (const name of entries) {
      if (name.startsWith(".") || HIDDEN.has(name)) continue;
      const childAbs = join(dirAbs, name);
      let st;
      try {
        st = statSync(childAbs);
      } catch {
        continue;
      }
      const rel = relative(root!, childAbs).split(sep).join("/");
      if (st.isDirectory()) {
        nodes.push({ path: rel, name, kind: "dir", children: walk(childAbs) });
      } else if (name.toLowerCase().endsWith(".md")) {
        nodes.push({ path: rel, name: name.replace(/\.md$/i, ""), kind: "file" });
      }
    }
    // Folders first, then files, each alphabetical.
    nodes.sort((a, b) =>
      a.kind === b.kind ? a.name.localeCompare(b.name) : a.kind === "dir" ? -1 : 1,
    );
    return nodes;
  };
  return walk(root);
}

// --- file ops -----------------------------------------------------------

export function read(rel: string): string {
  return readFileSync(abs(rel), "utf8");
}

export function write(rel: string, content: string): void {
  const p = abs(rel);
  mkdirSync(dirname(p), { recursive: true });
  writeFileSync(p, content);
}

function uniquePath(dirRel: string, base: string, ext: string): string {
  let n = 0;
  for (;;) {
    const name = n === 0 ? `${base}${ext}` : `${base} ${n}${ext}`;
    const rel = (dirRel ? `${dirRel}/` : "") + name;
    if (!existsSync(abs(rel))) return rel;
    n++;
  }
}

export function createFile(parentRel: string, kind: "file" | "dir"): string {
  if (kind === "dir") {
    const rel = uniquePath(parentRel, "New Folder", "");
    mkdirSync(abs(rel), { recursive: true });
    return rel;
  }
  const rel = uniquePath(parentRel, "Untitled", ".md");
  write(rel, "# Untitled\n\n");
  return rel;
}

export function remove(rel: string): void {
  rmSync(abs(rel), { recursive: true, force: true });
}

export function rename(rel: string, title: string): string {
  const p = abs(rel);
  const isDir = statSync(p).isDirectory();
  const ext = isDir ? "" : ".md";
  const clean = title.replace(/[\/\\]/g, "-").replace(/\.md$/i, "").trim() || "Untitled";
  const dest = join(dirname(rel), clean + ext).split(sep).join("/");
  renameSync(p, abs(dest));
  return dest;
}

export function move(fromRel: string, toDirRel: string): string {
  const name = basename(abs(fromRel));
  const dest = (toDirRel ? `${toDirRel}/` : "") + name;
  renameSync(abs(fromRel), abs(dest));
  return dest;
}

// Resolve a [[wiki-link]] target to a vault path, creating the note if missing.
export function resolve(title: string): string {
  const want = title.replace(/\.md$/i, "").toLowerCase();
  const flat = (nodes: Node[]): Node[] =>
    nodes.flatMap((n) => (n.kind === "dir" ? flat(n.children || []) : [n]));
  const hit = flat(tree()).find((n) => n.name.toLowerCase() === want);
  if (hit) return hit.path;
  const rel = uniquePath("", title.replace(/[\/\\]/g, "-").trim() || "Untitled", ".md");
  write(rel, `# ${title}\n\n`);
  return rel;
}
