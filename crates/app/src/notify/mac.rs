//! macOS notifications via UserNotifications, so alerts carry Sinclair's icon.
//!
//! UserNotifications attributes every alert to the running bundle, which is the
//! whole point — but it also means the framework needs a real bundle identity,
//! and `UNUserNotificationCenter.currentNotificationCenter` throws an
//! Objective-C exception outright when there isn't one (a `cargo run` build
//! straight out of `target/`). Every entry point here is written to fail by
//! returning `false` so the caller can fall back to `osascript`, and the throw
//! is caught rather than allowed to tear down the terminal.

use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::Once;
use std::time::Duration;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::Bool;
use objc2_foundation::{NSBundle, NSError, NSString};
use objc2_user_notifications::{
    UNAuthorizationOptions, UNMutableNotificationContent, UNNotificationRequest,
    UNUserNotificationCenter,
};

/// How long to wait for the authorization answer before posting anyway. On the
/// very first run this is the user reading a system dialog; every run after, the
/// answer is cached and the callback is immediate.
///
/// Deliberately short. `send` is synchronous and its callers are not all on
/// throwaway threads, so this bound is a UI-freeze budget, not a politeness
/// timeout — if the user is still reading the dialog when it expires we post
/// anyway and the system queues or drops it on its own terms.
const AUTH_WAIT: Duration = Duration::from_secs(2);

/// How long to wait for the notification to actually reach the notification
/// daemon. `send` promises the caller it has posted before returning, and
/// `sinclair notify` exits the moment it does — without this the XPC hand-off
/// races process teardown and the notification is silently lost.
const POST_WAIT: Duration = Duration::from_secs(2);

/// Ensures we only ask for authorization once per process.
static ASKED: Once = Once::new();

/// Makes each request identifier unique — reusing one replaces the notification
/// already on screen instead of posting a new one.
static SEQ: AtomicU64 = AtomicU64::new(0);

/// Post through UserNotifications. `false` means this build can't use the
/// framework and the caller should fall back.
pub(crate) fn send(title: &str, body: &str) -> bool {
    let Some(center) = center() else {
        return false;
    };
    authorize(&center);
    // AssertUnwindSafe: the closure only makes Objective-C calls, and a throw
    // leaves nothing half-written on the Rust side for us to observe after.
    objc2::exception::catch(AssertUnwindSafe(|| {
        let content = UNMutableNotificationContent::new();
        content.setTitle(&NSString::from_str(title));
        content.setBody(&NSString::from_str(body));
        let id = format!("io.wess.sinclair.{}", SEQ.fetch_add(1, Ordering::Relaxed));
        let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
            &NSString::from_str(&id),
            &content,
            None,
        );
        let (tx, rx) = mpsc::channel();
        let done = RcBlock::new(move |_err: *mut NSError| {
            let _ = tx.send(());
        });
        center.addNotificationRequest_withCompletionHandler(&request, Some(&done));
        let _ = rx.recv_timeout(POST_WAIT);
    }))
    .is_ok()
}

/// The notification center, or `None` when this process has no bundle identity
/// for the framework to attribute alerts to.
fn center() -> Option<Retained<UNUserNotificationCenter>> {
    let id = NSBundle::mainBundle().bundleIdentifier()?;
    if id.to_string().is_empty() {
        return None;
    }
    objc2::exception::catch(UNUserNotificationCenter::currentNotificationCenter).ok()
}

/// Request alert authorization once per process, waiting for the answer so the
/// notification that follows isn't dropped for being posted too early. A denial
/// or a timeout is not an error — we post regardless and let the system decide,
/// exactly as it would for any other app the user has muted.
fn authorize(center: &UNUserNotificationCenter) {
    ASKED.call_once(|| {
        let (tx, rx) = mpsc::channel();
        let handler = RcBlock::new(move |_granted: Bool, _err: *mut NSError| {
            let _ = tx.send(());
        });
        let requested = objc2::exception::catch(AssertUnwindSafe(|| {
            center.requestAuthorizationWithOptions_completionHandler(
                UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound,
                &handler,
            );
        }));
        if requested.is_ok() {
            let _ = rx.recv_timeout(AUTH_WAIT);
        }
    });
}
