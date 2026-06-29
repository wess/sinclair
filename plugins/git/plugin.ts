// Git panel — an IPC plugin for Prompt.
//
// Protocol: read one JSON request on stdin, write one JSON response on stdout.
//   request:  { kind: "render" | "action", panel, action?, cwd? }
//   response: { title?, blocks: Block[], run?: RunDirective[] }
//
// Blocks describe the panel UI; run directives are commands Prompt executes in
// the focused terminal. This plugin reads git state from `cwd` and offers
// one-click actions (stage, fetch, refresh) plus a "log in terminal" directive.

type Block =
  | { type: "section"; title: string }
  | { type: "text"; text: string; dimmed?: boolean }
  | { type: "divider" }
  | { type: "kv"; key: string; value: string }
  | { type: "badge"; label: string; color?: string }
  | { type: "button"; id: string; label: string; variant?: string }
  | { type: "row"; children: Block[] };

interface Response {
  title?: string;
  blocks: Block[];
  run?: { text: string; target?: string }[];
}

const req = JSON.parse((await Bun.stdin.text()) || "{}");
const cwd: string = req.cwd || process.cwd();

function git(...args: string[]): { ok: boolean; out: string } {
  const p = Bun.spawnSync(["git", "-C", cwd, ...args], { stderr: "pipe" });
  return { ok: p.exitCode === 0, out: new TextDecoder().decode(p.stdout).trim() };
}

function isRepo(): boolean {
  return git("rev-parse", "--is-inside-work-tree").ok;
}

// Map a porcelain status code to a short badge + color.
function badge(code: string): { label: string; color: string } {
  const c = code.trim();
  if (c === "??") return { label: "?", color: "gray" };
  if (c.includes("M")) return { label: "M", color: "yellow" };
  if (c.includes("A")) return { label: "A", color: "green" };
  if (c.includes("D")) return { label: "D", color: "red" };
  if (c.includes("R")) return { label: "R", color: "teal" };
  return { label: c || "•", color: "gray" };
}

function render(): Response {
  if (!isRepo()) {
    return { title: "Git", blocks: [{ type: "text", text: "Not a git repository.", dimmed: true }] };
  }

  const branch = git("rev-parse", "--abbrev-ref", "HEAD").out || "(detached)";
  const blocks: Block[] = [
    { type: "section", title: "Branch" },
    { type: "kv", key: "branch", value: branch },
  ];

  // Ahead/behind vs upstream, when one is configured.
  const ab = git("rev-list", "--left-right", "--count", "@{u}...HEAD");
  if (ab.ok && ab.out) {
    const [behind, ahead] = ab.out.split(/\s+/);
    blocks.push({ type: "kv", key: "ahead / behind", value: `${ahead} / ${behind}` });
  }

  // Read porcelain WITHOUT trimming — the leading column is significant
  // (a space means "unstaged"), and trimming would shift the first row's path.
  const statusRaw = new TextDecoder().decode(
    Bun.spawnSync(["git", "-C", cwd, "status", "--porcelain"], { stderr: "pipe" }).stdout,
  );
  const lines = statusRaw.split("\n").filter((l) => l.length > 0);
  blocks.push({ type: "divider" });
  blocks.push({ type: "section", title: `Changes (${lines.length})` });

  if (lines.length === 0) {
    blocks.push({ type: "text", text: "Working tree clean.", dimmed: true });
  } else {
    for (const line of lines.slice(0, 20)) {
      const code = line.slice(0, 2);
      const path = line.slice(3);
      const b = badge(code);
      blocks.push({
        type: "row",
        children: [
          { type: "badge", label: b.label, color: b.color },
          { type: "text", text: path },
        ],
      });
    }
    if (lines.length > 20) {
      blocks.push({ type: "text", text: `… and ${lines.length - 20} more`, dimmed: true });
    }
  }

  blocks.push({ type: "divider" });
  blocks.push({ type: "button", id: "stage_all", label: "Stage all", variant: "filled" });
  blocks.push({ type: "button", id: "fetch", label: "Fetch" });
  blocks.push({ type: "button", id: "refresh", label: "Refresh", variant: "subtle" });
  blocks.push({ type: "button", id: "log", label: "Log in terminal", variant: "outline" });

  return { title: `Git · ${branch}`, blocks };
}

function action(name: string): Response {
  switch (name) {
    case "stage_all":
      git("add", "-A");
      return render();
    case "fetch":
      git("fetch", "--all", "--prune");
      return render();
    case "log": {
      // Demonstrate a run directive: show the graph in the focused terminal.
      const r = render();
      r.run = [{ text: "git log --oneline --graph --decorate -20", target: "pane" }];
      return r;
    }
    case "refresh":
    default:
      return render();
  }
}

const resp: Response = req.kind === "action" ? action(req.action) : render();
console.log(JSON.stringify(resp));
