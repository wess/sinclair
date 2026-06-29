# ports

Lists the processes currently listening on local ports, printed into the focused
shell.

- Requires: `lsof` (preinstalled on macOS and most Linux distributions).
- Install: copy this folder into `~/.config/prompt/plugins/`, or add
  `plugin = /path/to/plugins/ports` to `~/.config/prompt/config`.
- Run it from the command palette ("List Listening Ports"), or bind a key with
  `keybind = cmd+ctrl+o=plugin_command:ports/list`.
