// System panel — an IPC plugin for Prompt. Read-only host stats (load, disk
// for the focused pane's directory) plus a one-click monitor in the terminal.

type Block =
  | { type: "section"; title: string }
  | { type: "text"; text: string; dimmed?: boolean }
  | { type: "divider" }
  | { type: "kv"; key: string; value: string }
  | { type: "button"; id: string; label: string; variant?: string };

interface Response {
  title?: string;
  blocks: Block[];
  run?: { text: string; target?: string }[];
  // For a `tool` request (an MCP agent call): the value that resolves the call.
  result?: unknown;
}

const req = JSON.parse((await Bun.stdin.text()) || "{}");
const cwd: string = req.cwd || process.cwd();

function sh(cmd: string[], at?: string): string {
  const p = Bun.spawnSync(cmd, { cwd: at, stderr: "pipe" });
  return new TextDecoder().decode(p.stdout).trim();
}

function render(): Response {
  const blocks: Block[] = [{ type: "section", title: "Host" }];

  const host = sh(["hostname"]);
  if (host) blocks.push({ type: "kv", key: "host", value: host });

  // `uptime` includes load averages on macOS and Linux.
  const uptime = sh(["uptime"]);
  if (uptime) {
    const load = uptime.match(/load aver\w+s?:\s*(.+)$/i);
    if (load) blocks.push({ type: "kv", key: "load", value: load[1].trim() });
  }

  blocks.push({ type: "divider" });
  blocks.push({ type: "section", title: "Disk (cwd)" });
  // `df -h .` reports the filesystem backing the focused directory.
  const df = sh(["df", "-h", "."], cwd).split("\n");
  if (df.length >= 2) {
    const cols = df[1].split(/\s+/);
    // size, used, avail, capacity are stable across BSD/GNU df layouts.
    blocks.push({ type: "kv", key: "size", value: cols[1] ?? "?" });
    blocks.push({ type: "kv", key: "used", value: cols[2] ?? "?" });
    blocks.push({ type: "kv", key: "avail", value: cols[3] ?? "?" });
  }

  blocks.push({ type: "divider" });
  blocks.push({ type: "button", id: "refresh", label: "Refresh", variant: "subtle" });
  blocks.push({ type: "button", id: "monitor", label: "Open monitor", variant: "outline" });

  return { title: "System", blocks };
}

function action(name: string): Response {
  if (name === "monitor") {
    const r = render();
    // Prefer btop, fall back to top, in a split below.
    r.run = [{ text: "command -v btop >/dev/null && btop || top", target: "split_down" }];
    return r;
  }
  return render();
}

// The `stats` tool: the same host data as the panel, but as structured JSON an
// agent can consume directly.
function stats(at: string): Response {
  const host = sh(["hostname"]);
  const load = sh(["uptime"]).match(/load aver\w+s?:\s*(.+)$/i)?.[1]?.trim() ?? null;
  const df = sh(["df", "-h", "."], at).split("\n");
  const cols = df.length >= 2 ? df[1].split(/\s+/) : [];
  return {
    blocks: [],
    result: {
      host: host || null,
      load,
      disk: { size: cols[1] ?? null, used: cols[2] ?? null, avail: cols[3] ?? null },
    },
  };
}

let resp: Response;
if (req.kind === "tool") {
  resp = stats((req.params && req.params.cwd) || cwd);
} else if (req.kind === "action") {
  resp = action(req.action);
} else {
  resp = render();
}
console.log(JSON.stringify(resp));
