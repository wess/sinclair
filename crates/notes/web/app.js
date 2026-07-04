// Notes — a lightweight Obsidian. Vanilla ES module served from the local
// server. Talks to the server over fetch (vault ops) + a WebSocket (external
// changes). The editor is a CodeMirror 6 live-preview surface (../dist/editor.js).

import { createEditor } from "./dist/editor.js";

// Session auth token, handed to us by the app on the boot URL (?token=…). The
// server rejects any /api or /ws request without it.
const TOKEN = new URLSearchParams(location.search).get("token") || "";
const authHeaders = () => (TOKEN ? { authorization: "Bearer " + TOKEN } : {});

const api = {
  get: (p) => fetch("/api" + p, { headers: authHeaders() }).then((r) => r.json()),
  send: (m, p, body) =>
    fetch("/api" + p, {
      method: m,
      headers: { "content-type": "application/json", ...authHeaders() },
      body: JSON.stringify(body || {}),
    }).then((r) => r.json()),
};

const state = {
  vault: null,
  tree: [],
  expanded: new Set(),
  openPath: null,
  content: "",
  dirty: false,
  saveTimer: null,
  editor: null,
  applying: false, // set while pushing external content, to suppress onChange
};

const app = document.getElementById("app");
const el = (tag, cls, txt) => {
  const n = document.createElement(tag);
  if (cls) n.className = cls;
  if (txt != null) n.textContent = txt;
  return n;
};

// ---------------------------------------------------------------- boot

(async function boot() {
  state.vault = await api.get("/vault");
  if (state.vault) state.tree = await api.get("/tree");
  render();
  connectWs();
})();

function connectWs() {
  let ws;
  try {
    ws = new WebSocket(`ws://${location.host}/ws?token=${encodeURIComponent(TOKEN)}`);
  } catch {
    return;
  }
  ws.onmessage = async (ev) => {
    let msg;
    try {
      msg = JSON.parse(ev.data);
    } catch {
      return;
    }
    if (msg.type === "changed") {
      state.tree = await api.get("/tree");
      renderTree();
      // Reload the open note if it changed externally and we have no edits.
      if (state.openPath && !state.dirty && msg.path === state.openPath) {
        const { content } = await api.get(`/file?path=${encodeURIComponent(state.openPath)}`);
        state.content = content;
        if (state.editor) {
          state.applying = true;
          state.editor.setDoc(content);
          state.applying = false;
        }
      }
    }
  };
  ws.onclose = () => setTimeout(connectWs, 1500);
}

// ---------------------------------------------------------------- render

function render() {
  app.innerHTML = "";
  if (!state.vault) return renderPicker();
  renderWorkspace();
}

async function renderPicker() {
  const recents = await api.get("/vaults/recents");
  const card = el("div", "picker-card");
  card.append(el("h1", null, "Notes"));
  card.append(el("p", null, "Open a folder of markdown files, or create a new vault."));

  const actions = el("div", "actions");
  const open = el("button", "primary", "Open folder…");
  open.onclick = () => pickVault("open");
  const create = el("button", null, "New vault…");
  create.onclick = () => pickVault("create");
  actions.append(open, create);
  card.append(actions);

  if (recents.length) {
    card.append(el("div", "recents-title", "Recent"));
    for (const r of recents) {
      const row = el("div", "recent");
      row.append(el("span", null, r.name));
      row.append(el("span", "path", r.path));
      const forget = el("span", "forget", "✕");
      forget.title = "Forget";
      forget.onclick = async (e) => {
        e.stopPropagation();
        await api.send("POST", "/vault/forget", { path: r.path });
        render();
      };
      row.append(forget);
      row.onclick = () => openVault(r.path);
      card.append(row);
    }
  }
  const wrap = el("div", "picker");
  wrap.append(card);
  app.append(wrap);
}

async function pickVault(mode) {
  const v = await api.send("POST", "/vault/pick", { mode });
  if (v && v.root) await afterVaultChange(v);
}
async function openVault(path) {
  const v = await api.send("POST", "/vault/open", { path });
  if (v && v.root) await afterVaultChange(v);
}
async function afterVaultChange(v) {
  state.vault = v;
  state.openPath = null;
  state.content = "";
  state.dirty = false;
  state.tree = await api.get("/tree");
  render();
}

function renderWorkspace() {
  const ws = el("div", "workspace");

  // Tree column.
  const tree = el("div", "tree");
  const head = el("div", "tree-head");
  head.append(el("span", "name", state.vault.name));
  const sw = el("span", "switch", "⇄");
  sw.title = "Switch vault";
  sw.onclick = () => {
    state.vault = null;
    render();
  };
  head.append(sw);
  tree.append(head);
  tree.append(el("div", "tree-scroll"));
  const acts = el("div", "tree-actions");
  const nn = el("button", null, "＋ Note");
  nn.onclick = () => createEntry("file");
  const nf = el("button", null, "＋ Folder");
  nf.onclick = () => createEntry("dir");
  acts.append(nn, nf);
  tree.append(acts);
  ws.append(tree);

  // Editor column.
  ws.append(el("div", "editor"));
  app.append(ws);

  renderTree();
  renderEditor();
}

function renderTree() {
  const scroll = document.querySelector(".tree-scroll");
  if (!scroll) return;
  scroll.innerHTML = "";
  const add = (nodes, depth) => {
    for (const n of nodes) {
      const row = el("div", `row ${n.kind}`);
      row.style.paddingLeft = `${6 + depth * 12}px`;
      if (n.path === state.openPath) row.classList.add("active");
      const tw = el("span", "tw", n.kind === "dir" ? (state.expanded.has(n.path) ? "▾" : "▸") : "•");
      row.append(tw, el("span", "lbl", n.name));
      row.onclick = () => {
        if (n.kind === "dir") {
          state.expanded.has(n.path) ? state.expanded.delete(n.path) : state.expanded.add(n.path);
          renderTree();
        } else {
          openNote(n.path);
        }
      };
      row.ondblclick = () => renameEntry(n);
      scroll.append(row);
      if (n.kind === "dir" && state.expanded.has(n.path) && n.children) add(n.children, depth + 1);
    }
  };
  add(state.tree, 0);
}

async function createEntry(kind) {
  const { path } = await api.send("POST", "/file", { parent: "", kind });
  state.tree = await api.get("/tree");
  renderTree();
  if (kind === "file") openNote(path);
}

async function renameEntry(n) {
  const next = prompt(`Rename ${n.kind === "dir" ? "folder" : "note"}`, n.name);
  if (!next || next === n.name) return;
  const { path } = await api.send("POST", "/file/rename", { path: n.path, title: next });
  if (state.openPath === n.path) state.openPath = path;
  state.tree = await api.get("/tree");
  renderTree();
  renderEditor();
}

// ---------------------------------------------------------------- editor

async function openNote(path) {
  if (state.dirty) await saveNow();
  const { content } = await api.get(`/file?path=${encodeURIComponent(path)}`);
  state.openPath = path;
  state.content = content ?? "";
  state.dirty = false;
  renderTree();
  renderEditor();
}

function renderEditor() {
  const pane = document.querySelector(".editor");
  if (!pane) return;
  if (state.editor) {
    state.editor.destroy();
    state.editor = null;
  }
  pane.innerHTML = "";
  if (!state.openPath) {
    pane.append(el("div", "empty", "Select a note, or create one."));
    return;
  }

  const head = el("div", "editor-head");
  head.append(el("span", "title", state.openPath.replace(/\.md$/i, "")));
  head.append(el("span", "spacer"));
  const dirty = el("span", "dirty", "");
  head.append(dirty);
  const del = el("button", "iconbtn", "🗑");
  del.title = "Delete note";
  del.onclick = () => deleteNote();
  head.append(del);
  pane.append(head);

  const body = el("div", "editor-body");
  pane.append(body);
  state.editor = createEditor(body, {
    doc: state.content,
    onChange: (doc) => {
      if (state.applying) return;
      state.content = doc;
      state.dirty = true;
      dirty.textContent = "●";
      scheduleSave();
    },
    onOpenLink: async (target) => {
      const { path } = await api.get(`/resolve?title=${encodeURIComponent(target)}`);
      state.tree = await api.get("/tree");
      openNote(path);
    },
  });
  state.editor.focus();
}

function scheduleSave() {
  clearTimeout(state.saveTimer);
  state.saveTimer = setTimeout(saveNow, 600);
}
async function saveNow() {
  if (!state.openPath || !state.dirty) return;
  const path = state.openPath;
  const content = state.content;
  await api.send("PUT", "/file", { path, content });
  if (state.openPath === path && state.content === content) {
    state.dirty = false;
    const d = document.querySelector(".editor-head .dirty");
    if (d) d.textContent = "";
  }
}

async function deleteNote() {
  if (!state.openPath) return;
  if (!confirm(`Delete "${state.openPath.replace(/\.md$/i, "")}"?`)) return;
  await api.send("DELETE", "/file", { path: state.openPath });
  state.openPath = null;
  state.content = "";
  state.dirty = false;
  state.tree = await api.get("/tree");
  renderTree();
  renderEditor();
}
