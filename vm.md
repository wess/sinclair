# Built-in OS Tabs (the `vm` crate)

OS Tabs run a Linux image in a tab. With Docker or Podman installed they use
that engine. Without one, Sinclair runs the image itself in a microVM, so an
OS Tab works on a machine with nothing else installed.

Sinclair owns the whole path. No smolvm, no container CLI, no daemon. The only
outside code is libkrun (the hypervisor glue) and libkrunfw (the guest kernel),
built from upstream source and shipped inside the app.

## Choosing the runner

`container-engine` in settings:

| value | OS Tabs run on |
|---|---|
| `auto` (default) | Docker/Podman when installed, else the built-in VM |
| `docker` / `podman` | that engine |
| `builtin` | the built-in VM |

The attach picker and the project sandbox still need Docker or Podman; they
depend on container labels and `docker exec`.

## How a tab runs

```
tab pty ── sinclair _vm run [--persist] --name N IMAGE COMMAND
             ├─ pull: registry → blobs/ (sha256-verified), unpack → images/<digest>/rootfs
             ├─ clone: images/<digest>/rootfs → machines/N/rootfs (APFS clonefile)
             └─ sinclair _vm boot machines/N   ← libkrun takes this process over
```

`run` stays alive while `boot` runs so it can remove an ephemeral machine when
the VM exits. libkrun's `krun_start_enter` never returns, which is why the VM
gets its own process. Anything a killed `run` leaves behind is swept by the
next `run`.

State lives under `~/.config/sinclair/data/vm/` (`blobs/`, `images/`, `refs/`,
`machines/`). A persistent machine is named after its profile
(`sinclair-debian`), so the next tab reopens the same one. A machine runs in
one tab at a time.

## The guest

- **Rootfs:** the unpacked image, shared into the guest over virtiofs. Files
  are written by the user, so each entry records the guest's owner and mode in
  the `user.containers.override_stat` xattr, which libkrun reads back. apt's
  `_apt` sandbox and setuid binaries work because of this.
- **Network:** libkrun's TSI sends guest TCP/UDP through the host's own
  sockets, so there is no NIC or DHCP to set up and whatever the host can
  reach, the guest can reach. `/etc/resolv.conf` is written from the host's
  nameservers. ICMP (`ping`) does not work.
- **Terminal:** the guest console is the tab's pty. Job control works, and
  resizes are forwarded (see the patch below).
- **Command:** the profile's command, else the image's entrypoint + cmd, else
  `/bin/sh`. libkrun passes argv on the kernel command line, so arguments with
  embedded quotes do not survive. A plain shell is fine.

## Building the runtime

```sh
scripts/krun.sh        # build libkrun + libkrunfw (cached) into target/krun/lib
scripts/krun.sh sign   # + ad-hoc sign dev binaries with the hypervisor entitlement
```

It needs Rust and lld (`brew install lld`). A VM only starts from a binary
signed with `com.apple.security.hypervisor` (in `assets/sinclair.entitlements`).
`cargo build` replaces the binary unsigned, so re-run `scripts/krun.sh sign`
after each build. An unsigned binary fails with `krun_start_enter failed:
Invalid argument`.

`scripts/bundle.sh` runs `krun.sh` and copies both libraries into
`Sinclair.app/Contents/Frameworks`. The app looks for them in `../Frameworks`,
`../lib/sinclair`, then `krun/` beside the executable.

`scripts/krun/*.patch` are applied on top of the pinned libkrun tag. Each is
meant for upstream:

- `sigwinch.patch`: forward terminal resizes to the guest on macOS. Upstream
  only does this on Linux, because macOS's pipe-backed eventfd needs its write
  end.

## Limits

- macOS on Apple silicon only. Upstream libkrun supports Hypervisor.framework
  on arm64 only. Linux (KVM) also needs a different answer for file ownership,
  since the xattr override is macOS-only in libkrun.
- Public images only: registry auth is anonymous.
- gzip and uncompressed layers only (no zstd).
