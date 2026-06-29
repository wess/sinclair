# sysmon

Opens an interactive system monitor in a split below the current pane. Two
commands let you pick your tool of choice.

- Requires: `htop` for the htop command, `btop` for the btop command.
- Install: copy this folder into `~/.config/prompt/plugins/`, or add
  `plugin = /path/to/plugins/sysmon` to `~/.config/prompt/config`.
- Keybinds: `cmd+ctrl+h` (htop) and `cmd+ctrl+b` (btop).
- Rebind with `keybind = cmd+ctrl+h=plugin_command:sysmon/htop`.
