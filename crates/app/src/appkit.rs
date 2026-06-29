//! macOS AppKit reach-through for the quick terminal. gpui exposes no
//! window-level or visibility controls, but it implements `HasWindowHandle`,
//! so we borrow the live `NSWindow` and message it directly to float the
//! window above every app and Space and to hide/show it without destroying
//! the session.

use gpui::Window;

/// Turn `window` into an overlay: above other applications, present on every
/// Space, and drawn over fullscreen apps. Idempotent.
pub fn make_overlay(window: &Window) {
    imp::make_overlay(window);
}

/// Order the window in and make it key, preserving its session.
pub fn show(window: &Window) {
    imp::show(window);
}

/// Order the window out (hidden but alive), preserving its session.
pub fn hide(window: &Window) {
    imp::hide(window);
}

/// Whether the window is currently on screen.
pub fn is_visible(window: &Window) -> bool {
    imp::is_visible(window)
}

/// The active keyboard layout's input-source id (e.g.
/// `com.apple.keylayout.US`), or `None` off macOS or when it can't be read.
/// Used to auto-decide `macos-option-as-alt`.
pub fn keyboard_layout_id() -> Option<String> {
    imp::keyboard_layout_id()
}

#[cfg(target_os = "macos")]
mod imp {
    use gpui::Window;
    use objc2_app_kit::{
        NSStatusWindowLevel, NSView, NSWindow, NSWindowButton, NSWindowCollectionBehavior,
    };
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    pub fn make_overlay(window: &Window) {
        with_nswindow(window, |w| {
            w.setLevel(NSStatusWindowLevel);
            w.setCollectionBehavior(
                NSWindowCollectionBehavior::CanJoinAllSpaces
                    | NSWindowCollectionBehavior::FullScreenAuxiliary
                    | NSWindowCollectionBehavior::Stationary,
            );
            for button in [
                NSWindowButton::CloseButton,
                NSWindowButton::MiniaturizeButton,
                NSWindowButton::ZoomButton,
            ] {
                if let Some(button) = w.standardWindowButton(button) {
                    button.setHidden(true);
                }
            }
        });
    }

    pub fn show(window: &Window) {
        with_nswindow(window, |w| w.makeKeyAndOrderFront(None));
    }

    pub fn hide(window: &Window) {
        with_nswindow(window, |w| w.orderOut(None));
    }

    pub fn is_visible(window: &Window) -> bool {
        let mut visible = false;
        with_nswindow(window, |w| visible = w.isVisible());
        visible
    }

    use std::ffi::{c_char, c_void, CStr};

    type CFTypeRef = *const c_void;
    type CFStringRef = *const c_void;
    const UTF8: u32 = 0x0800_0100;

    // SAFETY: standard system framework symbols. `TISCopy*` returns a +1
    // reference we release; `TISGetInputSourceProperty` returns a borrowed
    // value we must not release.
    #[allow(non_upper_case_globals)]
    unsafe extern "C" {
        fn TISCopyCurrentKeyboardInputSource() -> CFTypeRef;
        fn TISGetInputSourceProperty(source: CFTypeRef, key: CFStringRef) -> *mut c_void;
        static kTISPropertyInputSourceID: CFStringRef;
        fn CFStringGetCStringPtr(s: CFStringRef, encoding: u32) -> *const c_char;
        fn CFStringGetCString(s: CFStringRef, buf: *mut c_char, size: isize, encoding: u32) -> u8;
        fn CFStringGetLength(s: CFStringRef) -> isize;
        fn CFRelease(cf: CFTypeRef);
    }

    pub fn keyboard_layout_id() -> Option<String> {
        unsafe {
            let src = TISCopyCurrentKeyboardInputSource();
            if src.is_null() {
                return None;
            }
            let id = TISGetInputSourceProperty(src, kTISPropertyInputSourceID) as CFStringRef;
            let out = if id.is_null() {
                None
            } else {
                cfstring_to_string(id)
            };
            CFRelease(src);
            out
        }
    }

    /// Copy a `CFStringRef` into an owned `String`.
    unsafe fn cfstring_to_string(s: CFStringRef) -> Option<String> {
        let ptr = CFStringGetCStringPtr(s, UTF8);
        if !ptr.is_null() {
            return CStr::from_ptr(ptr).to_str().ok().map(str::to_owned);
        }
        let cap = (CFStringGetLength(s) * 4 + 1).max(16);
        let mut buf = vec![0 as c_char; cap as usize];
        if CFStringGetCString(s, buf.as_mut_ptr(), cap, UTF8) != 0 {
            CStr::from_ptr(buf.as_ptr()).to_str().ok().map(str::to_owned)
        } else {
            None
        }
    }

    /// Run `f` with the window's `NSWindow`, if its native handle is live.
    /// Must be called on the main thread (gpui guarantees this for the
    /// `handle.update`/render closures we call it from).
    fn with_nswindow(window: &Window, f: impl FnOnce(&NSWindow)) {
        let Ok(handle) = HasWindowHandle::window_handle(window) else {
            return;
        };
        let RawWindowHandle::AppKit(h) = handle.as_raw() else {
            return;
        };
        // SAFETY: gpui hands us a valid, retained NSView pointer for the
        // lifetime of the window; we only borrow it for this call.
        let view: &NSView = unsafe { &*(h.ns_view.as_ptr() as *const NSView) };
        if let Some(nswindow) = view.window() {
            f(&nswindow);
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use gpui::Window;

    pub fn make_overlay(_window: &Window) {}
    pub fn show(_window: &Window) {}
    pub fn hide(_window: &Window) {}
    pub fn is_visible(_window: &Window) -> bool {
        true
    }
    pub fn keyboard_layout_id() -> Option<String> {
        None
    }
}
