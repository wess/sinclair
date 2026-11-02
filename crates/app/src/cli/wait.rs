//! Waiting on a pane, and following the event stream, by polling the GUI's
//! event journal (see [`crate::journal`] for why it is polled). Shared by the
//! CLI and the MCP `wait`/`events` tools, which run in the `sinclair mcp`
//! process rather than the GUI, so a long wait never holds the socket.

use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::{Failure, Flags};

/// How often to poll.
const POLL: Duration = Duration::from_millis(200);

/// What to wait for.
#[derive(Clone, Debug, PartialEq)]
pub enum Until {
  /// The pane's process exits (or the pane closes).
  Exit,
  /// The next command finishes (shell integration, OSC 133 `D`).
  Command,
  /// The pane's text matches this regex.
  Text(String),
  /// The pane's status headline is one of these states.
  Status(Vec<String>),
}

impl Until {
  pub fn from_flags(f: &Flags) -> Result<Until, Failure> {
    let picked = [f.exit, f.command, f.text.is_some(), f.status.is_some()]
      .iter()
      .filter(|&&b| b)
      .count();
    if picked != 1 {
      return Err(Failure::usage(
        "usage: sinclair wait [-p PANE] (--exit | --command | --text REGEX | --status STATE[,STATE]) [--timeout DURATION] [--json]",
      ));
    }
    Ok(if f.exit {
      Until::Exit
    } else if f.command {
      Until::Command
    } else if let Some(t) = &f.text {
      Until::Text(t.clone())
    } else {
      Until::Status(
        f.status
          .as_deref()
          .unwrap_or_default()
          .split(',')
          .map(|s| s.trim().to_string())
          .filter(|s| !s.is_empty())
          .collect(),
      )
    })
  }

  /// From an MCP `wait` call's arguments.
  pub fn from_args(args: &Value) -> Result<Until, String> {
    let s = |k: &str| args.get(k).and_then(Value::as_str);
    match s("for").unwrap_or_default() {
      "exit" => Ok(Until::Exit),
      "command" => Ok(Until::Command),
      "text" => s("pattern")
        .map(|p| Until::Text(p.to_string()))
        .ok_or_else(|| "wait for text needs a `pattern`".to_string()),
      "status" => {
        let states: Vec<String> = s("state")
          .unwrap_or("done,error,blocked")
          .split(',')
          .map(|x| x.trim().to_string())
          .collect();
        Ok(Until::Status(states))
      }
      other => Err(format!("unknown wait condition `{other}` (exit|command|text|status)")),
    }
  }
}

/// How a wait ended.
#[derive(Debug)]
pub struct Outcome {
  /// The waited-on program's exit status, when it reported one.
  pub exit_code: Option<i32>,
  /// A JSON description of what happened.
  pub json: Value,
}

type Call<'a> = dyn Fn(&str, Value) -> Result<Value, Failure> + 'a;

/// Wait on `pane` (focused when `None`) until `until` holds or `timeout`
/// passes.
///
/// `cursor` is the journal position to watch from; `None` means now. A caller
/// that starts something and then waits for it passes the position from
/// *before* it started, so a fast finish isn't missed.
pub fn wait(
  until: &Until,
  pane: Option<&str>,
  timeout: Option<Duration>,
  cursor: Option<u64>,
  call: &Call,
) -> Result<Outcome, Failure> {
  let start = Instant::now();
  // Pin the pane down to its id first, so a wait on "the focused pane" or a
  // title keeps watching the same pane even if focus or titles change.
  let status = call("status", super::with_pane(json!({}), &pane.map(str::to_string)))?;
  let id = status["id"].as_str().unwrap_or_default().to_string();
  let target = Some(id.clone());
  let regex = match until {
    Until::Text(p) => Some(regex::Regex::new(p).map_err(|e| Failure::usage(format!("bad regex: {e}")))?),
    _ => None,
  };
  let mut cursor = match cursor {
    Some(c) => c,
    None => call("events", json!({}))?["next"].as_u64().unwrap_or(0),
  };
  loop {
    match until {
      Until::Exit | Until::Command => {
        // Read the whole journal and pick this pane's events out here: the
        // pane may already be gone (its process ended and it closed), but
        // its final events are still in the journal.
        let out = call("events", json!({ "after": cursor }))?;
        cursor = out["next"].as_u64().unwrap_or(cursor);
        let hit = out["events"].as_array().and_then(|evs| {
          evs
            .iter()
            .filter(|e| e["pane"] == id.as_str())
            .find(|e| e["event"] == "exit" || (*until == Until::Command && e["event"] == "command_finished"))
            .cloned()
        });
        if let Some(ev) = hit {
          let code = ev["data"]["exit_code"].as_i64().map(|c| c as i32);
          return Ok(Outcome {
            exit_code: code,
            json: json!({ "pane": id, "event": ev["event"], "exit_code": code }),
          });
        }
        // Closed without an exit event reaching the journal: still an end.
        if let Err(f) = call("status", json!({ "pane": id })) {
          if f.code == super::TARGET {
            return Ok(Outcome {
              exit_code: None,
              json: json!({ "pane": id, "event": "closed" }),
            });
          }
          return Err(f);
        }
      }
      Until::Text(_) => {
        let out = call("read_screen", super::with_pane(json!({}), &target))?;
        let text = out["text"].as_str().unwrap_or_default();
        if let Some(m) = regex.as_ref().and_then(|r| r.find(text)) {
          return Ok(Outcome {
            exit_code: None,
            json: json!({ "pane": id, "event": "text", "match": m.as_str() }),
          });
        }
      }
      Until::Status(states) => {
        let out = call("status", super::with_pane(json!({}), &target))?;
        let state = out["state"].as_str().unwrap_or("none");
        if states.iter().any(|s| s == state) {
          return Ok(Outcome {
            exit_code: None,
            json: json!({ "pane": id, "event": "status", "status": out }),
          });
        }
      }
    }
    if timeout.is_some_and(|t| start.elapsed() >= t) {
      return Err(Failure {
        code: super::FAILURE,
        message: format!("timed out after {:?}", start.elapsed()),
      });
    }
    std::thread::sleep(POLL);
  }
}

/// Print events as they happen: everything (or one pane's) from now on, until
/// `duration` passes; with `follow`, until interrupted. Without either, print
/// nothing new and return — there is no history to show from a fresh cursor,
/// so the useful forms are `--follow` and `--for`.
pub fn follow(
  pane: Option<&str>,
  follow: bool,
  duration: Option<Duration>,
  json_out: bool,
  call: &Call,
) -> Result<(), Failure> {
  if !follow && duration.is_none() {
    return Err(Failure::usage(
      "usage: sinclair events [-p PANE] (--follow | --for DURATION) [--json]",
    ));
  }
  let pane = pane.map(str::to_string);
  let start = Instant::now();
  let mut cursor = call("events", json!({}))?["next"].as_u64().unwrap_or(0);
  loop {
    let out = call("events", super::with_pane(json!({ "after": cursor }), &pane))?;
    cursor = out["next"].as_u64().unwrap_or(cursor);
    if out["lost"].as_bool() == Some(true) {
      eprintln!("sinclair events: fell behind; some events were dropped");
    }
    for ev in out["events"].as_array().map(|a| a.as_slice()).unwrap_or_default() {
      if json_out {
        println!("{ev}");
      } else {
        println!("{}", line(ev));
      }
    }
    if duration.is_some_and(|d| start.elapsed() >= d) {
      return Ok(());
    }
    std::thread::sleep(POLL);
  }
}

/// One event as a human-readable line.
pub fn line(ev: &Value) -> String {
  let data = &ev["data"];
  let detail = match ev["event"].as_str().unwrap_or_default() {
    "exit" | "command_finished" => format!("exit_code={}", data["exit_code"]),
    "title_changed" => data["title"].as_str().unwrap_or_default().to_string(),
    "dir_changed" => data["dir"].as_str().unwrap_or_default().to_string(),
    "notify" => data["body"].as_str().unwrap_or_default().to_string(),
    "status_changed" => data["status"]["state"].as_str().unwrap_or("cleared").to_string(),
    _ => String::new(),
  };
  format!(
    "{} {} {}",
    ev["pane"].as_str().unwrap_or_default(),
    ev["event"].as_str().unwrap_or_default(),
    detail
  )
  .trim_end()
  .to_string()
}
