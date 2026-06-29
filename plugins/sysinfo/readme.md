# sysinfo (IPC panel)

A small read-only system panel: hostname, load average, and the disk usage of
the focused pane's filesystem, with a one-click monitor.

- Requires: `bun` (and the standard `uptime`/`df`/`hostname` tools).
- Install: copy this folder into `~/.config/prompt/plugins/sysinfo` and open the
  panel from the activity bar (the ❖ icon).
- Actions: **Refresh**, and **Open monitor** (runs `btop`/`top` in a split via a
  run directive).
