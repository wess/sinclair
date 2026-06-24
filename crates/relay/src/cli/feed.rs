use super::{http, paths};
use anyhow::Result;
use serde_json::Value;
use std::time::Duration;

pub fn feed(follow: bool) -> Result<()> {
    let info = paths::read_info()?;
    let mut since = 0i64;
    loop {
        let resp = http::get(&info.addr, &format!("/control/feed?since={since}"))?;
        let v: Value = serde_json::from_str(&resp)?;
        if let Some(arr) = v["messages"].as_array() {
            for m in arr {
                print_msg(m);
            }
        }
        since = v["last"].as_i64().unwrap_or(since);
        if !follow {
            break;
        }
        std::thread::sleep(Duration::from_millis(600));
    }
    Ok(())
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
