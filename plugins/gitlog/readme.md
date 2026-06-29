# gitlog

Prints a compact, decorated commit graph into the focused shell — no extra tools
to install.

- Requires: `git`; run inside a git repository.
- Install: copy this folder into `~/.config/prompt/plugins/`, or add
  `plugin = /path/to/plugins/gitlog` to `~/.config/prompt/config`.
- Keybind: `cmd+ctrl+l` (rebind with
  `keybind = cmd+ctrl+l=plugin_command:gitlog/graph`).
