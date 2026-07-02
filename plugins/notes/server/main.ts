// The persistent Notes server: serves the web app and the vault API over HTTP,
// pushes external-change events over a WebSocket, and shuts itself down when
// idle (no connected client). Started detached by runtime/launcher.ts.

import { join, sep } from "node:path";
import { watch, type FSWatcher } from "node:fs";
import type { ServerWebSocket } from "bun";
import * as vault from "./vault.ts";
import { PORT } from "./pidfile.ts";

const WEB = join(import.meta.dir, "..", "web");
const port = Number(process.argv[2] || PORT);

const clients = new Set<ServerWebSocket<unknown>>();
let lastActive = Date.now();

function broadcast(msg: unknown): void {
  const s = JSON.stringify(msg);
  for (const ws of clients) {
    try {
      ws.send(s);
    } catch {}
  }
}

// Ignore watch events for files we just wrote ourselves (autosave echo).
const selfWrites = new Map<string, number>();
const markSelf = (rel: string) => selfWrites.set(rel, Date.now());
const isSelf = (rel: string) => {
  const t = selfWrites.get(rel);
  return t !== undefined && Date.now() - t < 1500;
};

let watcher: FSWatcher | null = null;
function startWatch(): void {
  watcher?.close();
  watcher = null;
  const cur = vault.current();
  if (!cur) return;
  try {
    watcher = watch(cur.root, { recursive: true }, (_e, filename) => {
      const rel = String(filename || "").split(sep).join("/");
      if (rel && isSelf(rel)) return;
      broadcast({ type: "changed", path: rel });
    });
  } catch {}
}

async function pickFolder(): Promise<string | null> {
  const proc = Bun.spawn([
    "osascript",
    "-e",
    'POSIX path of (choose folder with prompt "Choose a vault folder")',
  ]);
  const out = (await new Response(proc.stdout).text()).trim();
  await proc.exited;
  return out ? out.replace(/\/+$/, "") : null;
}

const json = (data: unknown, status = 200) => Response.json(data, { status });
const fail = (e: unknown, status = 400) => Response.json({ error: String(e) }, { status });

async function api(req: Request, url: URL): Promise<Response> {
  const p = url.pathname;
  const body = (): Promise<any> => req.json().catch(() => ({}));
  try {
    if (p === "/api/vault" && req.method === "GET") return json(vault.current());
    if (p === "/api/vault/open") {
      const { path } = await body();
      const v = vault.open(path);
      startWatch();
      return json(v);
    }
    if (p === "/api/vault/create") {
      const { path } = await body();
      const v = vault.create(path);
      startWatch();
      return json(v);
    }
    if (p === "/api/vault/pick") {
      const { mode } = await body();
      const dir = await pickFolder();
      if (!dir) return json(vault.current());
      const v = mode === "create" ? vault.create(dir) : vault.open(dir);
      startWatch();
      return json(v);
    }
    if (p === "/api/vaults/recents") return json(vault.recents());
    if (p === "/api/vault/forget") {
      const { path } = await body();
      vault.forgetRecent(path);
      return json(vault.recents());
    }
    if (p === "/api/tree") return json(vault.tree());
    if (p === "/api/file" && req.method === "GET") {
      return json({ content: vault.read(url.searchParams.get("path") || "") });
    }
    if (p === "/api/file" && req.method === "PUT") {
      const { path, content } = await body();
      markSelf(path);
      vault.write(path, content);
      return json({ ok: true });
    }
    if (p === "/api/file" && req.method === "POST") {
      const { parent, kind } = await body();
      const path = vault.createFile(parent || "", kind === "dir" ? "dir" : "file");
      broadcast({ type: "changed", path });
      return json({ path });
    }
    if (p === "/api/file" && req.method === "DELETE") {
      const { path } = await body();
      vault.remove(path);
      broadcast({ type: "changed", path });
      return json({ ok: true });
    }
    if (p === "/api/file/rename") {
      const { path, title } = await body();
      const dest = vault.rename(path, title);
      broadcast({ type: "changed", path: dest });
      return json({ path: dest });
    }
    if (p === "/api/file/move") {
      const { from, to } = await body();
      const dest = vault.move(from, to);
      broadcast({ type: "changed", path: dest });
      return json({ path: dest });
    }
    if (p === "/api/resolve") {
      return json({ path: vault.resolve(url.searchParams.get("title") || "") });
    }
    return json({ error: "not found" }, 404);
  } catch (e) {
    return fail(e);
  }
}

async function serveStatic(pathname: string): Promise<Response> {
  const name = pathname === "/" ? "index.html" : pathname.replace(/^\/+/, "");
  const file = Bun.file(join(WEB, name));
  if (await file.exists()) return new Response(file);
  return new Response("not found", { status: 404 });
}

startWatch();

Bun.serve({
  port,
  idleTimeout: 30,
  async fetch(req, server) {
    lastActive = Date.now();
    const url = new URL(req.url);
    if (url.pathname === "/health") return json({ server: "prompt-notes", port });
    if (url.pathname === "/ws") {
      if (server.upgrade(req)) return undefined as unknown as Response;
      return new Response("expected websocket", { status: 426 });
    }
    if (url.pathname.startsWith("/api/")) return api(req, url);
    return serveStatic(url.pathname);
  },
  websocket: {
    open(ws) {
      clients.add(ws);
      lastActive = Date.now();
    },
    close(ws) {
      clients.delete(ws);
      lastActive = Date.now();
    },
    message() {
      lastActive = Date.now();
    },
  },
});

// Idle reaper: exit when no client has been connected for a while, so a closed
// Notes tab doesn't leave a server running forever.
setInterval(() => {
  if (clients.size === 0 && Date.now() - lastActive > 60_000) process.exit(0);
}, 5_000);
