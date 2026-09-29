#!/usr/bin/env bash
# Smoke-test dist/linux packages inside a throwaway Ubuntu 22.04 VM (Lima).
#
#   scripts/vmtest.sh up         create/start the VM (first run provisions it)
#   scripts/vmtest.sh appimage   run the AppImage under headless sway, screenshot to dist/linux/vm-appimage.png
#   scripts/vmtest.sh deb        install the .deb, launch it under headless sway, screenshot
#   scripts/vmtest.sh shell      open a shell in the VM
#   scripts/vmtest.sh down       stop the VM; `destroy` deletes it
#
# Build packages first with scripts/linux.sh (Linux only) or download them from
# a release into dist/linux. The VM is aarch64, so it tests the aarch64 images.
set -euo pipefail

vm=sinclair-test
root="$(cd "$(dirname "$0")/.." && pwd)"
out="$root/dist/linux"
arch=aarch64

lima() { limactl shell "$vm" -- "$@"; }

need() { limactl list -q 2>/dev/null | grep -qx "$vm" || { echo "run: scripts/vmtest.sh up" >&2; exit 1; }; }

# launch $1 inside a headless sway (Wayland) for a few seconds, grim-screenshot to $2,
# fail if the app died early. Xvfb/Xvnc were tried first: the app's Vulkan surface
# renders black on both, so the Wayland path is the one that shows real output.
shot() {
  local cmd="$1" png="$2"
  lima bash -c "
    export XDG_RUNTIME_DIR=/tmp/xdg; rm -rf \$XDG_RUNTIME_DIR /tmp/shot.png
    mkdir -p \$XDG_RUNTIME_DIR; chmod 700 \$XDG_RUNTIME_DIR
    WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 WLR_RENDERER=pixman sway >/tmp/sway.log 2>&1 &
    sleep 4
    export WAYLAND_DISPLAY=\$(ls \$XDG_RUNTIME_DIR | grep -m1 '^wayland-[0-9]*\$'); unset DISPLAY
    $cmd >/tmp/app.log 2>&1 &
    pid=\$!
    sleep 12
    if ! kill -0 \$pid 2>/dev/null; then echo 'app exited early:'; tail -30 /tmp/app.log; pkill -x sway; exit 1; fi
    grim /tmp/shot.png
    kill \$pid; pkill -x sway; true
  "
  lima cat /tmp/shot.png >"$png"
  echo "[vmtest] screenshot -> $png"
}

case "${1:-}" in
  up)
    if limactl list -q 2>/dev/null | grep -qx "$vm"; then limactl start "$vm"
    else limactl start --name "$vm" --tty=false "$root/scripts/vmtest.yaml"; fi ;;
  appimage)
    need
    img=$(ls "$out"/Sinclair-*-$arch.AppImage | tail -1)
    lima bash -c "cp '$img' ~/s.AppImage && chmod +x ~/s.AppImage"
    shot "APPIMAGE_EXTRACT_AND_RUN=1 ~/s.AppImage" "$out/vm-appimage.png" ;;
  deb)
    need
    deb=$(ls "$out"/sinclair_*_arm64.deb | tail -1)
    lima bash -c "cp '$deb' /tmp/s.deb && sudo apt-get install -y /tmp/s.deb"
    shot "sinclair" "$out/vm-deb.png" ;;
  shell) need; limactl shell "$vm" ;;
  down) limactl stop "$vm" ;;
  destroy) limactl delete -f "$vm" ;;
  *) sed -n 2,12p "$0"; exit 1 ;;
esac
