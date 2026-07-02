# Notes

A lightweight Obsidian for Prompt: a **vault** (a folder of markdown files) with
a **file/folder tree** and a **markdown editor with live preview**, hosted as a
**tab**. Requires `bun`.

## How it works

The plugin is a web view opened in a tab, backed by a small local server:

- `[webview]` sets `boot = true`, so the app invokes the `[runtime]`'s `boot`
  method (from Rust) *before* loading the page.
- `runtime/launcher.ts` makes sure the **server** is running (reusing it via a
  pidfile, or spawning it detached) and returns its `{ port }`.
- The app then loads the tab at `http://127.0.0.1:{port}/`, served by
  `server/main.ts` — a persistent Bun HTTP + WebSocket server that owns the
  vault. The page talks to it over `fetch` (vault ops) and a `WebSocket`
  (external-change push). The server idle-shuts-down ~60s after the tab closes.

A live vault needs a long-lived process (to watch files and push updates), which
the serverless plugin `[runtime]` can't be — hence the launcher-plus-server
split. Loading from a served `http` origin (rather than `file://`) also lets the
bundled SPA use ES modules and `fetch`.

## Layout

```
plugin.toml            # [runtime] launcher + [webview] boot = true, placement = tab
runtime/launcher.ts    # boot handshake: ensure the server, return its port
server/
  main.ts              # Bun.serve: static app + /api vault routes + /ws + idle reaper
  pidfile.ts           # start/reuse the detached server; health check
  vault.ts             # the vault: tree, read/write/create/delete/rename, recents
web/
  index.html, app.js   # the SPA: vault picker, file tree, editor + live preview
  style.css
```

## Roadmap

- Swap the source+preview editor for a CodeMirror 6 **inline live-preview**
  editor (Obsidian-style) — adds a `bun build` step and a bundled `web/dist`.
- Port negotiation (if 4319 is taken), search, backlinks, and a Linux/Windows
  folder picker (currently macOS `osascript`).
