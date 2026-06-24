use super::{http, paths};
use anyhow::Result;
use serde_json::Value;

pub fn ps() -> Result<()> {
    let info = paths::read_info()?;
    let resp = http::get(&info.addr, "/control/state")?;
    let v: Value = serde_json::from_str(&resp)?;

    println!("agents:");
    match v["agents"].as_array() {
        Some(a) if !a.is_empty() => {
            for ag in a {
                println!(
                    "  {} {:<14} {:<16} ch:{}",
                    if ag["online"].as_bool().unwrap_or(false) { "●" } else { "○" },
                    ag["name"].as_str().unwrap_or("?"),
                    ag["role"].as_str().unwrap_or(""),
                    ag["channels"].as_i64().unwrap_or(0),
                );
            }
        }
        _ => println!("  (none)"),
    }

    println!("workers:");
    match v["workers"].as_array() {
        Some(w) if !w.is_empty() => {
            for wk in w {
                println!(
                    "  {:<14} {:<16} pid:{} restarts:{}",
                    wk["name"].as_str().unwrap_or("?"),
                    wk["status"].as_str().unwrap_or("?"),
                    wk["pid"].as_i64().unwrap_or(0),
                    wk["restarts"].as_i64().unwrap_or(0),
                );
            }
        }
        _ => println!("  (none)"),
    }
    Ok(())
}

pub fn kill(name: &str) -> Result<()> {
    let info = paths::read_info()?;
    let body = serde_json::json!({ "name": name }).to_string();
    let resp = http::post(&info.addr, "/control/stop", &body)?;
    let v: Value = serde_json::from_str(&resp).unwrap_or_default();
    if v["ok"].as_bool().unwrap_or(false) {
        println!("stopping '{name}'");
    } else {
        println!("no worker named '{name}'");
    }
    Ok(())
}
