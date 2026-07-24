# gpui dependency recipe

GPUI comes from the zed repo as a git dependency. Verified building on
rustc 1.96.0, zed rev `96285fc` (2026-06-12, scratch build in /tmp/gpuiprefetch).

Two things are required:

1. Rust stable >= 1.96 (zed main uses recently-stabilized library features;
   1.90 fails on `slice::as_array`).
2. Cargo `[patch.crates-io]` entries mirrored from zed's own workspace —
   cargo patches do NOT propagate through git dependencies, so the consumer
   workspace must declare them itself:

```toml
[dependencies]
gpui = { git = "https://github.com/zed-industries/zed" }

[patch.crates-io]
async-process = { git = "https://github.com/zed-industries/async-process.git", rev = "0b6d6713570af61806e1e5cb40e0f757cb93fd9d" }
async-task = { git = "https://github.com/smol-rs/async-task.git", rev = "b4486cd71e4e94fbda54ce6302444de14f4d190e" }
```

Why: zed's fork of async-process adds `Child::adopt_raw_pid`, used by zed's
`util` crate on darwin; `async-task` rides along on a pinned smol-rs rev.
Zed's other patches (calloop, notify, livekit, windows-capture, ...) are not
needed for gpui on darwin.

If the resolved zed rev ever changes, re-check zed's root `Cargo.toml`
`[patch.crates-io]` section for updated fork revs.

## guise + the gpui patch

The UI component library `guise` (vendored at `vendor/guise`) tracks crates.io
`gpui 0.2.2`, while we build gpui from the zed git rev above. To make the whole
tree resolve a single gpui, the root `Cargo.toml` adds:

```toml
[patch.crates-io]
gpui = { git = "https://github.com/zed-industries/zed", rev = "96285fc1" }
```

This redirects guise's crates.io `gpui ^0.2.2` onto our exact rev. `vendor/guise`
is `exclude`d from our workspace (it is its own). When you bump the zed rev,
rebuild guise against it (its `prompt-gpui-port` branch) and re-pin the
submodule. See `docs/guise.md`.
