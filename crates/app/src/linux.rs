//! Linux display-server integration for the quick terminal.
//!
//! NOTE: this module is written against the gpui and x11rb APIs but is NOT
//! compiled on the macOS development host — cross-building the Linux GUI
//! dependencies needs a Linux toolchain plus wayland/xcb/xkbcommon dev
//! libraries. It must be validated with a real Linux build.
//!
//! Two display servers, two mechanisms:
//! - Wayland: open the window as a `wlr-layer-shell` overlay surface anchored
//!   as a full-width top band. That single window kind gives float-above-all,
//!   present-on-every-output, and no decorations for free.
//! - X11: a normal window decorated after creation with EWMH/Motif hints
//!   (`_NET_WM_STATE_ABOVE` + `_STICKY`, all desktops, borderless).
//!
//! Global summon is handled elsewhere: `global-hotkey` (XGrabKey) on X11, and
//! the `--toggle-quick` socket (driven by a compositor keybind) on Wayland,
//! which forbids in-process global hotkeys.

use std::error::Error;

use gpui::{Window, WindowKind};

/// Whether this is a Wayland session (otherwise assume X11).
pub fn is_wayland() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some()
}

/// Window kind for the quick terminal: a layer-shell overlay on Wayland, a
/// normal window on X11 (decorated by [`make_overlay`]).
pub fn window_kind() -> WindowKind {
    if is_wayland() {
        use gpui::layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions};
        WindowKind::LayerShell(LayerShellOptions {
            namespace: "prompt-quick".to_string(),
            layer: Layer::Overlay,
            anchor: Anchor::TOP | Anchor::LEFT | Anchor::RIGHT,
            keyboard_interactivity: KeyboardInteractivity::OnDemand,
            ..Default::default()
        })
    } else {
        WindowKind::Normal
    }
}

/// On X11, mark the window always-on-top, sticky (all desktops), and
/// borderless via EWMH/Motif hints. No-op on Wayland (the layer-shell kind
/// already does all of this).
pub fn make_overlay(window: &Window) {
    if is_wayland() {
        return;
    }
    if let Err(error) = x11_overlay(window) {
        eprintln!("prompt: quick terminal: X11 overlay hints failed: {error}");
    }
}

fn x11_overlay(window: &Window) -> Result<(), Box<dyn Error>> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{AtomEnum, ConnectionExt, PropMode};
    use x11rb::wrapper::ConnectionExt as _;

    let win: u32 = match HasWindowHandle::window_handle(window)?.as_raw() {
        RawWindowHandle::Xlib(h) => h.window as u32,
        RawWindowHandle::Xcb(h) => h.window.get(),
        _ => return Ok(()),
    };

    let (conn, _screen) = x11rb::connect(None)?;
    let atom = |name: &[u8]| -> Result<u32, Box<dyn Error>> {
        Ok(conn.intern_atom(false, name)?.reply()?.atom)
    };

    // Always-on-top and present on every workspace.
    let wm_state = atom(b"_NET_WM_STATE")?;
    let above = atom(b"_NET_WM_STATE_ABOVE")?;
    let sticky = atom(b"_NET_WM_STATE_STICKY")?;
    conn.change_property32(
        PropMode::REPLACE,
        win,
        wm_state,
        AtomEnum::ATOM,
        &[above, sticky],
    )?;
    let desktop = atom(b"_NET_WM_DESKTOP")?;
    conn.change_property32(
        PropMode::REPLACE,
        win,
        desktop,
        AtomEnum::CARDINAL,
        &[0xFFFF_FFFF],
    )?;

    // Borderless: Motif hints with the decorations flag set and no decorations.
    let motif = atom(b"_MOTIF_WM_HINTS")?;
    conn.change_property32(PropMode::REPLACE, win, motif, motif, &[2, 0, 0, 0, 0])?;

    conn.flush()?;
    Ok(())
}
