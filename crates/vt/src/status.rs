//! Program status: what the programs in a terminal say they are doing.
//!
//! Two sequences feed it. OSC 7501 (the Program Status Protocol, revision 0.4)
//! carries a state — idle, working, done, blocked, error — plus why, how far
//! along, and a line of text, for any number of records keyed by a `/` path
//! id. OSC 9;4 (ConEmu progress) is the older busy/percent/error signal; it is
//! mapped onto the root record until the first OSC 7501 report arrives, since
//! a mapped 9;4 would otherwise wipe out a real report's `kind` and `msg`.
//!
//! Records live with the terminal, not a screen: alt-screen switches leave
//! them alone, RIS removes them all, DECSTR does not. `working` and `blocked`
//! records end when a new shell prompt starts (OSC 133 `A`) or the child
//! exits; `done` and `error` outlive both so the user can find them.
//!
//! Everything here is untrusted program input. A report that breaks a limit,
//! carries bad base64, or decodes to a control character is dropped whole.

/// A record's state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum State {
  Idle,
  Working,
  Done,
  Blocked,
  Error,
}

impl State {
  fn parse(s: &str) -> Option<State> {
    Some(match s {
      "idle" => State::Idle,
      "working" => State::Working,
      "done" => State::Done,
      "blocked" => State::Blocked,
      "error" => State::Error,
      _ => return None,
    })
  }

  /// The protocol's lowercase name.
  pub fn label(self) -> &'static str {
    match self {
      State::Idle => "idle",
      State::Working => "working",
      State::Done => "done",
      State::Blocked => "blocked",
      State::Error => "error",
    }
  }

  /// How much this state wants the user, highest first: something waiting on
  /// them, then something that failed, then something running, then a result
  /// to look at, then nothing.
  pub fn urgency(self) -> u8 {
    match self {
      State::Blocked => 4,
      State::Error => 3,
      State::Working => 2,
      State::Done => 1,
      State::Idle => 0,
    }
  }

  /// Whether the record ends at the next prompt or child exit.
  fn transient(self) -> bool {
    matches!(self, State::Working | State::Blocked)
  }
}

/// What a `blocked` record is waiting for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
  /// Approval to do something.
  Permission,
  /// An answer the user must type.
  Question,
  /// A login, token, or credential.
  Auth,
}

impl Kind {
  fn parse(s: &str) -> Option<Kind> {
    Some(match s {
      "permission" => Kind::Permission,
      "question" => Kind::Question,
      "auth" => Kind::Auth,
      _ => return None,
    })
  }

  pub fn label(self) -> &'static str {
    match self {
      Kind::Permission => "permission",
      Kind::Question => "question",
      Kind::Auth => "auth",
    }
  }
}

/// One stored record. `id` is empty for the root record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
  pub id: String,
  pub state: State,
  pub kind: Option<Kind>,
  /// 0–100; `None` is indeterminate.
  pub progress: Option<u8>,
  pub app: Option<String>,
  pub title: Option<String>,
  pub msg: Option<String>,
  /// Update order across the board; larger is newer.
  pub seq: u64,
}

impl Record {
  /// How deep the id nests: 0 for the root record, 1 for `build`, 2 for
  /// `build/test`.
  pub fn depth(&self) -> usize {
    if self.id.is_empty() {
      0
    } else {
      self.id.split('/').count()
    }
  }

  /// Whether `self` is `other` or one of its descendants.
  fn under(&self, other: &str) -> bool {
    other.is_empty()
      || self.id == other
      || (self.id.len() > other.len()
        && self.id.starts_with(other)
        && self.id.as_bytes()[other.len()] == b'/')
  }
}

/// A parsed OSC 7501 body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Report {
  /// `?`: feature detection.
  Query,
  /// `state=clear`, removing `id` (empty: everything) and what sits under it.
  Clear(String),
  /// A record to store, replacing any record with the same id.
  Set(Record),
}

/// The feature-detection reply body (after `OSC`, before `ST`).
pub const QUERY_REPLY: &str =
  "7501;?:states=idle,working,done,blocked,error:kinds=permission,question,auth";

/// Whole sequence, `OSC` through `ST`.
pub const MAX_SEQUENCE: usize = 4096;
const MAX_KEY: usize = 16;
const MAX_MSG_ENCODED: usize = 2732;
const MAX_MSG: usize = 2048;
const MAX_TITLE_ENCODED: usize = 256;
const MAX_TITLE: usize = 192;
const MAX_APP: usize = 32;
const MAX_ID: usize = 128;
const MAX_SEGMENT: usize = 32;
const MAX_DEPTH: usize = 8;
/// Records kept per terminal; past this the least recently updated goes.
pub const MAX_RECORDS: usize = 256;

/// Parse the body of an OSC 7501 (everything after `7501;`). `None` means the
/// report is ignored or discarded.
pub fn parse(body: &[u8]) -> Option<Report> {
  if body.len() > MAX_SEQUENCE {
    return None;
  }
  let text = std::str::from_utf8(body).ok()?;
  if text.trim() == "?" {
    return Some(Report::Query);
  }
  let mut state = None;
  let mut id = None;
  let mut kind = None;
  let mut progress = None;
  let mut app = None;
  let mut title = None;
  let mut msg = None;
  for pair in text.split(':') {
    let Some((key, value)) = pair.split_once('=') else {
      continue;
    };
    let (key, value) = (key.trim(), value.trim());
    if key.is_empty() || !key.bytes().all(|b| b.is_ascii_lowercase()) {
      continue;
    }
    // A key past the limit breaks the report, not just the pair.
    if key.len() > MAX_KEY {
      return None;
    }
    if !value.bytes().all(value_byte) {
      continue;
    }
    match key {
      "state" => state = Some(value),
      "id" => id = Some(value),
      "kind" => kind = Some(value),
      "progress" => progress = Some(value),
      "app" => app = Some(value),
      "title" => title = Some(value),
      "msg" => msg = Some(value),
      _ => {}
    }
  }
  let id = match id {
    Some(id) => {
      if id.len() > MAX_ID {
        return None;
      }
      if !valid_id(id) {
        return None;
      }
      id.to_string()
    }
    None => String::new(),
  };
  // Size limits are checked before any decoding.
  if msg.is_some_and(|m| m.len() > MAX_MSG_ENCODED)
    || title.is_some_and(|t| t.len() > MAX_TITLE_ENCODED)
  {
    return None;
  }
  let state = state?;
  if state == "clear" {
    return Some(Report::Clear(id));
  }
  let state = State::parse(state)?;
  let msg = match msg {
    Some(m) => Some(decode_text(m, MAX_MSG)?),
    None => None,
  };
  let title = match title {
    Some(t) => Some(decode_text(t, MAX_TITLE)?),
    None => None,
  };
  let kind = kind
    .filter(|_| state == State::Blocked)
    .and_then(Kind::parse);
  let progress = progress
    .filter(|_| matches!(state, State::Working | State::Blocked))
    .and_then(|p| p.parse::<u8>().ok())
    .filter(|&p| p <= 100);
  let app = app
    .filter(|a| a.len() <= MAX_APP && !a.is_empty() && a.bytes().all(segment_byte))
    .map(str::to_string);
  Some(Report::Set(Record {
    id,
    state,
    kind,
    progress,
    app,
    title,
    msg,
    seq: 0,
  }))
}

fn value_byte(b: u8) -> bool {
  b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b',' | b'+' | b'/' | b'=' | b'-')
}

fn segment_byte(b: u8) -> bool {
  b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'+' | b'-')
}

fn valid_id(id: &str) -> bool {
  let segments: Vec<&str> = id.split('/').collect();
  segments.len() <= MAX_DEPTH
    && segments
      .iter()
      .all(|s| !s.is_empty() && s.len() <= MAX_SEGMENT && s.bytes().all(segment_byte))
}

/// Decode a base64 text value. `None` (discard the report) on bad base64,
/// non-UTF-8, an oversized result, or any control character.
fn decode_text(value: &str, max: usize) -> Option<String> {
  let bytes = crate::term::report::base64_decode(value.as_bytes())?;
  if bytes.len() > max {
    return None;
  }
  let text = String::from_utf8(bytes).ok()?;
  if text.chars().any(char::is_control) {
    return None;
  }
  Some(text)
}

/// Strip the invisible formatting characters — bidi overrides and isolates,
/// zero-width joiners and marks — that could make text shown outside the grid
/// read differently from what it is.
pub fn disarm(text: &str) -> String {
  text
    .chars()
    .filter(|&c| {
      !matches!(c,
        '\u{200B}'..='\u{200F}'
        | '\u{202A}'..='\u{202E}'
        | '\u{2060}'..='\u{2064}'
        | '\u{2066}'..='\u{2069}'
        | '\u{FEFF}')
    })
    .collect()
}

/// The records one terminal holds.
#[derive(Clone, Debug, Default)]
pub struct Board {
  records: Vec<Record>,
  seq: u64,
  /// An OSC 7501 report arrived since the last RIS; OSC 9;4 stops mapping.
  native: bool,
  changed: bool,
}

impl Board {
  pub fn records(&self) -> &[Record] {
    &self.records
  }

  pub fn is_empty(&self) -> bool {
    self.records.is_empty()
  }

  /// Whether anything changed since the last call.
  pub fn take_changed(&mut self) -> bool {
    std::mem::take(&mut self.changed)
  }

  /// Apply a parsed OSC 7501 report. Returns the reply to write back, if any.
  pub fn apply(&mut self, report: Report) -> Option<&'static str> {
    match report {
      Report::Query => return Some(QUERY_REPLY),
      Report::Clear(id) => {
        self.native = true;
        self.clear(&id);
      }
      Report::Set(record) => {
        self.native = true;
        self.store(record);
      }
    }
    None
  }

  /// Map an OSC 9;4 progress report (`st`, `pr`) onto the root record, unless
  /// a native report has been seen.
  pub fn progress(&mut self, st: u8, pr: Option<u8>) {
    if self.native {
      return;
    }
    let pr = pr.filter(|&p| p <= 100);
    let (state, progress) = match st {
      0 => {
        self.clear_root();
        return;
      }
      1 => (State::Working, Some(pr.unwrap_or(0))),
      2 => (State::Error, None),
      3 => (State::Working, None),
      4 => (State::Working, pr),
      _ => return,
    };
    self.store(Record {
      id: String::new(),
      state,
      kind: None,
      progress,
      app: None,
      title: None,
      msg: None,
      seq: 0,
    });
  }

  /// A new shell prompt began: drop `working` and `blocked` records.
  pub fn prompt(&mut self) {
    self.drop_transient();
  }

  /// The child process exited: drop `working` and `blocked` records.
  pub fn exited(&mut self) {
    self.drop_transient();
  }

  /// RIS: remove everything and start mapping OSC 9;4 again.
  pub fn reset(&mut self) {
    if !self.records.is_empty() {
      self.changed = true;
    }
    self.records.clear();
    self.native = false;
  }

  /// Drop `done` and `error` records: the user has seen them.
  pub fn acknowledge(&mut self) {
    let before = self.records.len();
    self
      .records
      .retain(|r| !matches!(r.state, State::Done | State::Error));
    if self.records.len() != before {
      self.changed = true;
    }
  }

  /// The record's `app`, or its nearest ancestor's.
  pub fn app_of<'a>(&'a self, record: &'a Record) -> Option<&'a str> {
    if let Some(app) = record.app.as_deref() {
      return Some(app);
    }
    let mut id = record.id.as_str();
    while !id.is_empty() {
      id = id.rsplit_once('/').map_or("", |(parent, _)| parent);
      if let Some(app) = self
        .records
        .iter()
        .find(|r| r.id == id)
        .and_then(|r| r.app.as_deref())
      {
        return Some(app);
      }
    }
    None
  }

  /// The record that best stands for the whole terminal: the most urgent,
  /// newest first among equals.
  pub fn headline(&self) -> Option<&Record> {
    self
      .records
      .iter()
      .max_by_key(|r| (r.state.urgency(), r.seq))
  }

  fn store(&mut self, mut record: Record) {
    self.seq += 1;
    record.seq = self.seq;
    if let Some(slot) = self.records.iter_mut().find(|r| r.id == record.id) {
      *slot = record;
    } else {
      if self.records.len() >= MAX_RECORDS {
        if let Some(oldest) = self
          .records
          .iter()
          .enumerate()
          .min_by_key(|(_, r)| r.seq)
          .map(|(i, _)| i)
        {
          self.records.remove(oldest);
        }
      }
      self.records.push(record);
    }
    self.changed = true;
  }

  fn clear(&mut self, id: &str) {
    let before = self.records.len();
    self.records.retain(|r| !r.under(id));
    if self.records.len() != before {
      self.changed = true;
    }
  }

  fn clear_root(&mut self) {
    let before = self.records.len();
    self.records.retain(|r| !r.id.is_empty());
    if self.records.len() != before {
      self.changed = true;
    }
  }

  fn drop_transient(&mut self) {
    let before = self.records.len();
    self.records.retain(|r| !r.state.transient());
    if self.records.len() != before {
      self.changed = true;
    }
  }
}

#[cfg(test)]
#[path = "../tests/status.rs"]
mod tests;
