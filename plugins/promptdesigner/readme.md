# promptdesigner (IPC panel)

Design your shell prompt visually, then apply it to your shell — no hand-editing
`PS1`/`PROMPT` escape codes.

- Requires: `bun`. Works with `zsh` and `bash`.
- Install: copy this folder into `~/.config/sinclair/plugins/promptdesigner` and
  open the panel from the activity bar (the ✎ icon).
- Pick a preset, toggle segments (user@host, cwd, git branch, time), choose a
  symbol and color, watch the live preview, then **Apply**.

How it applies: the design is written to `~/.config/prompt-designer/prompt.sh`,
and a single managed block in your `~/.zshrc` / `~/.bashrc` sources it (added
once, idempotent). Apply also `source`s it in the focused pane so you see the
change immediately. **Remove** strips the managed block. Your settings persist
in `~/.config/prompt-designer/design.json`.
