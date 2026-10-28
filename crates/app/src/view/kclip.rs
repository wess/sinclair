//! Kitty's clipboard protocol (OSC 5522) on the app side: who may read or
//! write the clipboard, and doing it.
//!
//! `vt` parses the packets and assembles writes; this decides. Listing the
//! clipboard's types is always allowed (the spec exempts it). A read or write
//! follows `clipboard-read` / `clipboard-write` — allow, ask, or deny — with
//! two ways past an `ask`:
//!
//! - a **paste event** (mode 5522): the user's paste hands the program a
//!   one-time password for the pasted data, scoped to its location and
//!   short-lived, so the read the paste invites needs no second prompt;
//! - a **remembered client**: answering "always" to a prompt from a client
//!   that sent a password and name trusts that pair for this pane.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use gpui::{ClipboardEntry, ClipboardItem, ImageFormat};
use vt::term::kclip;
use vt::{Auth, ClipboardRequest};

use super::*;

/// How long a paste event's password stays good.
const PASTE_TOKEN_TTL: Duration = Duration::from_secs(30);

/// The name the paste-event addendum says a client reads a paste under.
const PASTE_EVENT: &str = "Paste event";

/// Per-pane protocol state.
#[derive(Default)]
pub(crate) struct KittyClipboard {
  /// The newest paste event's password, its location, and its expiry.
  token: Option<(String, bool, Instant)>,
  /// Clients the user said to always trust, by password and name.
  trusted: HashSet<Auth>,
}

/// MIME types that mean plain text on every platform's clipboard.
fn is_text(mime: &str) -> bool {
  let base = mime.split(';').next().unwrap_or(mime).trim();
  base.eq_ignore_ascii_case("text/plain") || matches!(mime, "UTF8_STRING" | "TEXT" | "STRING")
}

/// The MIME types `item` can be read back as.
pub(crate) fn types_of(item: &ClipboardItem) -> Vec<&'static str> {
  let mut types = Vec::new();
  for entry in item.entries() {
    let mime = match entry {
      ClipboardEntry::String(_) => "text/plain",
      ClipboardEntry::Image(image) => image.format.mime_type(),
      ClipboardEntry::ExternalPaths(_) => continue,
    };
    if !types.contains(&mime) {
      types.push(mime);
    }
  }
  types
}

/// Build what a write can put on the system clipboard: text types become the
/// string entry, image types an image; anything else (HTML, rich text) has no
/// slot on gpui's clipboard and is left out. An alias naming a text type
/// marks its target as text.
pub(crate) fn clipboard_item(
  items: &[(String, Vec<u8>)],
  aliases: &[(String, Vec<String>)],
) -> Option<ClipboardItem> {
  let mut entries = Vec::new();
  let mut has_text = false;
  for (mime, data) in items {
    let aliased_text = aliases
      .iter()
      .any(|(target, names)| target == mime && names.iter().any(|n| is_text(n)));
    if is_text(mime) || aliased_text {
      if !has_text {
        has_text = true;
        let text = String::from_utf8_lossy(data).into_owned();
        entries.push(ClipboardEntry::String(gpui::ClipboardString::new(text)));
      }
    } else if let Some(format) = ImageFormat::from_mime_type(mime) {
      entries.push(ClipboardEntry::Image(gpui::Image::from_bytes(
        format,
        data.clone(),
      )));
    }
  }
  (!entries.is_empty()).then_some(ClipboardItem { entries })
}

/// The requested data from `item`, in request order, skipping types it does
/// not have. Several names for text all read the one string.
pub(crate) fn read_items(item: &ClipboardItem, mimes: &[String]) -> Vec<(String, Vec<u8>)> {
  let mut out = Vec::new();
  for mime in mimes {
    let data = item.entries().iter().find_map(|entry| match entry {
      ClipboardEntry::String(s) if is_text(mime) => Some(s.text().as_bytes().to_vec()),
      ClipboardEntry::Image(image) if image.format.mime_type() == mime => Some(image.bytes.clone()),
      _ => None,
    });
    if let Some(data) = data {
      out.push((mime.clone(), data));
    }
  }
  out
}

/// A fresh one-time password. Not cryptographic, but unguessable to a
/// program that never saw it: hashed from the process's random hasher keys,
/// the time, and a counter.
fn one_time_password() -> String {
  use std::hash::{BuildHasher, Hasher};
  use std::sync::atomic::{AtomicU64, Ordering};
  static COUNTER: AtomicU64 = AtomicU64::new(0);
  let n = COUNTER.fetch_add(1, Ordering::Relaxed);
  let now = std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .map(|d| d.as_nanos())
    .unwrap_or_default();
  let half = |salt: u64| {
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(now);
    h.write_u64(n ^ salt);
    h.finish()
  };
  format!("{:016x}{:016x}", half(0), half(u64::MAX))
}

impl TerminalView {
  /// Act on an OSC 5522 request from the program.
  pub(crate) fn clipboard_request(&mut self, request: ClipboardRequest, cx: &mut Context<Self>) {
    let (policy, primary, auth) = match &request {
      ClipboardRequest::Read {
        mimes,
        primary,
        id,
        auth,
      } => {
        if mimes.len() == 1 && mimes[0] == kclip::LIST {
          // Listing types reveals no content and needs no permission.
          let item = self.clipboard_item(*primary, cx);
          let types = item.as_ref().map(types_of).unwrap_or_default();
          let _ = self
            .session
            .write(&kclip::list_reply(id.as_deref(), &types));
          return;
        }
        (paneopts::clipboard_read(cx), *primary, auth.clone())
      }
      ClipboardRequest::Write { primary, auth, .. } => {
        (self.clipboard_write, *primary, auth.clone())
      }
    };
    let granted = auth
      .as_ref()
      .is_some_and(|auth| self.kclip_grants(auth, primary, &request));
    match (granted, policy) {
      (true, _) | (false, config::ClipboardAccess::Allow) => self.clipboard_perform(request, cx),
      (false, config::ClipboardAccess::Deny) => self.clipboard_refuse(&request),
      (false, config::ClipboardAccess::Ask) if self.assist.is_some() => {
        // Never swap a prompt out from under the user's keypress; the
        // program can ask again once this one is answered.
        let reply = match &request {
          ClipboardRequest::Read { id, .. } => kclip::status("read", "EBUSY", id.as_deref()),
          ClipboardRequest::Write { id, .. } => kclip::status("write", "EBUSY", id.as_deref()),
        };
        let _ = self.session.write(&reply);
      }
      (false, config::ClipboardAccess::Ask) => {
        self.search = None;
        self.assist = Some(Assist::KittyClipboard { request });
        cx.notify();
      }
    }
  }

  /// Whether `auth` lets this request skip the prompt: a remembered client,
  /// or a read carrying the live paste event's password for the same
  /// location (which it then uses up).
  fn kclip_grants(&mut self, auth: &Auth, primary: bool, request: &ClipboardRequest) -> bool {
    if self.kclip.trusted.contains(auth) {
      return true;
    }
    let is_read = matches!(request, ClipboardRequest::Read { .. });
    let live = self.kclip.token.as_ref().is_some_and(|(pw, loc, expires)| {
      *pw == auth.password && *loc == primary && Instant::now() < *expires
    });
    if is_read && live && auth.name == PASTE_EVENT {
      self.kclip.token = None;
      return true;
    }
    false
  }

  /// The user's answer to the prompt. `always` remembers the client when it
  /// identified itself with a password and name.
  pub(crate) fn clipboard_answer(
    &mut self,
    request: ClipboardRequest,
    allow: bool,
    always: bool,
    cx: &mut Context<Self>,
  ) {
    if !allow {
      self.clipboard_refuse(&request);
      return;
    }
    if always {
      let auth = match &request {
        ClipboardRequest::Read { auth, .. } | ClipboardRequest::Write { auth, .. } => auth.clone(),
      };
      if let Some(auth) = auth {
        self.kclip.trusted.insert(auth);
      }
    }
    self.clipboard_perform(request, cx);
  }

  fn clipboard_refuse(&self, request: &ClipboardRequest) {
    let reply = match request {
      ClipboardRequest::Read { id, .. } => kclip::status("read", "EPERM", id.as_deref()),
      ClipboardRequest::Write { id, .. } => kclip::status("write", "EPERM", id.as_deref()),
    };
    let _ = self.session.write(&reply);
  }

  fn clipboard_perform(&mut self, request: ClipboardRequest, cx: &mut Context<Self>) {
    let reply = match request {
      ClipboardRequest::Read {
        primary, id, mimes, ..
      } => match self.clipboard_item(primary, cx) {
        Some(item) => {
          let data = read_items(&item, &mimes);
          let refs: Vec<(&str, &[u8])> = data
            .iter()
            .map(|(m, d)| (m.as_str(), d.as_slice()))
            .collect();
          kclip::read_reply(id.as_deref(), &refs)
        }
        None => kclip::read_reply(id.as_deref(), &[]),
      },
      ClipboardRequest::Write {
        primary,
        id,
        items,
        aliases,
        ..
      } => {
        let status = match clipboard_item(&items, &aliases) {
          None => "EIO",
          Some(item) if primary => match item.text() {
            Some(text) => {
              crate::clipboard::write_primary(text, cx);
              "DONE"
            }
            // The primary selection holds text only.
            None => "ENOSYS",
          },
          Some(item) => {
            if let Some(text) = item.text() {
              crate::clipboard::remember(&text, cx);
            }
            cx.write_to_clipboard(item);
            "DONE"
          }
        };
        kclip::status("write", status, id.as_deref())
      }
    };
    let _ = self.session.write(&reply);
  }

  /// The clipboard (or primary selection) as a gpui item, if it has one.
  fn clipboard_item(&self, primary: bool, cx: &App) -> Option<ClipboardItem> {
    if primary {
      crate::clipboard::read_primary(cx).map(ClipboardItem::new_string)
    } else {
      cx.read_from_clipboard()
    }
  }

  /// If the program asked for paste events (mode 5522), announce this paste
  /// as its MIME types plus a one-time password and return `true`; the
  /// program reads the data itself. Otherwise `false`: paste as text.
  pub(crate) fn paste_event(&mut self, primary: bool, cx: &mut Context<Self>) -> bool {
    if self.read_only || !self.session.with_term(|t| t.clipboard_paste_events()) {
      return false;
    }
    let types = self
      .clipboard_item(primary, cx)
      .as_ref()
      .map(types_of)
      .unwrap_or_default();
    if types.is_empty() {
      return true; // nothing to paste; still not text's to send
    }
    let password = one_time_password();
    self.kclip.token = Some((password.clone(), primary, Instant::now() + PASTE_TOKEN_TTL));
    self.scroll_to_bottom(cx);
    let _ = self
      .session
      .write(&kclip::paste_event(primary, &password, &types));
    true
  }

  /// One line describing a pending request, for the prompt.
  pub(crate) fn clipboard_summary(request: &ClipboardRequest) -> String {
    let who = |auth: &Option<Auth>| {
      auth
        .as_ref()
        .map(|a| format!("“{}”", a.name))
        .unwrap_or_else(|| "A program".to_string())
    };
    let place = |primary: bool| {
      if primary {
        "primary selection"
      } else {
        "clipboard"
      }
    };
    match request {
      ClipboardRequest::Read {
        primary,
        mimes,
        auth,
        ..
      } => format!(
        "{} wants to read the {} ({}).",
        who(auth),
        place(*primary),
        mimes.join(", ")
      ),
      ClipboardRequest::Write {
        primary,
        items,
        auth,
        ..
      } => {
        let what: Vec<String> = items
          .iter()
          .map(|(mime, data)| format!("{mime}, {}", size(data.len())))
          .collect();
        format!(
          "{} wants to set the {} ({}).",
          who(auth),
          place(*primary),
          what.join("; ")
        )
      }
    }
  }
}

fn size(bytes: usize) -> String {
  match bytes {
    b if b < 1024 => format!("{b} B"),
    b if b < 1024 * 1024 => format!("{:.1} KB", b as f64 / 1024.0),
    b => format!("{:.1} MB", b as f64 / (1024.0 * 1024.0)),
  }
}

#[cfg(test)]
#[path = "../../tests/kclip.rs"]
mod tests;
