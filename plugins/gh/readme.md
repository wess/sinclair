# gh

Quick GitHub pull request views using the [gh](https://cli.github.com) CLI, in a
split to the right.

- Requires: `gh` authenticated (`gh auth login`); run inside a GitHub repo.
- Install: copy this folder into `~/.config/prompt/plugins/`, or add
  `plugin = /path/to/plugins/gh` to `~/.config/prompt/config`.
- Commands: "List Pull Requests" (`gh pr list`) and "PR Status" (`gh pr status`).
- Bind a key with `keybind = cmd+ctrl+p=plugin_command:gh/prs`.
