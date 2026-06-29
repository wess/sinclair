// Docker panel — an IPC plugin for Prompt. Lists running containers with a
// status badge, and offers a stats shortcut in the terminal.

type Block =
  | { type: "section"; title: string }
  | { type: "text"; text: string; dimmed?: boolean }
  | { type: "divider" }
  | { type: "badge"; label: string; color?: string }
  | { type: "button"; id: string; label: string; variant?: string }
  | { type: "row"; children: Block[] };

interface Response {
  title?: string;
  blocks: Block[];
  run?: { text: string; target?: string }[];
}

const req = JSON.parse((await Bun.stdin.text()) || "{}");

function docker(...args: string[]): { ok: boolean; out: string } {
  const p = Bun.spawnSync(["docker", ...args], { stderr: "pipe" });
  return { ok: p.exitCode === 0, out: new TextDecoder().decode(p.stdout).trim() };
}

function render(): Response {
  const have = Bun.spawnSync(["docker", "version", "--format", "{{.Server.Version}}"], {
    stderr: "pipe",
  });
  if (have.exitCode !== 0) {
    return {
      title: "Docker",
      blocks: [{ type: "text", text: "Docker is not available or not running.", dimmed: true }],
    };
  }

  // Tab-separated so names with spaces are unambiguous.
  const ps = docker("ps", "-a", "--format", "{{.Names}}\t{{.Status}}\t{{.Image}}");
  const lines = ps.out ? ps.out.split("\n") : [];

  const blocks: Block[] = [{ type: "section", title: `Containers (${lines.length})` }];
  if (lines.length === 0) {
    blocks.push({ type: "text", text: "No containers.", dimmed: true });
  } else {
    for (const line of lines.slice(0, 30)) {
      const [name, status, image] = line.split("\t");
      const up = (status || "").toLowerCase().startsWith("up");
      blocks.push({
        type: "row",
        children: [
          { type: "badge", label: up ? "up" : "off", color: up ? "green" : "gray" },
          { type: "text", text: `${name}  ·  ${image ?? ""}` },
        ],
      });
    }
  }

  blocks.push({ type: "divider" });
  blocks.push({ type: "button", id: "refresh", label: "Refresh", variant: "subtle" });
  blocks.push({ type: "button", id: "stats", label: "Stats in terminal", variant: "outline" });
  blocks.push({ type: "button", id: "prune", label: "Prune stopped", variant: "filled" });

  return { title: "Docker", blocks };
}

function action(name: string): Response {
  switch (name) {
    case "stats": {
      const r = render();
      r.run = [{ text: "docker stats", target: "tab" }];
      return r;
    }
    case "prune":
      docker("container", "prune", "-f");
      return render();
    case "refresh":
    default:
      return render();
  }
}

const resp: Response = req.kind === "action" ? action(req.action) : render();
console.log(JSON.stringify(resp));
