// Ensure the persistent Notes server is running, and return its port. Reuses a
// live server recorded in the pidfile; otherwise spawns a fresh detached one.

import { join } from "node:path";
import { homedir } from "node:os";
import { mkdirSync, existsSync, readFileSync, writeFileSync } from "node:fs";

export const PORT = 4319;
const CONFIG_DIR = join(homedir(), ".config", "prompt", "notes");
const PIDFILE = join(CONFIG_DIR, "server.json");
const SERVER = join(import.meta.dir, "main.ts");

export async function health(port: number): Promise<boolean> {
  try {
    const r = await fetch(`http://127.0.0.1:${port}/health`, {
      signal: AbortSignal.timeout(500),
    });
    if (!r.ok) return false;
    const j = (await r.json()) as { server?: string };
    return j?.server === "prompt-notes";
  } catch {
    return false;
  }
}

export async function ensureServer(): Promise<number> {
  mkdirSync(CONFIG_DIR, { recursive: true });

  // Reuse a running server if the recorded port answers as ours.
  if (existsSync(PIDFILE)) {
    try {
      const { port } = JSON.parse(readFileSync(PIDFILE, "utf8"));
      if (typeof port === "number" && (await health(port))) return port;
    } catch {}
  }

  // Otherwise start a fresh detached server on the preferred port.
  const child = Bun.spawn(["bun", "run", SERVER, String(PORT)], {
    cwd: join(import.meta.dir, ".."),
    stdio: ["ignore", "ignore", "ignore"],
  });
  child.unref();
  writeFileSync(PIDFILE, JSON.stringify({ pid: child.pid, port: PORT }));

  for (let i = 0; i < 60; i++) {
    if (await health(PORT)) return PORT;
    await Bun.sleep(50);
  }
  throw new Error("Notes server did not start");
}
