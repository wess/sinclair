# docker (IPC panel)

A live Docker panel listing containers with an up/stopped badge, plus shortcuts.

- Requires: `bun` and `docker` (with a running daemon).
- Install: copy this folder into `~/.config/prompt/plugins/docker` and open the
  panel from the activity bar (the ❑ icon).
- Actions: **Refresh**, **Stats in terminal** (runs `docker stats` in a new
  tab), and **Prune stopped** (`docker container prune -f`).
