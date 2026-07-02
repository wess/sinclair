// The Notes [runtime] — a one-shot launcher invoked over Prompt's serverless
// bridge. On the page's `boot` call it ensures the persistent server is running
// and returns its port. Everything real happens in ../server (a detached,
// long-lived Bun process); this just supervises it.
//
// Protocol: read one JSON request on stdin, write one JSON response on stdout.
//   request:  { kind: "message", method: "boot", ... }
//   response: { result: { port } }

import { ensureServer } from "../server/pidfile.ts";

const req = JSON.parse((await Bun.stdin.text()) || "{}");

if (req.kind === "message" && req.method === "boot") {
  try {
    const port = await ensureServer();
    console.log(JSON.stringify({ result: { port } }));
  } catch (e) {
    console.log(JSON.stringify({ result: { error: String(e) } }));
  }
} else {
  console.log(JSON.stringify({ result: null }));
}
