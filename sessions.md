# Persistent sessions

With `persistent-sessions` on (Settings → General, or `"persistent-sessions": true`
in `settings.json`; macOS and Linux), panes run in a separate session server
instead of inside the app. A crash, a self-update, or quitting leaves every
shell — and whatever it is running — alive, and the next launch puts them back.

## What happens

- The first pane starts `sinclair _server` in the background: its own session
  and process group, output to `data/logs/sessions.log`. It exits by itself
  after 10 seconds with no sessions and no connections.
- Each pane is a session in the server. The app attaches to it over a per-user
  socket (`<runtime dir>/<app id>-sessions.sock`, owner-only, peer uid checked).
- **Quit** lets the sessions go; they keep running. **Closing a pane** ends its
  session, as it always did.
- **Launch** reattaches the panes the saved session names (`session-restore`),
  in their old layout, and opens every other session the server is holding —
  what a crash left behind — as a tab. The fresh shell the window opened with is
  closed when that happens.
- A reattached pane keeps the `SINCLAIR_PANE` its shell started with, so agent
  hooks keep reporting to the right pane; new panes are numbered above it.
- Quitting with `confirm-quit` on no longer warns about processes in server
  panes, since they don't end.

`sinclair sessions` lists what the server is running (`--json`, `--kill ID`).

## How it works

The server owns the pty and a `vt::Terminal` for each session (`terminal::mux`).
On attach it sends a snapshot — `vt::Terminal::snapshot`, the session's whole
state written back out as escape sequences — then streams raw child output. The
app feeds both into a terminal of its own, so rendering, search, selection, copy
mode and the rest work on that local copy unchanged. Keystrokes, resizes, the
theme colors used to answer OSC color queries, and kills go the other way.

Taking the snapshot and registering the client both happen under the session's
terminal lock, and output reaches clients from a tap that runs under the same
lock, so a client sees every byte once, after its snapshot. Only the server's
terminal answers the child (DSR, DA, XTGETTCAP); replies the app's copy
generates are dropped.

The wire format (`terminal::mux::frame`) is a one-byte kind, a big-endian `u32`
length, and the payload: raw bytes for output and input, small JSON objects
for control. A GUI updated in place attaches to a server the previous version
started, so the format only grows: new kinds or new JSON fields, never changes
to old ones. `PROTOCOL` bumps only for a break an older peer can't ignore.

## What a snapshot doesn't carry

Images (sixel/kitty), hyperlink targets, the DECSC saved cursor, the title
stack, character sets and custom tab stops. A reattached pane shows the text and
styling where images were. Opt-in gates set in the app (`vt-checksum-report`,
`vt-window-resize-allowed`) apply to the app's terminal, not the server's.

## Not done

- Windows (no ConPTY server or named-pipe transport yet).
- Remote clients and several viewers on one session. The protocol allows more
  than one client per session, but the app attaches one, and only over the
  local socket.
