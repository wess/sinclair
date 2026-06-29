# tig

Opens [tig](https://jonas.github.io/tig/), the ncurses git history browser, in a
new tab.

- Requires: `tig` on your `PATH`; run inside a git repository.
- Install: copy this folder into `~/.config/prompt/plugins/`, or add
  `plugin = /path/to/plugins/tig` to `~/.config/prompt/config`.
- Run it from the command palette ("Open tig"), or bind a key with
  `keybind = cmd+ctrl+t=plugin_command:tig/open`.
