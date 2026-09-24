#!/usr/bin/env bash
# Build the VM runtime behind built-in OS Tabs from upstream source: libkrun
# (the hypervisor glue) and libkrunfw (the guest kernel), at pinned tags, with
# the fixes in scripts/krun/*.patch applied (each is meant for upstream). The
# libraries land in target/krun/lib, and are linked beside the dev binaries
# (target/{debug,release}/krun) where the app looks for them.
#
#   scripts/krun.sh          build (cached) and link beside dev builds
#   scripts/krun.sh sign     also ad-hoc sign dev binaries with the hypervisor
#                            entitlement, which a VM needs to start
#
# Needs Rust, and lld for libkrun's guest init (`brew install lld`, or llvm).
# macOS arm64 only for now.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

libkrun_tag="v1.19.4"
libkrunfw_tag="v5.6.1"

work="target/krun"
out="$work/lib"
mkdir -p "$work" "$out"

[ "$(uname -s)" = "Darwin" ] && [ "$(uname -m)" = "arm64" ] || {
  echo "error: built-in OS Tabs are macOS arm64 only for now" >&2
  exit 1
}

# lld links libkrun's guest init. Expose only the linker, never Homebrew's
# clang, which would shadow Apple's for bindgen.
if ! command -v ld.lld >/dev/null; then
  for d in /opt/homebrew/opt/lld/bin /opt/homebrew/opt/llvm/bin /opt/homebrew/opt/llvm@*/bin; do
    if [ -x "$d/ld.lld" ]; then
      mkdir -p "$work/bin"
      ln -sf "$d/ld.lld" "$work/bin/ld.lld"
      export PATH="$root/$work/bin:$PATH"
      break
    fi
  done
fi
command -v ld.lld >/dev/null || { echo "error: lld not found (brew install lld)" >&2; exit 1; }

# bindgen's libclang. Left to itself clang-sys may link Xcode's, whose
# @rpath install name the build script then can't resolve at run time;
# Homebrew llvm's (installed alongside lld) carries an absolute one.
if [ -z "${LIBCLANG_PATH:-}" ]; then
  for d in /opt/homebrew/opt/llvm/lib /opt/homebrew/opt/llvm@*/lib; do
    if [ -f "$d/libclang.dylib" ]; then
      export LIBCLANG_PATH="$d"
      break
    fi
  done
fi

fetch() { # repo tag dir
  if [ ! -d "$3/.git" ] || [ "$(git -C "$3" describe --tags 2>/dev/null)" != "$2" ]; then
    rm -rf "$3"
    git clone -q --depth 1 --branch "$2" "https://github.com/containers/$1" "$3"
  fi
}

# The build is keyed by tag plus patches, so editing a patch rebuilds.
libkrun_key="$libkrun_tag $(cat scripts/krun/*.patch | shasum -a 256 | cut -c1-16)"
if [ ! -f "$out/libkrun.1.dylib" ] || [ "$(cat "$out/.libkrun" 2>/dev/null)" != "$libkrun_key" ]; then
  echo "[krun] libkrun $libkrun_tag"
  fetch libkrun "$libkrun_tag" "$work/libkrun"
  git -C "$work/libkrun" checkout -q -- .
  for patch in scripts/krun/*.patch; do
    git -C "$work/libkrun" apply "$root/$patch"
  done
  SDKROOT="$(xcrun --show-sdk-path)" make -C "$work/libkrun"
  cp "$work/libkrun/target/release/libkrun.dylib" "$out/libkrun.1.dylib"
  echo "$libkrun_key" > "$out/.libkrun"
fi

if [ ! -f "$out/libkrunfw.5.dylib" ] || [ "$(cat "$out/.libkrunfw" 2>/dev/null)" != "$libkrunfw_tag" ]; then
  echo "[krun] libkrunfw $libkrunfw_tag"
  # The prebuilt bundle is the kernel as C source; compiling it is all a macOS
  # host needs (building the kernel itself takes a Linux toolchain).
  rm -rf "$work/libkrunfw" && mkdir -p "$work/libkrunfw"
  gh release download "$libkrunfw_tag" -R containers/libkrunfw \
    -p libkrunfw-prebuilt-aarch64.tgz -D "$work/libkrunfw" --clobber 2>/dev/null ||
    curl -fsSL -o "$work/libkrunfw/libkrunfw-prebuilt-aarch64.tgz" \
      "https://github.com/containers/libkrunfw/releases/download/$libkrunfw_tag/libkrunfw-prebuilt-aarch64.tgz"
  tar xzf "$work/libkrunfw/libkrunfw-prebuilt-aarch64.tgz" -C "$work/libkrunfw"
  make -C "$work/libkrunfw/libkrunfw"
  cp "$work/libkrunfw/libkrunfw/libkrunfw.5.dylib" "$out/libkrunfw.5.dylib"
  echo "$libkrunfw_tag" > "$out/.libkrunfw"
fi

for profile in debug release; do
  mkdir -p "target/$profile"
  ln -sfn "../krun/lib" "target/$profile/krun"
done
echo "[krun] -> $out"

if [ "${1:-}" = "sign" ]; then
  for bin in target/debug/sinclairdev target/release/sinclairdev; do
    [ -f "$bin" ] || continue
    codesign --force --entitlements assets/sinclair.entitlements -s - "$bin"
    echo "[krun] signed $bin"
  done
fi
