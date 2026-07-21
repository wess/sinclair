//! Self-update: check GitHub for a newer release, install it **in place**, and
//! say how to relaunch. Modeled on Zed's `auto_update` crate; gpui-free so the
//! mechanics are testable — the app's `updateui` drives it and owns the UI.
//!
//! The load-bearing design decision is that an install is never *replaced*,
//! only *rewritten in place*:
//!
//! - **macOS**: the release `.dmg` is mounted and the new bundle's contents are
//!   `rsync --delete`d onto the installed `.app`. The bundle directory itself
//!   (its path *and* inode) never changes, so LaunchServices' registration
//!   stays valid and the relaunch is [`Relaunch::Current`] — gpui's restart
//!   reopens the running bundle via `NSBundle`. The previous rename-swap
//!   (`.app` → `.app.old`, staged → `.app`, delete `.app.old`) handed `open` a
//!   brand-new directory inode while deleting the running executable; when
//!   LaunchServices resolved the stale registration it fell back to running the
//!   inner Mach-O as a plain executable — inside Terminal.app, unbundled and
//!   broken. rsync-in-place removes every ingredient of that failure.
//! - **Linux AppImage**: the new image is downloaded *next to* the running one
//!   (a rename across filesystems fails, and `/tmp` is often tmpfs) and renamed
//!   over it; the relaunch is [`Relaunch::Binary`] pointing at the image.
//! - Anything else ([`Install::Unknown`]: a root-owned distro package, a dev
//!   build, Windows) can't be rewritten in place — the UI opens the release
//!   page instead.
//!
//! A release is only ever offered once it has published the asset this machine
//! would install — see [`Release::ready_for`]. GitHub publishes the release
//! before CI uploads to it, so a newer tag can be visible for the length of a
//! notarization run with nothing on it we can use; prompting in that window
//! yields an Update button that can only fail.
//!
//! [`migrate_bundle`] is the one exception to "never touch the bundle
//! directory", and it runs at startup rather than during an install: a
//! `Prompt.app` that has already updated its *contents* to Sinclair renames
//! itself once and relaunches at the new path. See [`migrate`].

mod appimage;
mod fetch;
mod install;
mod mac;
mod migrate;
mod release;
mod semver;

pub use install::{detect, install, Install, Relaunch, Stage};
pub use migrate::migrate_bundle;
pub use release::{check, Asset, Check, Release};
pub use semver::is_newer;

/// How often to re-check while running (a conservative hourly cadence).
pub const POLL: std::time::Duration = std::time::Duration::from_secs(60 * 60);
