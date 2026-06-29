# serve

Serves the current directory over HTTP using Python's built-in server, in a
split to the right so you can keep working on the left.

- Requires: `python3` on your `PATH`.
- Install: copy this folder into `~/.config/prompt/plugins/`, or add
  `plugin = /path/to/plugins/serve` to `~/.config/prompt/config`.
- Commands: "Serve on :8000" (`cmd+ctrl+8`) and "Serve on :3000".
- Rebind with `keybind = cmd+ctrl+8=plugin_command:serve/8000`.
