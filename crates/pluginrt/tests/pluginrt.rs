use super::*;
use std::sync::{Arc, Mutex};

/// A mock app host that records the commands a plugin runs and serves a canned
/// screen to `read_screen`.
struct MockHost {
    commands: Arc<Mutex<Vec<String>>>,
    screen: String,
    /// Programs `exec` was asked to run, as `program arg arg`.
    execs: Arc<Mutex<Vec<String>>>,
}

impl MockHost {
    fn new(screen: &str) -> Self {
        Self {
            commands: Arc::new(Mutex::new(Vec::new())),
            screen: screen.to_string(),
            execs: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl AppHost for MockHost {
    fn log(&mut self, _level: LogLevel, _message: String) {}
    fn storage_get(&mut self, _key: String) -> Option<String> {
        None
    }
    fn storage_set(&mut self, _key: String, _value: String) {}
    fn run_command(&mut self, text: String, _target: CommandTarget) -> Result<(), String> {
        self.commands.lock().unwrap().push(text);
        Ok(())
    }
    fn send_input(&mut self, _bytes: Vec<u8>) -> Result<(), String> {
        Ok(())
    }
    fn read_screen(&mut self, _lines: u32) -> Result<String, String> {
        Ok(self.screen.clone())
    }
    fn selection(&mut self) -> Option<String> {
        None
    }
    fn fetch(&mut self, _request: HttpRequest) -> Result<HttpResponse, String> {
        Err("no network".into())
    }
    fn read_file(&mut self, _path: String) -> Result<Vec<u8>, String> {
        Err("no fs".into())
    }
    fn write_file(&mut self, _path: String, _data: Vec<u8>) -> Result<(), String> {
        Err("no fs".into())
    }
    fn clipboard_read(&mut self) -> Result<String, String> {
        Err("no clipboard".into())
    }
    fn clipboard_write(&mut self, _text: String) -> Result<(), String> {
        Ok(())
    }
    fn notify(&mut self, _title: String, _body: String) {}
    fn exec(&mut self, request: ExecRequest) -> Result<ExecOutput, String> {
        let mut line = request.program.clone();
        for a in &request.args {
            line.push(' ');
            line.push_str(a);
        }
        self.execs.lock().unwrap().push(line);
        Ok(ExecOutput {
            status: 0,
            stdout: "ok\n".to_string(),
            stderr: String::new(),
        })
    }
}

fn fixture() -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/example.wasm");
    std::fs::read(path).expect("example.wasm fixture")
}

#[test]
fn builds_a_component_engine() {
    engine().expect("wasmtime component engine");
}

#[test]
fn tool_call_and_gated_host_call() {
    let eng = engine().unwrap();
    let host = MockHost::new("");
    let commands = host.commands.clone();
    let execs = host.execs.clone();
    let mut plugin = PluginInstance::new(&eng, &fixture(), &granted(), Box::new(host))
        .expect("instantiate with the declared capabilities");

    // A pure tool round-trips its params.
    let echoed = plugin.call_tool("echo", "{\"a\":1}").unwrap().unwrap();
    assert_eq!(echoed, "{\"a\":1}");

    // A tool that calls the gated host-commands interface reaches the host.
    let ran = plugin.call_tool("run", "{}").unwrap().unwrap();
    assert_eq!(ran, "{\"ran\":true}");
    assert_eq!(commands.lock().unwrap().as_slice(), &["echo hi".to_string()]);

    // A tool that calls the gated `host-process` interface reaches the host, and
    // the host's output comes back through.
    let ran = plugin.call_tool("exec", "{}").unwrap().unwrap();
    assert!(ran.contains("\"status\":0"), "{ran}");
    assert!(ran.contains("ok"), "{ran}");
    assert_eq!(
        execs.lock().unwrap().as_slice(),
        &["git status --porcelain".to_string()]
    );

    // An unknown tool returns the guest's error, not a trap.
    let err = plugin.call_tool("nope", "{}").unwrap().unwrap_err();
    assert!(err.contains("unknown tool"), "{err}");
}

/// Every capability the test fixture's world imports.
fn granted() -> Vec<String> {
    vec!["commands".to_string(), "process".to_string()]
}

#[test]
fn missing_capability_blocks_instantiation() {
    let eng = engine().unwrap();
    let host = Box::new(MockHost::new(""));
    // The guest imports host-commands; without the `commands` capability the host
    // doesn't link it, so the component can't instantiate. That is the enforced
    // capability boundary — not an advisory flag.
    let result = PluginInstance::new(&eng, &fixture(), &[], host);
    assert!(result.is_err(), "instantiation must fail without the commands capability");
}

/// `process` is gated on its own, not carried in by another capability: the same
/// guest that instantiates with `commands` + `process` is refused when only
/// `commands` is granted.
#[test]
fn process_capability_is_gated_independently() {
    let eng = engine().unwrap();
    let host = Box::new(MockHost::new(""));
    let result = PluginInstance::new(&eng, &fixture(), &["commands".to_string()], host);
    assert!(
        result.is_err(),
        "a guest importing host-process must not instantiate without the process capability"
    );
}

/// The JS SDK's component (built via componentize-js) loads and runs the same
/// way a Rust one does. Skipped when `sdk/js/plugin.wasm` isn't built (CI),
/// since the 12 MB artifact isn't committed.
#[test]
fn js_component_loads_and_runs_if_built() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../sdk/js/plugin.wasm");
    if !path.exists() {
        eprintln!("skipping: sdk/js/plugin.wasm not built");
        return;
    }
    let wasm = std::fs::read(path).unwrap();
    let eng = engine().unwrap();
    let host = Box::new(MockHost::new("one two three\n"));
    let mut plugin = PluginInstance::new(&eng, &wasm, &["screen".to_string()], host)
        .expect("instantiate the JS component");
    let out = plugin.call_tool("wordcount", "{}").unwrap().unwrap();
    let value: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(value["words"], 3, "{out}");
}

/// An infinite-loop tool traps on fuel exhaustion instead of hanging the host.
#[test]
fn runaway_guest_is_fuel_bounded() {
    let eng = engine().unwrap();
    let host = Box::new(MockHost::new(""));
    let mut plugin =
        PluginInstance::new(&eng, &fixture(), &granted(), host).unwrap();
    plugin.set_fuel_budget(50_000_000); // small budget so the test is fast
    let result = plugin.call_tool("spin", "{}");
    assert!(result.is_err(), "an infinite-loop tool must trap, not hang");
}

/// The shipped bundled `screentools` plugin actually loads and runs: it reads the
/// screen through the gated host-screen interface and greps it.
#[test]
fn bundled_screentools_greps_the_screen() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../plugins/screentools/plugin.wasm");
    let wasm = std::fs::read(path).expect("screentools plugin.wasm");
    let eng = engine().unwrap();
    let host = Box::new(MockHost::new("alpha\nbeta error\ngamma\ndelta error\n"));
    let mut plugin = PluginInstance::new(&eng, &wasm, &["screen".to_string()], host)
        .expect("instantiate screentools");
    let out = plugin.call_tool("grep", "{\"query\":\"error\"}").unwrap().unwrap();
    let value: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(value["count"], 2, "{out}");
    assert_eq!(value["matches"][0], "beta error");

    // Its panel renders a node tree (the v2 render path).
    let tree = plugin.render("{}").unwrap();
    let node: serde_json::Value = serde_json::from_str(&tree).unwrap();
    assert_eq!(node["title"], "Screen Tools");
    assert_eq!(node["blocks"][0]["type"], "section");
    // A UI event is accepted (no-op here).
    plugin.on_ui_event("{\"id\":\"x\"}").unwrap();
}
