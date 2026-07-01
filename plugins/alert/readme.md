# Alert

A **trigger-only** plugin: no command, no panel, no web view — it just reacts to
terminal events. This one posts a desktop notification whenever a command exits
non-zero.

```toml
[[trigger]]
on = "command_finished"   # bell | title_changed | notify | exit | command_finished | dir_changed
when = "nonzero"          # exit filter: any | zero/success | nonzero/failure
notify = "A command exited with a non-zero status"
```

## Events

| Event | Fires when | `when` filter |
| ----- | ---------- | ------------- |
| `bell` | a BEL is received | — |
| `title_changed` | the title changes | substring of the title |
| `notify` | the app is asked to notify (OSC 9/777/99) | substring of the body |
| `exit` | the pane's process exits | exit-code class |
| `command_finished` | a command finishes (OSC 133 `D`) | exit-code class |
| `dir_changed` | the working directory changes (OSC 7) | substring of the path |

`command_finished` and `dir_changed` need **shell integration** enabled.

## Actions

Exactly one per trigger:

- `notify = "…"` — desktop notification.
- `run = "…"` with optional `target` — `background` (default), `pane`, `tab`,
  `split_right`, or `split_down`. Runs with the focused pane's working directory.
- `invoke = "method"` — call the plugin's `[runtime]` with the event payload;
  the runtime may return `run` directives to execute.

## Ideas

- **autenv** — `on = "dir_changed"`, `run = "direnv reload"`, `target = "background"`.
- **ci watcher / error lens** — `on = "command_finished"`, `invoke = "onFinish"`,
  and let the runtime inspect and react.
