# Plugin ideas

A living list of plugins worth building. Split into what the current declarative
command model can do today, and what the planned plugin API (the IPC service —
event hooks, host capabilities, and server-driven column UI) unlocks.

See the design sketch for the API itself; the capability tags below
(`ui.sidebar`, `terminal.read`, `events:*`, `notify`, `storage`, `net`, `fs`)
refer to that model.

## Buildable today (command model)

These need nothing new — a `plugin.toml` that runs a shell command in a pane,
tab, or split. Several already ship in this catalog.

- **lazygit / lazydocker / k9s / tig** — launch a TUI in a tab. *(shipped)*
- **serve / ports / sysmon / gitlog / repl / gh / weather** — one-shot helpers in
  a pane or split. *(shipped)*
- **scripts** — read `package.json` / `Cargo.toml` and expose each script as a
  command (generated manifest).
- **ssh hosts** — one command per saved host that opens a tab and connects.
- **tunnel** — `cloudflared` / `ngrok` for the current port in a split.
- **dotfiles** — open your config repo in a tab; re-run `stow`/sync.

## Unlocked by the plugin API (IPC service)

Once plugins can subscribe to events, call host capabilities, and paint into the
left/right columns, the interesting work begins. Grouped by the surface that
makes them possible.

### Live column panels (server-driven UI)

A persistent panel in a side drawer, redrawn from a JSON block tree, with buttons
that fire `panel_action` events back to the plugin.

- **git** — branch, ahead/behind, staged/unstaged files for the current repo;
  click to stage, fetch, log. *(shipped: [git/](./git/))*
- **containers** — live `docker`/`podman` list with stats/prune actions, instead
  of just launching a TUI. *(shipped: [docker/](./docker/))*
- **system** — host load and disk for the focused pane, monitor shortcut.
  *(shipped: [sysinfo/](./sysinfo/))*
- **kubernetes** — pods/contexts/namespaces with quick switch and log tail.
  *(ui.sidebar, events:interval)*
- **tests** — a pass/fail tree parsed from the last test run; click a failure to
  jump or re-run just that test. *(ui.sidebar, terminal.read, events:command_finished)*
- **http client** — a Postman-lite column: saved requests, fire, view response.
  *(ui.sidebar, net, storage)*
- **database** — schema tree plus a query box wired to a split. *(ui.sidebar, net)*
- **snippets** — searchable snippet library; insert into the focused pane.
  *(ui.sidebar, terminal.write, storage)*
- **clipboard history** — recent yanks, click to paste. *(ui.sidebar, terminal.write)*
- **bookmarks / jumper** — frecency-ranked directories; click to `cd` a pane.
  *(ui.sidebar, terminal.run, events:dir_changed, storage)*
- **todos / scratch** — per-project notes and checklist that persist.
  *(ui.sidebar, storage)*
- **env profiles** — pick a `.env` profile to inject into new panes.
  *(ui.sidebar, fs, terminal.run)*
- **log viewer** — tail a file with a regex filter box and level highlighting.
  *(ui.sidebar, fs)*

### Event-driven automation (hooks, no UI required)

Functions that run on a terminal event and act through host capabilities.

- **ci watcher** — on push, poll GitHub Actions; notify on failure, badge while
  running. *(events:command_finished, net, notify, status bar)*
- **error lens** — watch command output for stack traces; surface the parsed
  error (and a fix hint) in a column or notification. *(events:command_finished,
  terminal.read)*
- **autenv** — on `dir_changed`, load the right toolchain / node version / venv.
  *(events:dir_changed, terminal.run)*
- **command explainer** — as you type, show flags and a cheat sheet for the
  current command. *(events: input, ui.sidebar)*
- **timekeeper** — track time per project from focus/dir events; status-bar timer
  and a daily log. *(events:pane_focused, dir_changed, storage, status bar)*
- **guardrails** — warn (don't block) before risky commands in protected dirs.
  *(events: pre_command, notify)*

### Ambient / status surfaces

Small persistent indicators rather than full panels.

- **status badges** — git ahead/behind, CI state, k8s context, cloud profile,
  battery/clock, pinned to the title or status strip. *(status bar, events:*)*
- **pomodoro** — a focus timer with a status-bar countdown. *(status bar, events:interval)*
- **ai composer** — a column that turns a natural-language ask into a command
  (ties into the local `assist` crate). *(ui.sidebar, terminal.write)*

## Notes for contributors

- UI plugins describe their columns as block trees; they never link gpui.
- Every capability a plugin uses must be declared in its manifest and approved on
  install — keep the request set minimal.
- Prefer events that are opt-in and cheap; `command_finished` fires constantly,
  so filter early.
