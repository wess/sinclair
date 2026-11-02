//! `sinclair ls | send | run | capture | focus | status | wait | events`: the
//! running terminal, driven from a shell or a script.
//!
//! Every command talks to the live GUI over the single-instance socket (see
//! [`crate::ipc`]). A pane is named with `-p` (see [`crate::target`]); with no
//! `-p`, a command run inside a Sinclair pane acts on that pane, and one run
//! elsewhere acts on the focused pane.
//!
//! Exit status: 0 success, 1 failure (including a `wait` that timed out),
//! 2 bad command line, 3 target not found or ambiguous, 4 Sinclair not
//! reachable. `wait --exit` and `wait --command` exit with the waited-on
//! program's own status when it reported one.

pub mod wait;

use std::time::Duration;

use serde_json::{json, Value};

pub const OK: i32 = 0;
pub const FAILURE: i32 = 1;
pub const USAGE: i32 = 2;
pub const TARGET: i32 = 3;
pub const UNREACHABLE: i32 = 4;

/// The subcommands this module owns.
pub const COMMANDS: &[&str] = &[
  "ls", "send", "run", "capture", "focus", "zoom", "status", "wait", "events", "sessions",
];

/// A failed command: its exit status and what to print.
#[derive(Debug, PartialEq)]
pub struct Failure {
  pub code: i32,
  pub message: String,
}

impl Failure {
  pub fn usage(message: impl Into<String>) -> Failure {
    Failure {
      code: USAGE,
      message: message.into(),
    }
  }

  /// Classify an error string from the socket client or the server.
  pub fn from_request(err: String) -> Failure {
    let code = if err.starts_with(crate::target::NOT_FOUND) || err.starts_with(crate::target::AMBIGUOUS) {
      TARGET
    } else if err.starts_with("no running sinclair instance")
      || err.contains("MCP server is disabled")
      || err.starts_with("no active terminal window")
    {
      UNREACHABLE
    } else {
      FAILURE
    };
    Failure { code, message: err }
  }
}

/// Parsed common flags plus the remaining positional words.
#[derive(Debug, Default, PartialEq)]
pub struct Flags {
  pub pane: Option<String>,
  pub json: bool,
  pub follow: bool,
  pub timeout: Option<Duration>,
  pub lines: Option<u64>,
  pub exit: bool,
  pub wait: bool,
  pub split: Option<String>,
  pub tab: bool,
  pub keep_open: bool,
  pub on: Option<bool>,
  pub kill: Option<String>,
  pub command: bool,
  pub text: Option<String>,
  pub status: Option<String>,
  pub words: Vec<String>,
}

/// Parse flags. Anything after `--` is positional.
pub fn parse(args: &[String]) -> Result<Flags, Failure> {
  let mut f = Flags::default();
  let mut it = args.iter();
  let mut rest = false;
  while let Some(a) = it.next() {
    if rest {
      f.words.push(a.clone());
      continue;
    }
    let mut value = |name: &str| {
      it.next()
        .cloned()
        .ok_or_else(|| Failure::usage(format!("{name} needs a value")))
    };
    match a.as_str() {
      "--" => rest = true,
      "-p" | "--pane" => f.pane = Some(value(a)?),
      "--json" => f.json = true,
      "-f" | "--follow" => f.follow = true,
      "--timeout" | "--for" => {
        let v = value(a)?;
        f.timeout = Some(duration(&v).ok_or_else(|| Failure::usage(format!("bad duration `{v}`")))?);
      }
      "-n" | "--lines" => {
        let v = value(a)?;
        f.lines = Some(v.parse().map_err(|_| Failure::usage(format!("bad line count `{v}`")))?);
      }
      "--exit" => f.exit = true,
      "--wait" => f.wait = true,
      "--split" => f.split = Some(value(a)?),
      "--tab" => f.tab = true,
      "--keep-open" => f.keep_open = true,
      "--on" => f.on = Some(true),
      "--off" => f.on = Some(false),
      "--kill" => f.kill = Some(value(a)?),
      "--command" => f.command = true,
      "--text" => f.text = Some(value(a)?),
      "--status" => f.status = Some(value(a)?),
      s if s.starts_with('-') && s.len() > 1 => {
        return Err(Failure::usage(format!("unknown flag `{s}`")))
      }
      _ => f.words.push(a.clone()),
    }
  }
  Ok(f)
}

/// `90`, `90s`, `500ms`, `10m`, `2h`.
pub fn duration(s: &str) -> Option<Duration> {
  let s = s.trim();
  let split = s.find(|c: char| !c.is_ascii_digit() && c != '.').unwrap_or(s.len());
  let (n, unit) = s.split_at(split);
  let n: f64 = n.parse().ok()?;
  let secs = match unit {
    "" | "s" => n,
    "ms" => n / 1000.0,
    "m" => n * 60.0,
    "h" => n * 3600.0,
    _ => return None,
  };
  (secs >= 0.0 && secs.is_finite()).then(|| Duration::from_secs_f64(secs))
}

/// The pane a command acts on: `-p`, else the calling pane, else none (the
/// focused pane).
pub fn default_pane(flag: Option<String>) -> Option<String> {
  flag.or_else(|| {
    std::env::var("SINCLAIR_PANE")
      .ok()
      .filter(|t| t.parse::<u64>().is_ok())
      .map(|t| crate::target::id(t.parse().unwrap_or(0)))
  })
}

/// Arguments carrying the target, when there is one.
pub fn with_pane(mut args: Value, pane: &Option<String>) -> Value {
  if let Some(p) = pane {
    args["pane"] = json!(p);
  }
  args
}

/// Entry point for one subcommand. Returns the process exit status.
pub fn main(command: &str, args: &[String]) -> i32 {
  match run(command, args) {
    Ok(()) => OK,
    Err(f) => {
      if !f.message.is_empty() {
        eprintln!("sinclair {command}: {}", f.message);
      }
      f.code
    }
  }
}

fn run(command: &str, args: &[String]) -> Result<(), Failure> {
  let f = parse(args)?;
  let pane = default_pane(f.pane.clone());
  let call = |op: &str, args: Value| crate::ipc::request(op, &args).map_err(Failure::from_request);
  match command {
    "ls" => {
      let out = call("ls", json!({}))?;
      if f.json {
        println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
      } else {
        print!("{}", ls_table(&out));
      }
    }
    "send" | "run" => {
      let text = f.words.join(" ");
      if text.is_empty() {
        return Err(Failure::usage(format!("usage: sinclair {command} [-p PANE] TEXT")));
      }
      let text = if command == "send" { unescape(&text) } else { text };
      let op = if command == "send" { "send_input" } else { "run_command" };
      // `run --split right|down` / `run --tab` start the command in a new
      // pane, print its id, and (with --wait) wait on that pane.
      if command == "run" && (f.split.is_some() || f.tab) {
        let target = match (f.split.as_deref(), f.tab) {
          (Some("right"), _) => "split_right",
          (Some("down"), _) => "split_down",
          (Some(other), _) => return Err(Failure::usage(format!("--split takes right or down, not `{other}`"))),
          (None, _) => "tab",
        };
        let cursor = call("events", json!({}))?["next"].as_u64().unwrap_or(0);
        let out = call(
          "run_command",
          json!({ "text": text, "target": target, "keep_open": f.keep_open }),
        )?;
        let id = out["pane"].as_str().unwrap_or_default().to_string();
        if !f.wait {
          println!("{id}");
          return Ok(());
        }
        let outcome = wait::wait(&wait::Until::Exit, Some(&id), f.timeout, Some(cursor), &call)?;
        if f.json {
          println!("{}", outcome.json);
        }
        return match outcome.exit_code {
          Some(code) if code != 0 => Err(Failure { code, message: String::new() }),
          _ => Ok(()),
        };
      }
      // `run --wait`: note where the journal stands before typing, so even a
      // command that finishes at once is seen, then exit with its status.
      let cursor = if f.wait && command == "run" {
        Some(call("events", json!({}))?["next"].as_u64().unwrap_or(0))
      } else {
        None
      };
      call(op, with_pane(json!({ "text": text }), &pane))?;
      if cursor.is_some() {
        let outcome = wait::wait(&wait::Until::Command, pane.as_deref(), f.timeout, cursor, &call)?;
        if f.json {
          println!("{}", outcome.json);
        }
        if let Some(code) = outcome.exit_code.filter(|&c| c != 0) {
          return Err(Failure {
            code,
            message: String::new(),
          });
        }
      }
    }
    "capture" => {
      let mut args = json!({});
      if let Some(n) = f.lines {
        args["lines"] = json!(n);
      }
      let out = call("read_screen", with_pane(args, &pane))?;
      print!("{}", out["text"].as_str().unwrap_or_default());
    }
    "focus" => {
      let p = f.pane.or_else(|| f.words.first().cloned()).ok_or_else(|| Failure::usage("usage: sinclair focus PANE"))?;
      call("focus", json!({ "pane": p }))?;
    }
    "zoom" => {
      let p = f.pane.clone().or_else(|| f.words.first().cloned());
      let mut args = with_pane(json!({}), &p.or(pane));
      if let Some(on) = f.on {
        args["on"] = json!(on);
      }
      let out = call("zoom", args)?;
      if f.json {
        println!("{out}");
      }
    }
    // The session server's own view, straight from its socket: what is
    // running there whether or not any window shows it.
    "sessions" => sessions(&f)?,
    "status" => {
      let out = call("status", with_pane(json!({}), &pane))?;
      if f.json {
        println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
      } else {
        println!("{}", out["signal"].as_str().unwrap_or("none"));
      }
    }
    "wait" => {
      let until = wait::Until::from_flags(&f)?;
      let outcome = wait::wait(&until, pane.as_deref(), f.timeout, None, &call)?;
      if f.json {
        println!("{}", outcome.json);
      }
      if let Some(code) = outcome.exit_code {
        if code != 0 {
          return Err(Failure {
            code,
            message: String::new(),
          });
        }
      }
    }
    "events" => {
      // Only a pane named explicitly filters the stream: following
      // "everything" is the common case.
      wait::follow(f.pane.as_deref(), f.follow, f.timeout, f.json, &call)?;
    }
    other => return Err(Failure::usage(format!("unknown command `{other}`"))),
  }
  Ok(())
}

#[cfg(unix)]
fn sessions(f: &Flags) -> Result<(), Failure> {
  let path = crate::sessiond::socket();
  if let Some(id) = &f.kill {
    return terminal::mux::client::kill(&path, id).map_err(|e| Failure {
      code: if e.to_string().contains("no such session") { TARGET } else { FAILURE },
      message: e.to_string(),
    });
  }
  let listed = terminal::mux::client::list(&path).map_err(|_| Failure {
    code: UNREACHABLE,
    message: "no session server is running".into(),
  })?;
  if f.json {
    let rows: Vec<Value> = listed
      .iter()
      .map(|s| json!({ "id": s.id, "pane": crate::target::id(s.token), "title": s.title, "attached": s.clients }))
      .collect();
    println!("{}", serde_json::to_string_pretty(&json!({ "sessions": rows })).unwrap_or_default());
  } else {
    for s in &listed {
      let state = if s.clients > 0 { "attached" } else { "detached" };
      println!("{}  {:<9}  {:<8}  {}", s.id, crate::target::id(s.token), state, s.title);
    }
  }
  Ok(())
}

#[cfg(not(unix))]
fn sessions(_f: &Flags) -> Result<(), Failure> {
  Err(Failure {
    code: UNREACHABLE,
    message: "persistent sessions are not available on this platform".into(),
  })
}

/// `\n`, `\r`, `\t`, `\e`, `\\`, and `\xNN` in `send` text, so keys like
/// Enter or Ctrl-C can be sent: `sinclair send 'q\r'`, `sinclair send '\x03'`.
pub fn unescape(s: &str) -> String {
  let mut out = String::with_capacity(s.len());
  let mut chars = s.chars().peekable();
  while let Some(c) = chars.next() {
    if c != '\\' {
      out.push(c);
      continue;
    }
    match chars.next() {
      Some('n') => out.push('\n'),
      Some('r') => out.push('\r'),
      Some('t') => out.push('\t'),
      Some('e') => out.push('\x1b'),
      Some('\\') => out.push('\\'),
      Some('x') => {
        let hex: String = chars.by_ref().take(2).collect();
        match u8::from_str_radix(&hex, 16) {
          Ok(b) if b < 0x80 => out.push(b as char),
          _ => {
            out.push_str("\\x");
            out.push_str(&hex);
          }
        }
      }
      Some(other) => {
        out.push('\\');
        out.push(other);
      }
      None => out.push('\\'),
    }
  }
  out
}

/// `sinclair ls` as aligned text.
pub fn ls_table(out: &Value) -> String {
  let rows: Vec<[String; 4]> = out["panes"]
    .as_array()
    .map(|a| a.as_slice())
    .unwrap_or_default()
    .iter()
    .map(|p| {
      [
        format!(
          "{}{}",
          if p["active"].as_bool() == Some(true) { "*" } else { " " },
          p["index"]
        ),
        p["id"].as_str().unwrap_or_default().to_string(),
        p["signal"].as_str().unwrap_or("-").to_string(),
        p["title"].as_str().unwrap_or_default().to_string(),
      ]
    })
    .collect();
  let width = |i: usize| rows.iter().map(|r| r[i].chars().count()).max().unwrap_or(0);
  let (w0, w1, w2) = (width(0), width(1), width(2));
  rows
    .iter()
    .map(|r| format!("{:<w0$}  {:<w1$}  {:<w2$}  {}\n", r[0], r[1], r[2], r[3]))
    .collect()
}

#[cfg(test)]
#[path = "../../tests/cli.rs"]
mod tests;
