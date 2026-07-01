# Dashboard

A minimal **web-view plugin** for Prompt: an HTML page that drives the terminal
and calls back into the plugin's runtime, over the `window.Prompt` bridge.

It demonstrates all three message paths:

- `Prompt.runCommand("…")` — a built-in capability; runs in the focused
  terminal (handled by the app, no runtime call).
- `Prompt.readScreen(20)` — a built-in capability; returns the visible screen.
- `Prompt.invoke("ping", {…})` — a custom method; forwarded to `plugin.ts`
  (the `[runtime]`), whose `result` resolves the page promise.

## Files

- `plugin.toml` — declares the `[webview]` (loads `index.html`) and a `[runtime]`.
- `index.html` — the UI; talks to the app via `window.Prompt`.
- `plugin.ts` — the runtime; answers `invoke()` methods that aren't built-ins.

## Try it

Point Prompt at this directory (add `plugin = /abs/path/to/plugins/dashboard`
to your config, or copy it into `~/.config/prompt/plugins/`), then open it from
the command palette ("Open Dashboard") or the right sidebar's ◱ icon.

Change `placement` in `plugin.toml` to `window` or `tab` to host the same page
in a standalone window instead of the sidebar panel.
