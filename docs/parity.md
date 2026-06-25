# Terminal feature coverage

A feature-by-feature map of Prompt's terminal support, as of phases 1–12.
Status key: **✓** implemented, **◑** partial (works for the common case, with
documented limits), **✗** not yet.

## Terminal emulation (VT)

| Area | Status | Notes |
|------|--------|-------|
| C0/C1 controls, ESC dispatch | ✓ | BEL/BS/HT/LF/VT/FF/CR/SO/SI, ESC 7/8/D/E/H/M/c/=/>, SCS, DECALN |
| CSI cursor/erase/scroll/insert | ✓ | CUU…CUP, ED/EL/ECH, IL/DL/ICH/DCH, SU/SD, REP, DECSTBM |
| SGR (colors + attributes) | ✓ | 16/256/truecolor (semicolon + colon forms), underline styles, all attrs |
| Modes (DEC private + ANSI) | ✓ | DECAWM/DECTCEM/DECOM/IRM, 47/1047/1048/1049, bracketed paste |
| Charsets (G0/G1, DEC special) | ✓ | line-drawing via SCS + SO/SI |
| Scrollback + alt screen | ✓ | ring buffer, content-anchored offset, no scrollback on alt |
| Wide characters | ✓ | width 2 + spacer cells |
| Combining characters | ◑ | one inline combining mark per cell (covers diacritics; stacked marks beyond the first are dropped) |
| Reflow on resize | ✓ | rejoins `wrapped` lines and re-wraps at the new width, cursor follows, overflow → scrollback |
| Damage tracking | ✓ | per-row + full-escalation (renderer does not yet clip to it) |

## Input

| Area | Status | Notes |
|------|--------|-------|
| Legacy xterm key encoding | ✓ | modifiers, cursor/tilde/function keys, app cursor/keypad |
| Mouse reporting | ✓ | X10/normal/button/any + SGR (1000/1002/1003/1006), alt-scroll |
| Bracketed paste | ✓ | |
| Kitty keyboard protocol | ◑ | negotiation + disambiguation encoding; **press-only** (no release/repeat events from the host, so event-type/alternate-key/associated-text flags are tracked but not encoded) |

## OSC / clipboard / links

| Area | Status | Notes |
|------|--------|-------|
| Title (OSC 0/2) + title stack | ✓ | |
| Palette OSC 4 / 104, cursor OSC 12 / 112 | ✓ | |
| Dynamic color *queries* (OSC 4/10/11/12 `?`) | ✓ | answered from theme via `set_report_colors` |
| OSC 7 cwd reporting | ✓ | inherited by new splits, tabs, and windows; defaults to `$HOME` when unknown |
| OSC 52 clipboard | ✓ | base64 decode → system clipboard |
| Desktop notifications (OSC 9 / 777 / 99) | ✓ | native banner + per-tab attention indicator; cleared on focus |
| OSC 8 hyperlinks | ✓ | interned per-cell, underlined, cmd-click opens |
| URL detection (no OSC 8) | ✓ | cmd-click opens detected URLs |
| Focus reporting (?1004) | ✓ | |
| Synchronized output (?2026) | ✓ | frame-gated with a 150 ms stuck-sync timeout |
| XTGETTCAP, DA1/DA2, DSR | ✓ | |

## Shell integration

| Area | Status | Notes |
|------|--------|-------|
| Semantic prompts (OSC 133) | ✓ | A marks prompt rows (into scrollback) |
| Jump-to-prompt | ✓ | `jump_to_prompt:N`, default cmd+up/down |
| Auto-injected shell scripts | ✓ | `shell-integration` injects OSC 133/7 hooks via env (zsh ZDOTDIR, fish vendor_conf.d, bash PROMPT_COMMAND); no rc edits |
| sudo / title helpers | ✗ | part of the shell scripts above |

## Fonts & rendering

| Area | Status | Notes |
|------|--------|-------|
| Font family + size | ✓ | live-reloadable |
| Fallback chain | ✓ | repeated `font-family` |
| Emoji | ✓ | via fallback chain + system fallback |
| Ligatures | ✓ | run shaping + `calt` |
| Font features | ✓ | `+liga`/`-calt`/`ss01`/`cv01=2` |
| Box-drawing / blocks | ◑ | light lines/junctions, blocks, shades, eighths drawn custom; heavy/double/dashed/rounded fall back to font |
| Cursor styles (DECSCUSR) | ✓ | block/bar/underline, config default |
| Images (sixel) | ✓ | sixel decoded (RGB/HLS palette, RLE, raster attrs) and GPU-composited, anchored to the grid so it scrolls with text |
| Images (kitty graphics) | ✗ | APC `_G` is not delivered by the pinned vte 0.13 (no APC callback); needs a vte bump/fork |

## UI / workspace

| Area | Status | Notes |
|------|--------|-------|
| Tabs | ✓ | bar, activate, close, move, goto N |
| Splits | ✓ | binary tree, directional focus, divider drag |
| Selection (cell/word/line) | ✓ | copy, copy-on-select, bracketed paste |
| Scrollback view + indicator | ✓ | |
| Search in scrollback | ✓ | cmd+f overlay with editable query (caret, cursor keys), live highlight, n/N nav |
| Config (`key = value`) | ✓ | full option set, diagnostics |
| Live config reload | ✓ | theme/font/padding/cursor/keybinds |
| Settings panel (GUI) | ✓ | cmd+, modal: click controls (theme/font size+style/cursor/padding/scrollback/copy-on-select) plus editable text fields (font family, shell, foreground, background) via a built-in text-input widget; all written back to the config file |
| Text-input widget | ✓ | `textedit` model (insert/delete/cursor, unicode) + in-panel field with caret; also backs the search query |
| Keybindings (`trigger = action`) | ✓ | config-driven, defaults + user overrides + unbind |
| Command palette | ✓ | cmd+shift+p fuzzy launcher over the action catalog, shows keybinds |
| Broadcast input | ✓ | cmd+shift+b mirrors typed keys to every pane in the tab (floating indicator) |
| Session restore | ✓ | `session-restore` saves tabs/splits/cwds on quit, rebuilds on launch |
| Session recording | ✓ | record a pane to an asciinema v2 `.cast` (cmd+shift+r); plays with `asciinema play`, embeddable |
| Per-tab context | ✓ | each tab shows the focused pane's git branch + working directory |
| Agent attention | ✓ | OSC 9/777/99 (or `prompt notify`) post a native banner and light up the tab |
| Scripting API (MCP + socket) | ✓ | `prompt mcp` exposes run_command, send_input, read_screen, new_tab, split, list_tabs/panes, focus_tab, notify |
| Themes | ✓ | 22 builtin schemes + overrides |
| Native macOS menu bar | ✓ | Prompt/File/Edit/View/Workspace/Window menus (plus AI when enabled), items reuse config actions (shortcuts shown); includes an About panel (icon, version, release date) |
| Custom window titlebar | ✓ | transparent native bar; app-drawn strip with tabs folded in and drag-to-move. macOS keeps the traffic lights; Linux draws its own minimize/maximize/close + resize edges (client-side decorations) |
| macOS status-bar (tray) item | ✗ | NSStatusBar is not exposed by the UI layer; needs custom native code |

## Prioritized remaining gaps

1. **Kitty graphics protocol** — blocked on the pinned vte 0.13, whose `Perform`
   has no APC callback; needs a vte bump/fork to capture `ESC _ G … ST`. (Sixel
   already works.)
2. **Stacked combining marks** — only the first combining mark per cell is kept;
   full grapheme clusters / ZWJ emoji need spillover storage.
3. **Damage-clipped rendering** — shape only dirty rows for big-throughput wins.
4. **Kitty release/repeat events** — needs key-up delivery from the host layer.
5. **Heavy/double/dashed/rounded box-drawing** — extend `boxdraw` geometry.
6. **macOS status-bar (tray) item** — needs native NSStatusBar code.
