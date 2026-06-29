# k9s

Opens [k9s](https://k9scli.io), the terminal UI for managing Kubernetes
clusters, in a new tab.

- Requires: `k9s` on your `PATH` and a working kubeconfig.
- Install: copy this folder into `~/.config/prompt/plugins/`, or add
  `plugin = /path/to/plugins/k9s` to `~/.config/prompt/config`.
- Keybind: `cmd+ctrl+k` (rebind with
  `keybind = cmd+ctrl+k=plugin_command:k9s/open`).
