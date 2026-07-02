# notes

The **Notes** vault server, shipped as a bundled sidecar binary beside the
`prompt` executable (like `relay`). It replaces the old Bun-based Notes plugin —
there is **no runtime dependency**; the web app is embedded in the binary.

## Modes

- `notes serve [PORT]` — run the HTTP + WebSocket server (default port 4319).
  Serves the embedded web app (`web/`) plus the vault API, watches the open
  vault for external changes and pushes them over `/ws`, and reaps itself after
  ~60 s with no connected client.
- `notes` / `notes boot` — launcher. Reads a plugin `boot` request on stdin,
  ensures the server is up (spawning `notes serve` detached if needed), and
  replies `{"result":{"port":PORT}}`. The app's `boot` webview invokes this and
  navigates to the reported address.

The app opens Notes (File → Notes) by handing the existing `boot` webview flow a
synthetic plugin whose runtime is this binary (`crates/app/src/notes.rs`).

## Vault API

`GET /health`, `GET /api/vault`, `POST /api/vault/{open,create,pick,forget}`,
`GET /api/vaults/recents`, `GET /api/tree`, `GET/PUT/POST/DELETE /api/file`,
`POST /api/file/{rename,move}`, `GET /api/resolve`, and `GET /ws` (change push).

## Rebuilding the editor bundle

The live-preview editor is CodeMirror 6, prebuilt to `web/dist/editor.js` and
committed so the crate has no build-time toolchain requirement. To regenerate it
after editing `web/src/editor.ts` (dev-only; needs Bun):

```sh
cd crates/notes/web && bun build src/editor.ts --outfile dist/editor.js --format esm
```
