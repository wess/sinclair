use super::*;
use serde_json::json;

#[test]
fn install_adds_all_events() {
    let out = install_into(json!({}), "/bin/sinclair");
    let hooks = out.get("hooks").unwrap().as_object().unwrap();
    assert_eq!(hooks.len(), HOOK_EVENTS.len());
    for (event, state) in HOOK_EVENTS {
        let arr = hooks.get(*event).unwrap().as_array().unwrap();
        let cmd = arr[0]["hooks"][0]["command"].as_str().unwrap();
        assert_eq!(cmd, format!("/bin/sinclair agent-status {state}"));
    }
}

#[test]
fn install_is_idempotent() {
    let once = install_into(json!({}), "/bin/sinclair");
    let twice = install_into(once.clone(), "/bin/sinclair");
    assert_eq!(once, twice, "installing twice must not duplicate entries");
    // Each event still has exactly one entry.
    for (event, _) in HOOK_EVENTS {
        let arr = twice["hooks"][event].as_array().unwrap();
        assert_eq!(arr.len(), 1);
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
    let out = install_into(existing, "/bin/sinclair");
    assert_eq!(out["model"], json!("opus"));
    // The foreign Stop hook survives; ours is appended alongside it.
    let stop = out["hooks"]["Stop"].as_array().unwrap();
    assert_eq!(stop.len(), 2);
    assert_eq!(stop[0]["hooks"][0]["command"], json!("echo bye"));
    assert!(stop[1]["hooks"][0]["command"].as_str().unwrap().contains("agent-status"));
}

#[test]
fn uninstall_removes_only_ours() {
    let installed = install_into(
        json!({
            "hooks": {
                "Stop": [ { "hooks": [ { "type": "command", "command": "echo bye" } ] } ]
            }
        }),
        "/bin/sinclair",
    );
    let cleaned = uninstall_from(installed);
    // The foreign Stop hook remains; our events are gone.
    let stop = cleaned["hooks"]["Stop"].as_array().unwrap();
    assert_eq!(stop.len(), 1);
    assert_eq!(stop[0]["hooks"][0]["command"], json!("echo bye"));
    assert!(cleaned["hooks"].get("UserPromptSubmit").is_none());
    assert!(cleaned["hooks"].get("SessionStart").is_none());
}

#[test]
fn uninstall_drops_empty_hooks_object() {
    let installed = install_into(json!({ "model": "opus" }), "/bin/sinclair");
    let cleaned = uninstall_from(installed);
    assert!(cleaned.get("hooks").is_none(), "empty hooks object removed");
    assert_eq!(cleaned["model"], json!("opus"), "other settings preserved");
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
