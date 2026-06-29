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

const resp: Response = req.kind === "action" ? action(req.action) : render();
console.log(JSON.stringify(resp));
