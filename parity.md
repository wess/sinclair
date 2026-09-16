# Terminal feature coverage

A feature-by-feature map of Sinclair's terminal support, as of phases 1–12.
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
| Scrollback + alt screen | ✓ | two-tier ring: hot tail + lz4-compressed 512-row blocks, compacted incrementally when the pane is idle (~60x smaller history; default limit 100k rows); content-anchored offset, no scrollback on alt |
| Wide characters | ✓ | width 2 + spacer cells |
| Combining characters | ◑ | one inline combining mark per cell (covers diacritics; stacked marks beyond the first are dropped) |
| Reflow on resize | ✓ | rejoins `wrapped` lines and re-wraps at the new width, cursor follows, overflow → scrollback |
| Damage tracking | ✓ | per-row + full-escalation (renderer does not yet clip to it) |

## Input

| Area | Status | Notes |
|------|--------|-------|
| Legacy key encoding | ✓ | modifiers, cursor/tilde/function keys, app cursor/keypad |
| Mouse reporting | ✓ | X10/normal/button/any + SGR (1000/1002/1003/1006), alt-scroll |
| Bracketed paste | ✓ | |
| Kitty keyboard protocol | ✓ | all five enhancement flags encoded, over the full functional-key table: negotiation (`CSI ? / > / < / = u`, 16-deep per-screen stack), disambiguation, press/repeat/release event types, alternate (shifted) keys, associated text, and all-keys-as-escape-codes — including the bare modifier keys, synthesised from gpui's modifier-state changes. Modifier parameter carries super, hyper, meta and the locks; keys with no legacy spelling (F13–F35, keypad, media, lock and menu keys) use their `57344+` codes. Reported modifiers are the ones the window layer delivers — gpui exposes shift/alt/ctrl/super and caps lock, not hyper, meta, or num lock; keypad keys arrive under their plain names on macOS, so they encode as those |

## OSC / clipboard / links

| Area | Status | Notes |
|------|--------|-------|
| Title (OSC 0/2) + title stack | ✓ | |
| Palette OSC 4 / 104, cursor OSC 12 / 112 | ✓ | |
| Dynamic color *queries* (OSC 4/10/11/12 `?`) | ✓ | answered from theme via `set_report_colors` |
| OSC 7 cwd reporting | ✓ | inherited by new splits, tabs, and windows; defaults to `$HOME` when unknown |
| OSC 52 clipboard | ✓ | base64 decode → system clipboard |
| Desktop notifications (OSC 9 / 777 / 99) | ✓ | native banner + per-tab attention indicator; cleared on focus |
| OSC 8 hyperlinks | ✓ | interned per-cell, underlined, open-modifier click opens |
| URL detection (no OSC 8) | ✓ | open-modifier click opens detected URLs |
| Path detection | ✓ | an open-modifier click opens a filesystem path with the desktop's default handler, and hovering underlines it — but only once the candidate resolves against the pane's cwd, so text that merely looks like a path stays inert. Right-click still reveals it in the file manager. The modifier is cmd on macOS, ctrl elsewhere |
| Focus reporting (?1004) | ✓ | |
| Synchronized output (?2026) | ✓ | frame-gated with a 150 ms stuck-sync timeout |
| XTGETTCAP, DA1/DA2, DSR | ✓ | DA1 advertises sixel (`?62;4;22c`) |
| XTSMGRAPHICS, XTWINOPS size reports | ✓ | color registers (256) + sixel geometry; `CSI 14/16/18 t` report pixel/cell/grid sizes |

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
| Images (sixel) | ✓ | sixel decoded (RGB/HLS palette, RLE, raster attrs) and GPU-composited, anchored to the grid so it scrolls with text; advertised via DA1 attribute 4 and XTSMGRAPHICS so clients auto-detect it |
| Images (kitty graphics) | ✓ | a byte-level APC pre-parser (`term/apc.rs`) captures `ESC _G … ST` — which vte still discards — and `term/gfx.rs` dispatches all eight actions: transmit, transmit-and-display, display, delete, query, frame, animate, compose. Every medium (`t=d/f/t/s`, with `O=`/`S=` windows), every format (RGB/RGBA/PNG, zlib, chunked), image ids *and* image numbers, the full delete-specifier table, source cropping, cell-box scaling, cell offsets, z-index (including below the cell background), placement ids, virtual placements with unicode placeholders, and animation — frames, gaps, composition modes, loops, playback state. Responses honour the quiet level and name the image the way the request did |

## UI / workspace

| Area | Status | Notes |
|------|--------|-------|
| Tabs | ✓ | bar, activate, close, move, goto N |
| Splits | ✓ | binary tree, directional focus, divider drag |
| Selection (cell/word/line) | ✓ | copy, copy-on-select, bracketed paste |
| Scrollback view + indicator | ✓ | |
| Search in scrollback | ✓ | cmd+f overlay with editable query (caret, cursor keys), live highlight, n/N nav |
| Hint mode (keyboard link-follow) | ✓ | `hints` (cmd+shift+u) labels visible URLs; type a label to open. `vt::semantic`/`visible_links` |
| Smart selection | ✓ | double-click selects the whole URL/email/path/git-hash (`vt::semantic`), else a word; `smart-select` |
| Command blocks (OSC 133) | ✓ | `copy_command_output` (cmd+shift+o) copies the last command's output between prompt marks |
| Secret redaction | ✓ | `redact` regexes mask matches (•) on every copy path before the clipboard |
| Regex output triggers | ✓ | `trigger = regex \| title` fires a desktop notification when new output matches |
| Auto light/dark theme | ✓ | `theme-light`/`theme-dark` follow the OS appearance live (observe_window_appearance) |
| Minimum contrast | ✓ | `minimum-contrast` nudges fg toward black/white (WCAG ratio) until readable |
| Background opacity | ✓ | `background-opacity` makes the window translucent; default-bg cells show through |
| Middle-click paste / focus-follows-mouse | ✓ | `middle-click-paste`, `focus-follows-mouse` |
| Copy / vi mode | ✓ | `copy_mode` (cmd+shift+space): hjkl/arrows, w/b, 0/$, g/G, v select, y/Enter yank |
| Activity dashboard | ✓ | sidebar panel: every tab's working / attention / idle state at a glance |
| Clipboard history | ✓ | `clipboard_history` (cmd+shift+y): Spotlight over recent copies, paste on pick |
| Global search across tabs | ✓ | `search_all` (cmd+alt+f): Spotlight over recent output from every tab |
| Unicode / emoji picker | ✓ | `unicode_picker` (cmd+alt+e): fuzzy insert of emoji/symbols |
| Snippets / workflows | ✓ | `snippet = label \| cmd` + `snippets` (cmd+alt+s) insert picker |
| Launch profiles | ✓ | `profile = label \| cmd` + `profiles` (cmd+alt+p) opens a command tab |
| Pane badges | ✓ | `badge` watermark with {cwd}/{host} placeholders |
| Background image | ✓ | `background-image` behind the terminal (best with `background-opacity`) |
| Line timestamps | ✓ | `timestamps`: a faint relative time (5s/2m/1h/3d) beside each scrollback row (vt `committed_lines` + a capture-time ring) |
| Line annotations | ✓ | `annotate` (cmd+alt+a): pin a note to a line, keyed by stable line sequence; drawn as a left-gutter pill |

### Larger / blocked follow-ups (not yet done)

- **Persistent, detachable sessions** — a live
  mux server you detach/reattach; a multi-week subsystem. Session *restore* on
  quit exists (agent panes resume their native sessions); a live server does not.
- **SSH multiplexing domains, multiple cursors, serial** — out of current scope.
  Launch profiles cover opening an `ssh`/REPL/env tab; true remote multiplexing,
  the kitty multiple-cursor protocol, and serial connections do not exist yet.
| Config (`key = value`) | ✓ | full option set, diagnostics |
| Live config reload | ✓ | theme/font/padding/cursor/keybinds |
| Settings panel (GUI) | ✓ | cmd+, modal: click controls (theme/font size+style/cursor/padding/scrollback/copy-on-select) plus editable text fields (font family, shell, foreground, background) via a built-in text-input widget; all written back to the config file |
| Text-input widget | ✓ | `textedit` model (insert/delete/cursor, unicode) + in-panel field with caret; also backs the search query |
| Keybindings (`trigger = action`) | ✓ | config-driven, defaults + user overrides + unbind |
| Command palette | ✓ | cmd+shift+p fuzzy launcher over the action catalog, shows keybinds |
| Broadcast input | ✓ | cmd+shift+b mirrors typed keys to every pane in the tab (floating indicator) |
| Session restore | ✓ | `session-restore` saves tabs/splits/cwds on quit, rebuilds on launch; each pane comes back with its last `session-restore-lines` rows of scrollback (colors and all), agent panes resumed |
| Session recording | ✓ | record a pane to an asciinema v2 `.cast` (cmd+shift+r); plays with `asciinema play`, embeddable |
| Per-tab context | ✓ | each tab shows the focused pane's git branch + working directory |
| Agent attention | ✓ | OSC 9/777/99 (or `sinclair notify`) post a native banner and light up the tab |
| Scripting API (MCP + socket) | ✓ | `sinclair mcp` exposes run_command, send_input, read_screen, new_tab, split, list_tabs/panes, focus_tab, notify |
| Themes | ✓ | 22 builtin schemes + overrides |
| Native macOS menu bar | ✓ | Sinclair/File/Edit/View/Workspace/Window menus (plus AI when enabled), items reuse config actions (shortcuts shown); includes an About panel (icon, version, release date) |
| Custom window titlebar | ✓ | transparent native bar; app-drawn strip with drag-to-move. `unified-tab-bar` (on) folds the tabs into it; off gives an ordinary titlebar — controls, the active tab's title, the split buttons — with the tab bars beneath. The split controls sit at the right edge either way, and act on the focused pane when they are in the titlebar. macOS keeps the traffic lights; Linux draws its own minimize/maximize/close + resize edges (client-side decorations) |
| macOS status-bar (tray) item | ✗ | NSStatusBar is not exposed by the UI layer; needs custom native code |

## Prioritized remaining gaps

1. **Stacked combining marks** — only the first combining mark per cell is kept;
   full grapheme clusters / ZWJ emoji need spillover storage. (Kitty
   placeholder cells are unaffected: their marks are decoded to coordinates on
   write and packed into the same slot.)
2. **Heavy/double/dashed/rounded box-drawing** — extend `boxdraw` geometry.
3. **macOS status-bar (tray) item** — needs native NSStatusBar code.
