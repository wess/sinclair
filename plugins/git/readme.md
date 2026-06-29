# git (IPC panel)

A live Git side panel built on Prompt's plugin API. Shows the current branch,
ahead/behind, and the working-tree changes for the directory of the focused
pane, with one-click actions.

- Requires: `bun` and `git` on your `PATH`.
- Install: copy this folder into `~/.config/prompt/plugins/git`, then open the
  panel from the right/left activity bar (the ⎇ icon).
- Actions: **Stage all** (`git add -A`), **Fetch**, **Refresh**, and **Log in
  terminal** (runs `git log --graph` in the focused pane via a run directive).

This is an IPC plugin: Prompt invokes `bun run plugin.ts` per render/action,
passing the focused pane's cwd, and renders the returned block tree.
