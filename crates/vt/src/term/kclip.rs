//! Kitty's clipboard protocol, OSC 5522: clipboard reads and writes of any
//! MIME type, where OSC 52 only carries text.
//!
//! `OSC 5522 ; key=value:key=value ; base64 payload ST`. A write arrives as
//! `type=write`, then `type=wdata:mime=…` chunks, then an empty `type=wdata`
//! that ends it; this module assembles the chunks (bounded) and hands the
//! host one [`ClipboardRequest`]. A read is a single packet. Whether either
//! is allowed, and the clipboard itself, are the host's business; the reply
//! encoders here keep the wire format in one place.
//!
//! Spec: <https://sw.kovidgoyal.net/kitty/clipboard/>, plus the paste-event
//! addendum (mode 5522): <https://rockorager.dev/misc/bracketed-paste-mime/>.

use super::Inner;

/// Largest write accepted, decoded. The spec asks terminals to take at least
/// 64 MiB; anything past it is refused with `EFBIG` rather than buffered.
pub const MAX_WRITE_BYTES: usize = 64 * 1024 * 1024;

/// Bytes of data per `DATA` packet before encoding, as the spec requires.
const CHUNK: usize = 4096;

/// Requests waiting for the host. A program that floods reads without the
/// host draining them gets `EBUSY` instead of an ever-growing queue.
const MAX_PENDING: usize = 8;

/// The MIME "type" that asks for the list of available types.
pub const LIST: &str = ".";

/// A clipboard request for the host to vet and carry out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardRequest {
  /// Read `mimes` (or just list what is there, when it is [`LIST`]).
  Read {
    primary: bool,
    id: Option<String>,
    mimes: Vec<String>,
    auth: Option<Auth>,
  },
  /// Replace the clipboard with `items`, each a MIME type and its bytes.
  /// `aliases` offer one item under further type names.
  Write {
    primary: bool,
    id: Option<String>,
    items: Vec<(String, Vec<u8>)>,
    aliases: Vec<(String, Vec<String>)>,
    auth: Option<Auth>,
  },
}

/// A client's password and the name it goes by. The spec ignores a
/// password sent without a name, so one only exists with both.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Auth {
  pub password: String,
  pub name: String,
}

/// A write being assembled from `wdata` packets.
#[derive(Debug, Default)]
pub(crate) struct Pending {
  primary: bool,
  id: Option<String>,
  auth: Option<Auth>,
  /// Base64 text per MIME type. Kept encoded: only the concatenation of a
  /// type's chunks has to be validly padded, so it is decoded at the end.
  items: Vec<(String, Vec<u8>)>,
  aliases: Vec<(String, Vec<String>)>,
  /// Decoded size so far (estimated from the base64 length).
  bytes: usize,
  /// An error was reported; further `wdata` is ignored until a new write.
  failed: bool,
}

/// Handle `OSC 5522 ; meta [; payload]`.
pub(crate) fn dispatch(inner: &mut Inner, params: &[&[u8]]) {
  let meta = Meta::parse(params.get(1).copied().unwrap_or_default());
  let payload = params.get(2).copied().unwrap_or_default();
  match meta.kind {
    "read" => read(inner, &meta, payload),
    "write" => {
      inner.kclip_write = Some(Pending {
        primary: meta.primary,
        id: meta.id.clone(),
        auth: meta.auth(),
        ..Pending::default()
      });
    }
    "wdata" => wdata(inner, &meta, payload),
    "walias" => walias(inner, &meta, payload),
    _ => {}
  }
}

fn read(inner: &mut Inner, meta: &Meta, payload: &[u8]) {
  // A read with a bad payload has no error status to answer with: ignore it.
  let Some(list) = decode(payload).and_then(|b| String::from_utf8(b).ok()) else {
    return;
  };
  let mimes: Vec<String> = list.split_whitespace().map(str::to_string).collect();
  if mimes.is_empty() {
    return;
  }
  if inner.kclip_requests.len() >= MAX_PENDING {
    let reply = status("read", "EBUSY", meta.id.as_deref());
    inner.output.extend_from_slice(&reply);
    return;
  }
  inner.kclip_requests.push(ClipboardRequest::Read {
    primary: meta.primary,
    id: meta.id.clone(),
    mimes,
    auth: meta.auth(),
  });
}

fn wdata(inner: &mut Inner, meta: &Meta, payload: &[u8]) {
  let Some(pending) = inner.kclip_write.as_mut().filter(|p| !p.failed) else {
    return;
  };
  let Some(mime) = meta.mime.as_deref() else {
    if payload.is_empty() {
      finish(inner);
    } else {
      fail(inner, "EINVAL"); // data with no type to file it under
    }
    return;
  };
  pending.bytes += payload.len() / 4 * 3;
  if pending.bytes > MAX_WRITE_BYTES {
    fail(inner, "EFBIG");
    return;
  }
  match pending.items.last_mut() {
    Some((last, data)) if last == mime => data.extend_from_slice(payload),
    _ => pending.items.push((mime.to_string(), payload.to_vec())),
  }
}

fn walias(inner: &mut Inner, meta: &Meta, payload: &[u8]) {
  let Some(pending) = inner.kclip_write.as_mut().filter(|p| !p.failed) else {
    return;
  };
  let aliases = decode(payload).and_then(|b| String::from_utf8(b).ok());
  match (meta.mime.clone(), aliases) {
    (Some(target), Some(aliases)) if pending.aliases.len() < 64 => {
      let names = aliases
        .split_whitespace()
        .take(16)
        .map(str::to_string)
        .collect();
      pending.aliases.push((target, names));
    }
    (Some(_), Some(_)) => {}
    _ => fail(inner, "EINVAL"),
  }
}

/// End of a write: decode every type and queue the request.
fn finish(inner: &mut Inner) {
  let Some(pending) = inner.kclip_write.take() else {
    return;
  };
  let mut items = Vec::with_capacity(pending.items.len());
  for (mime, encoded) in pending.items {
    match decode_quads(&encoded) {
      Some(data) => items.push((mime, data)),
      None => {
        let reply = status("write", "EINVAL", pending.id.as_deref());
        inner.output.extend_from_slice(&reply);
        return;
      }
    }
  }
  if inner.kclip_requests.len() >= MAX_PENDING {
    let reply = status("write", "EBUSY", pending.id.as_deref());
    inner.output.extend_from_slice(&reply);
    return;
  }
  inner.kclip_requests.push(ClipboardRequest::Write {
    primary: pending.primary,
    id: pending.id,
    items,
    aliases: pending.aliases,
    auth: pending.auth,
  });
}

/// Report `code` for the write in progress, drop its data, and ignore the
/// rest of it.
fn fail(inner: &mut Inner, code: &str) {
  let id = inner.kclip_write.as_ref().and_then(|p| p.id.clone());
  inner.kclip_write = Some(Pending {
    failed: true,
    ..Pending::default()
  });
  let reply = status("write", code, id.as_deref());
  inner.output.extend_from_slice(&reply);
}

/// The metadata field, `key=value` pairs split on `:`.
struct Meta<'a> {
  kind: &'a str,
  mime: Option<String>,
  primary: bool,
  id: Option<String>,
  pw: Option<String>,
  name: Option<String>,
}

impl<'a> Meta<'a> {
  fn parse(raw: &'a [u8]) -> Self {
    let mut meta = Meta {
      kind: "",
      mime: None,
      primary: false,
      id: None,
      pw: None,
      name: None,
    };
    let Ok(raw) = std::str::from_utf8(raw) else {
      return meta;
    };
    for pair in raw.split(':') {
      let Some((key, value)) = pair.split_once('=') else {
        continue;
      };
      match key {
        "type" => meta.kind = value,
        "mime" => meta.mime = decode_str(value),
        "loc" => meta.primary = value == "primary",
        "id" => meta.id = Some(sanitize_id(value)).filter(|id| !id.is_empty()),
        "pw" => meta.pw = decode_str(value),
        "name" => meta.name = decode_str(value),
        _ => {}
      }
    }
    meta
  }

  fn auth(&self) -> Option<Auth> {
    Some(Auth {
      password: self.pw.clone().filter(|p| !p.is_empty())?,
      name: self.name.clone().filter(|n| !n.is_empty())?,
    })
  }
}

/// Keep only the characters the spec allows in an id, capped.
fn sanitize_id(id: &str) -> String {
  id.chars()
    .filter(|c| c.is_ascii_alphanumeric() || "-_+.".contains(*c))
    .take(64)
    .collect()
}

fn decode_str(value: &str) -> Option<String> {
  let bytes = decode(value.as_bytes())?;
  String::from_utf8(bytes).ok().filter(|s| s.len() <= 256)
}

/// Strict RFC 4648 base64: standard alphabet, padding required, and no
/// whitespace — the spec says invalid input is rejected, never repaired.
pub fn decode(input: &[u8]) -> Option<Vec<u8>> {
  let pad = input.iter().rev().take_while(|&&b| b == b'=').count();
  // Padding anywhere but the end is a concatenation, not one value.
  if input[..input.len() - pad].contains(&b'=') {
    return None;
  }
  decode_quads(input)
}

/// Decode a write's concatenated chunks: valid quads back to back, where a
/// padded quad may end any chunk. That takes both ways clients split data —
/// raw base64 cut anywhere (padded only at the very end, as the spec words
/// it) and each 4096-byte chunk encoded and padded on its own — while every
/// quad is still checked strictly.
fn decode_quads(input: &[u8]) -> Option<Vec<u8>> {
  if !input.len().is_multiple_of(4) {
    return None;
  }
  let mut out = Vec::with_capacity(input.len() / 4 * 3);
  for quad in input.chunks(4) {
    let pad = match quad {
      [_, _, b'=', b'='] => 2,
      [_, _, _, b'='] => 1,
      _ => 0,
    };
    let mut acc = 0u32;
    for &b in &quad[..4 - pad] {
      let v = match b {
        b'A'..=b'Z' => b - b'A',
        b'a'..=b'z' => b - b'a' + 26,
        b'0'..=b'9' => b - b'0' + 52,
        b'+' => 62,
        b'/' => 63,
        _ => return None,
      };
      acc = (acc << 6) | v as u32;
    }
    acc <<= 6 * pad;
    out.extend_from_slice(&acc.to_be_bytes()[1..4 - pad]);
  }
  Some(out)
}

/// Standard padded base64.
pub fn encode(input: &[u8]) -> String {
  const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
  let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
  for chunk in input.chunks(3) {
    let b = [
      chunk[0],
      *chunk.get(1).unwrap_or(&0),
      *chunk.get(2).unwrap_or(&0),
    ];
    let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
    for i in 0..4 {
      if i <= chunk.len() {
        out.push(ABC[(n >> (18 - 6 * i) & 63) as usize] as char);
      } else {
        out.push('=');
      }
    }
  }
  out
}

fn packet(meta: &str, payload: Option<&str>) -> Vec<u8> {
  let mut out = format!("\x1b]5522;{meta}");
  if let Some(payload) = payload {
    out.push(';');
    out.push_str(payload);
  }
  out.push_str("\x1b\\");
  out.into_bytes()
}

fn with_id(mut meta: String, id: Option<&str>) -> String {
  if let Some(id) = id {
    meta.push_str(":id=");
    meta.push_str(id);
  }
  meta
}

/// A bare status reply: `type=<kind>:status=<code>`.
pub fn status(kind: &str, code: &str, id: Option<&str>) -> Vec<u8> {
  packet(&with_id(format!("type={kind}:status={code}"), id), None)
}

/// A successful read: `OK`, each item's data in [`CHUNK`]-sized `DATA`
/// packets, then `DONE`. An item may be empty; it still gets one packet.
pub fn read_reply(id: Option<&str>, items: &[(&str, &[u8])]) -> Vec<u8> {
  let mut out = status("read", "OK", id);
  for (mime, data) in items {
    let meta = with_id(
      format!("type=read:status=DATA:mime={}", encode(mime.as_bytes())),
      id,
    );
    let mut chunks = data.chunks(CHUNK).peekable();
    if chunks.peek().is_none() {
      out.extend(packet(&meta, Some("")));
    }
    for chunk in chunks {
      out.extend(packet(&meta, Some(&encode(chunk))));
    }
  }
  out.extend(status("read", "DONE", id));
  out
}

/// The answer to a [`LIST`] read: the available types, space-separated.
pub fn list_reply(id: Option<&str>, mimes: &[&str]) -> Vec<u8> {
  read_reply(id, &[(LIST, mimes.join(" ").as_bytes())])
}

/// What a paste sends instead of bracketed text while mode 5522 is on: the
/// pasted types, with a one-time `password` that lets the program read them
/// back without a prompt.
pub fn paste_event(primary: bool, password: &str, mimes: &[&str]) -> Vec<u8> {
  let loc = if primary { ":loc=primary" } else { "" };
  let pw = format!(":pw={}", encode(password.as_bytes()));
  let mut out = packet(&format!("type=read:status=OK{loc}{pw}"), None);
  let list = encode(mimes.join(" ").as_bytes());
  let data = format!("type=read:status=DATA:mime={}{pw}", encode(LIST.as_bytes()));
  out.extend(packet(&data, Some(&list)));
  out.extend(packet(&format!("type=read:status=DONE{pw}"), None));
  out
}

#[cfg(test)]
#[path = "../../tests/term/kclip.rs"]
mod tests;
