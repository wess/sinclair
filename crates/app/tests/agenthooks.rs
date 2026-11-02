use super::*;
use serde_json::json;

fn claude(root: Value, exe: &str) -> Value {
  install_into(root, exe, CLAUDE_EVENTS)
}

fn codex(root: Value, exe: &str) -> Value {
  install_into(root, exe, CODEX_EVENTS)
}

#[test]
fn install_adds_all_events() {
  let out = claude(json!({}), "/bin/sinclair");
  let hooks = out.get("hooks").unwrap().as_object().unwrap();
  assert_eq!(hooks.len(), CLAUDE_EVENTS.len());
  for (event, state) in CLAUDE_EVENTS {
    let arr = hooks.get(*event).unwrap().as_array().unwrap();
    let cmd = arr[0]["hooks"][0]["command"].as_str().unwrap();
    assert_eq!(cmd, format!("'/bin/sinclair' agent-status {state}"));
  }
}

#[test]
fn codex_events_use_codex_names() {
  let out = codex(json!({}), "/bin/sinclair");
  let hooks = out["hooks"].as_object().unwrap();
  assert_eq!(hooks.len(), CODEX_EVENTS.len());
  assert!(hooks.get("Notification").is_none());
  assert_eq!(
    hooks["PermissionRequest"][0]["hooks"][0]["command"],
    json!("'/bin/sinclair' agent-status blocked")
  );
  assert_eq!(hooks["PermissionRequest"][0]["hooks"][0]["type"], json!("command"));
}

#[test]
fn install_quotes_an_exe_path_with_spaces() {
  let out = claude(json!({}), "/Applications/My Apps/sinclair");
  let cmd = out["hooks"]["Stop"][0]["hooks"][0]["command"]
    .as_str()
    .unwrap();
  assert_eq!(cmd, "'/Applications/My Apps/sinclair' agent-status done");
}

#[test]
fn install_is_idempotent() {
  let once = claude(json!({}), "/bin/sinclair");
  let twice = claude(once.clone(), "/bin/sinclair");
  assert_eq!(once, twice, "installing twice must not duplicate entries");
  for (event, _) in CLAUDE_EVENTS {
    assert_eq!(twice["hooks"][event].as_array().unwrap().len(), 1);
  }
}

#[test]
fn install_preserves_foreign_settings_and_hooks() {
  let existing = json!({
      "model": "opus",
      "hooks": {
          "Stop": [ { "hooks": [ { "type": "command", "command": "echo bye" } ] } ]
      }
  });
  let out = claude(existing, "/bin/sinclair");
  assert_eq!(out["model"], json!("opus"));
  // The foreign Stop hook keeps index 0; ours is appended after it.
  let stop = out["hooks"]["Stop"].as_array().unwrap();
  assert_eq!(stop.len(), 2);
  assert_eq!(stop[0]["hooks"][0]["command"], json!("echo bye"));
  assert!(stop[1]["hooks"][0]["command"]
    .as_str()
    .unwrap()
    .contains("agent-status"));
}

#[test]
fn install_keeps_our_position_and_updates_a_stale_command() {
  let old = codex(
    json!({
        "hooks": {
            "Stop": [ { "hooks": [ { "type": "command", "command": "echo first" } ] } ]
        }
    }),
    "/old/sinclair",
  );
  // A foreign group added after ours must not move us.
  let mut mixed = old.clone();
  mixed["hooks"]["Stop"]
    .as_array_mut()
    .unwrap()
    .push(json!({ "hooks": [ { "type": "command", "command": "echo last" } ] }));
  let out = codex(mixed, "/new/sinclair");
  let stop = out["hooks"]["Stop"].as_array().unwrap();
  assert_eq!(stop.len(), 3);
  assert_eq!(stop[0]["hooks"][0]["command"], json!("echo first"));
  assert_eq!(stop[1]["hooks"][0]["command"], json!("'/new/sinclair' agent-status done"));
  assert_eq!(stop[2]["hooks"][0]["command"], json!("echo last"));
}

#[test]
fn codex_install_adds_only_hooks_key_to_an_empty_file() {
  let out = codex(json!({}), "/bin/sinclair");
  assert_eq!(out.as_object().unwrap().len(), 1, "Codex rejects unknown top-level keys");
  let kept = codex(json!({ "description": "mine" }), "/bin/sinclair");
  assert_eq!(kept["description"], json!("mine"));
}

#[test]
fn uninstall_removes_only_ours() {
  let installed = claude(
    json!({
        "hooks": {
            "Stop": [ { "hooks": [ { "type": "command", "command": "echo bye" } ] } ]
        }
    }),
    "/bin/sinclair",
  );
  let cleaned = uninstall_from(installed);
  let stop = cleaned["hooks"]["Stop"].as_array().unwrap();
  assert_eq!(stop.len(), 1);
  assert_eq!(stop[0]["hooks"][0]["command"], json!("echo bye"));
  assert!(cleaned["hooks"].get("UserPromptSubmit").is_none());
  assert!(cleaned["hooks"].get("SessionStart").is_none());
}

#[test]
fn uninstall_keeps_foreign_handlers_in_our_group() {
  let root = json!({
      "hooks": { "Stop": [ { "matcher": "x", "hooks": [
          { "type": "command", "command": "echo keep" },
          { "type": "command", "command": "'/bin/sinclair' agent-status done" }
      ] } ] }
  });
  let cleaned = uninstall_from(root);
  let hs = cleaned["hooks"]["Stop"][0]["hooks"].as_array().unwrap();
  assert_eq!(hs.len(), 1);
  assert_eq!(hs[0]["command"], json!("echo keep"));
}

#[test]
fn uninstall_drops_empty_hooks_object() {
  let installed = codex(json!({ "description": "d" }), "/bin/sinclair");
  let cleaned = uninstall_from(installed);
  assert!(cleaned.get("hooks").is_none(), "empty hooks object removed");
  assert_eq!(cleaned["description"], json!("d"));
}

#[test]
fn status_reports_missing_outdated_current() {
  let root = json!({});
  assert_eq!(status_of(&root, "/bin/sinclair", CODEX_EVENTS), Status::Missing);
  let installed = codex(root, "/bin/sinclair");
  assert_eq!(status_of(&installed, "/bin/sinclair", CODEX_EVENTS), Status::Current);
  assert_eq!(status_of(&installed, "/moved/sinclair", CODEX_EVENTS), Status::Outdated);
  // Claude's events are a different set: a Claude-shaped install is incomplete for Codex.
  let partial = claude(json!({}), "/bin/sinclair");
  assert_eq!(status_of(&partial, "/bin/sinclair", CODEX_EVENTS), Status::Outdated);
}

#[test]
fn read_config_and_roundtrip_on_disk() {
  let dir = std::env::temp_dir().join(format!("sinclair-hooks-{}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  let path = dir.join(".codex").join("hooks.json");
  assert_eq!(read_config(&path).unwrap(), json!({}));
  write_config(&path, &codex(json!({}), "/bin/sinclair")).unwrap();
  let back = read_config(&path).unwrap();
  assert_eq!(status_of(&back, "/bin/sinclair", CODEX_EVENTS), Status::Current);
  std::fs::write(&path, "not json").unwrap();
  assert!(read_config(&path).is_err(), "a corrupt file is not overwritten");
  std::fs::write(&path, "  \n").unwrap();
  assert_eq!(read_config(&path).unwrap(), json!({}));
  let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn session_context_shape() {
  let v = session_context(7);
  assert_eq!(v["hookSpecificOutput"]["hookEventName"], json!("SessionStart"));
  let text = v["hookSpecificOutput"]["additionalContext"].as_str().unwrap();
  assert!(text.contains("SINCLAIR_PANE=7"));
  assert!(text.contains("run_command") && text.contains("agent_states"));
}

#[test]
fn payload_helpers_read_hook_json() {
  let v = json!({ "hook_event_name": "SessionStart", "session_id": "abc" });
  assert_eq!(payload_event(&v), Some("SessionStart"));
  assert_eq!(payload_session(&v), Some("abc".to_string()));
  assert_eq!(payload_event(&Value::Null), None);
}

#[test]
fn agent_parse_and_files() {
  assert_eq!(Agent::parse("codex"), Some(Agent::Codex));
  assert_eq!(Agent::parse("nope"), None);
  assert_eq!(Agent::Codex.file_name(), "hooks.json");
  assert_eq!(Agent::Claude.file_name(), "settings.json");
}

#[test]
fn flag_reads_value_pairs() {
  let args = vec![
    "working".to_string(),
    "--session".to_string(),
    "s1".to_string(),
  ];
  assert_eq!(flag(&args, "--session"), Some("s1".to_string()));
  assert_eq!(flag(&args, "--name"), None);
}
