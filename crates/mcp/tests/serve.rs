use super::*;

fn tools() -> Vec<Tool> {
    vec![Tool::new("echo", "echoes input", json!({ "type": "object" }))]
}

fn handle(name: &str, args: &Value) -> Result<Value, String> {
    match name {
        "echo" => Ok(args.clone()),
        _ => Err(format!("unknown tool {name}")),
    }
}

#[test]
fn initialize_advertises_protocol_and_tools_capability() {
    let msg = json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize" });
    let reply: Value = serde_json::from_str(
        &dispatch(&msg, &tools(), &json!({ "name": "prompt" }), &handle).unwrap(),
    )
    .unwrap();
    assert_eq!(reply["result"]["protocolVersion"], PROTOCOL_VERSION);
    assert!(reply["result"]["capabilities"]["tools"].is_object());
}

#[test]
fn notifications_get_no_reply() {
    let msg = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
    assert!(dispatch(&msg, &tools(), &json!({}), &handle).is_none());
}

#[test]
fn tools_list_returns_registered_tools() {
    let msg = json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" });
    let reply: Value =
        serde_json::from_str(&dispatch(&msg, &tools(), &json!({}), &handle).unwrap()).unwrap();
    let list = reply["result"]["tools"].as_array().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["name"], "echo");
}

#[test]
fn tools_call_wraps_handler_result_as_text_content() {
    let msg = json!({
        "jsonrpc": "2.0", "id": 3, "method": "tools/call",
        "params": { "name": "echo", "arguments": { "hi": 1 } }
    });
    let reply: Value =
        serde_json::from_str(&dispatch(&msg, &tools(), &json!({}), &handle).unwrap()).unwrap();
    assert_eq!(reply["result"]["isError"], false);
    let text = reply["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("\"hi\""));
}

#[test]
fn handler_error_is_a_failed_tool_call_not_a_protocol_error() {
    let msg = json!({
        "jsonrpc": "2.0", "id": 4, "method": "tools/call",
        "params": { "name": "missing" }
    });
    let reply: Value =
        serde_json::from_str(&dispatch(&msg, &tools(), &json!({}), &handle).unwrap()).unwrap();
    assert!(reply["result"]["isError"].as_bool().unwrap());
    assert!(reply.get("error").is_none());
}

#[test]
fn unknown_method_is_a_protocol_error() {
    let msg = json!({ "jsonrpc": "2.0", "id": 5, "method": "frobnicate" });
    let reply: Value =
        serde_json::from_str(&dispatch(&msg, &tools(), &json!({}), &handle).unwrap()).unwrap();
    assert_eq!(reply["error"]["code"], -32601);
}
