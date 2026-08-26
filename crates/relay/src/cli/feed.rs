use super::{http, paths};
use anyhow::Result;
use serde_json::Value;
use std::io::BufRead;

/// Print the message bus. `--follow` holds an SSE stream open on
/// `/control/feed/live` and prints batches as the server pushes them — no
/// polling; an idle mesh costs nothing. Exits when the server goes away, like
/// the one-shot path does.
pub fn feed(follow: bool) -> Result<()> {
  let info = paths::read_info()?;
  if !follow {
    let resp = http::get(&info.addr, "/control/feed?since=0")?;
    let v: Value = serde_json::from_str(&resp)?;
    print_batch(&v);
    return Ok(());
  }

  let mut reader = http::open_stream(&info.addr, "/control/feed/live?since=0")?;
  let mut line = String::new();
  loop {
    line.clear();
    if reader.read_line(&mut line)? == 0 {
      return Ok(());
    }
    if let Some(rest) = line.strip_prefix("data:") {
      let payload = rest.trim();
      if payload.is_empty() {
        continue;
      }
      if let Ok(v) = serde_json::from_str::<Value>(payload) {
        print_batch(&v);
      }
    }
  }
}

fn print_batch(v: &Value) {
  if let Some(arr) = v["messages"].as_array() {
    for m in arr {
      print_msg(m);
    }
  }
}

fn print_msg(m: &Value) {
  let kind = m["kind"].as_str().unwrap_or("");
  let target = m["target"].as_str().unwrap_or("");
  let arrow = match kind {
    "direct" => format!("→{target}"),
    "channel" => format!("#{target}"),
    _ => "*all".to_string(),
  };
  println!(
    "{}  {:<12} {:<12} {}",
    hms(m["created"].as_i64().unwrap_or(0)),
    m["sender"].as_str().unwrap_or("?"),
    arrow,
    m["body"].as_str().unwrap_or(""),
  );
}

fn hms(epoch: i64) -> String {
  let s = epoch.rem_euclid(86400);
  format!("{:02}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
}
