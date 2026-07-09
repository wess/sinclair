#!/usr/bin/env bash
# Screenshot the running Sinclair window to a PNG, for eyeballing UI during debug
# testing. macOS only (uses `screencapture`). Needs a one-time Screen Recording
# permission grant for the invoking terminal.
#
# Usage: scripts/shot.sh [out.png]
#   Captures the frontmost window owned by a `sinclair`/`app` process. If it can't
#   resolve the window id, falls back to an interactive window pick.
set -euo pipefail

out="${1:-/tmp/sinclair-shot.png}"

# Find a window id owned by the debug binary (`app`) or the bundled `sinclair`.
# `screencapture -l<id>` grabs exactly that window (no shadow with -o).
winid="$(osascript <<'AS' 2>/dev/null || true
tell application "System Events"
  set procs to (every process whose name is "app" or name is "sinclair" or name is "Sinclair")
  repeat with p in procs
    if (count of windows of p) > 0 then
      return id of window 1 of p
    end if
  end repeat
end tell
AS
)"

if [[ -n "${winid}" && "${winid}" =~ ^[0-9]+$ ]]; then
  screencapture -o -x -l"${winid}" "${out}"
else
  echo "shot.sh: couldn't resolve the Sinclair window id (grant Accessibility to your terminal, or pick manually)." >&2
  # Interactive window capture: click the window to grab it.
  screencapture -o -w "${out}"
fi

echo "${out}"
