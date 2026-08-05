# Releasing Sinclair

Sinclair ships as a signed macOS `.dmg` (plus a Homebrew cask) and Linux
`.tar.gz`, `.deb`, and `.AppImage` packages for x86_64 and aarch64. Releases are
cut by GitHub Actions (`.github/workflows/release.yml`); the local scripts under
`scripts/` are the same steps you can run by hand.

## Cutting a release

1. Bump `version` in the workspace `Cargo.toml` (`[workspace.package]`).
2. Merge to `main`.

The workflow notices the new version (no matching `vX.Y.Z` tag yet), tags it,
creates a **draft** GitHub Release, then in parallel: builds and notarizes
`Sinclair.dmg`; and builds the Linux packages (matrix over x86_64 and aarch64
on native runners). Both upload to the draft. Once macOS and Linux have both
succeeded, the `publish` job flips the draft live, and only then do the
`sinclair` cask in
[`wess/homebrew-packages`](https://github.com/wess/homebrew-packages) and the
Scoop manifest update — both build their manifests by downloading from the
public release URL, which 404s on a draft. The version check is idempotent, so
re-running is safe.

The draft matters for more than tidiness. `releases/latest` only reports
*published* releases, and that endpoint is what the in-app updater polls.
Publishing up front advertised a version for the length of a notarization run
with none of its macOS assets attached — the update prompt would appear and its
Update button could only fail. The client refuses such a release now too (see
`Release::ready_for` in `crates/updater/src/release.rs`), but the draft is what
keeps the window from existing at all.

`publish` waits for the Windows job but doesn't require it: Windows is beta and
has no in-place update path, so a failure there won't hold back macOS or Linux.

### If a build fails and the release is stuck as a draft

`create-release` pushes the tag before the builds run, so once a version has
been attempted, `check-version` sees the tag and reports `changed=false` — a
re-push of the same version does nothing, and the draft sits there invisible.
Recover with either:

- **Re-run failed jobs** on the workflow run (keeps `check-version`'s outputs,
  so `publish` re-evaluates and flips the draft once the retry succeeds), or
- `gh release edit "vX.Y.Z" --draft=false` by hand, if the assets are actually
  all present and only `publish` failed.

Failing closed like this is deliberate: a stuck draft ships nothing, where the
old behaviour shipped a release users' updaters could see but not install.

Because the Linux build includes code that never compiles on the macOS dev host
(`linux.rs`, the `#[cfg(target_os = "linux")]` blocks), validate it **before**
cutting the release: open a PR, which runs the `Linux Build` workflow
(`.github/workflows/linux.yml`) on both architectures and uploads the artifacts
to the run for inspection.

## Local build

```sh
scripts/icon.sh      # regenerate assets/icon.{png,icns} (only if the icon changed)
scripts/bundle.sh    # cargo build --release + assemble dist/Sinclair.app
scripts/dmg.sh       # package dist/Sinclair.dmg
```

Without `CODESIGN_IDENTITY` set, `bundle.sh` ad-hoc-signs the app: it launches on
the build machine but is not distributable. For a signed local build, set
`CODESIGN_IDENTITY` to a Developer ID Application identity from your keychain.

## Linux build

```sh
scripts/linux.sh            # build + package for the host arch
scripts/linux.sh aarch64    # label the artifacts (still builds natively)
```

`scripts/linux.sh` builds the release binary and produces, in `dist/linux/`, a
`.tar.gz` (FHS tree), a `.deb` (via `cargo-deb`, configured in
`crates/app/Cargo.toml`'s `[package.metadata.deb]`), and an `.AppImage` (via
`linuxdeploy` + `appimagetool`, downloaded on demand). It builds **natively** —
CI uses a separate runner per architecture rather than cross-compiling.

Requirements: a Rust toolchain, `cargo-deb` (installed on demand), `curl`,
`file`, and the gpui system libraries — `clang`, `libasound2-dev`,
`libfontconfig-dev`, `libssl-dev`, `libvulkan1`, `libwayland-dev`,
`libx11-xcb-dev`, `libxkbcommon-x11-dev`. The `.desktop` entry is
`assets/sinclair.desktop`; the AppImage icon must be a standard size, so packaging
uses `assets/icon512.png` (the 1024px master is rejected by `linuxdeploy`).

## Signing & notarization (CI)

Signing is optional — without secrets the workflow produces an ad-hoc build and
warns. To sign + notarize, set these repository secrets:

| Secret | What it is |
|--------|------------|
| `APPLE_SIGNING_IDENTITY` | e.g. `Developer ID Application: Your Name (TEAMID)` |
| `APPLE_CERT_P12` | base64 of the exported Developer ID `.p12` |
| `APPLE_CERT_PASSWORD` | password for that `.p12` |
| `KEYCHAIN_PASSWORD` | any password for the throwaway CI keychain |
| `APPLE_ID` | Apple ID email for notarytool |
| `APPLE_TEAM_ID` | Apple Developer Team ID |
| `APPLE_APP_PASSWORD` | app-specific password for that Apple ID |
| `HOMEBREW_TAP_TOKEN` | token with write access to `wess/homebrew-packages` |

The app is signed with a hardened runtime and `assets/sinclair.entitlements`
(GPUI/Metal needs the JIT / unsigned-executable-memory entitlements), then the
`.app` and `.dmg` are notarized and stapled.

## Icon

`scripts/icon.swift` draws the icon (a terminal `>_` glyph on a dark indigo
squircle) with CoreGraphics — no third-party tooling. `scripts/icon.sh` renders
the 1024px master and compiles the `.icns`. The committed `assets/icon.png` and
`assets/icon.icns` are what the macOS bundle embeds; `assets/icon512.png` is the
512px downscale used for Linux packaging. Regenerate them only when the design
changes.
