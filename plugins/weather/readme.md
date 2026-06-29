# weather

Prints the local weather forecast into the focused shell, courtesy of
[wttr.in](https://wttr.in).

- Requires: `curl`. Makes a network request to `wttr.in` when you run it.
- Install: copy this folder into `~/.config/prompt/plugins/`, or add
  `plugin = /path/to/plugins/weather` to `~/.config/prompt/config`.
- Run it from the command palette ("Weather"), or bind a key with
  `keybind = cmd+ctrl+w=plugin_command:weather/now`.
