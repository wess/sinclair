use super::*;
use serde_json::Value;
use std::sync::{Arc, Mutex};

/// A mock app host: records what a plugin asked for, serves a canned screen to
/// `read_screen`, and answers `exec` from a lookup table — enough to drive a
/// whole process-backed plugin without a real git or docker on the machine.
struct MockHost {
    commands: Arc<Mutex<Vec<String>>>,
    screen: String,
    /// Command lines `exec` was asked to run, as `program arg arg`.
    execs: Arc<Mutex<Vec<String>>>,
    /// Full command line ("git status --porcelain") -> stdout. An unlisted
    /// command *fails*, the way a missing binary or a down daemon would — so a
    /// test that forgets to stub something sees the failure path, not a
    /// silently-empty success.
    replies: Vec<(&'static str, &'static str)>,
}

impl MockHost {
    fn new(screen: &str) -> Self {
        Self {
            commands: Arc::new(Mutex::new(Vec::new())),
            screen: screen.to_string(),
            execs: Arc::new(Mutex::new(Vec::new())),
            replies: Vec::new(),
        }
    }

    fn replying(mut self, replies: &[(&'static str, &'static str)]) -> Self {
        self.replies = replies.to_vec();
        self
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
        let args = request.args.join(" ");
        let mut line = request.program.clone();
        if !args.is_empty() {
            line.push(' ');
            line.push_str(&args);
        }
        self.execs.lock().unwrap().push(line.clone());
        match self.replies.iter().find(|(k, _)| *k == line) {
            Some((_, stdout)) => Ok(ExecOutput {
                status: 0,
                stdout: stdout.to_string(),
                stderr: String::new(),
            }),
            None => Ok(ExecOutput {
                status: 1,
                stdout: String::new(),
                stderr: format!("{line}: not stubbed"),
            }),
        }
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
    let host = MockHost::new("").replying(&[("git status --porcelain", "ok\n")]);
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
    assert_eq!(
        commands.lock().unwrap().as_slice(),
        &["echo hi".to_string()]
    );

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
    assert!(
        result.is_err(),
        "instantiation must fail without the commands capability"
    );
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
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../sdk/js/plugin.wasm");
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
    let mut plugin = PluginInstance::new(&eng, &fixture(), &granted(), host).unwrap();
    plugin.set_fuel_budget(50_000_000); // small budget so the test is fast
    let result = plugin.call_tool("spin", "{}");
    assert!(result.is_err(), "an infinite-loop tool must trap, not hang");
}

/// The `git` plugin, ported off the subprocess tier: it reaches git only through
/// the gated `host-process` interface, so a mock host that answers `exec` is
/// enough to drive its panel and its tools.
#[test]
fn git_plugin_runs_entirely_through_host_process() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins/git/plugin.wasm");
    let wasm = std::fs::read(path).expect("git plugin.wasm");
    let eng = engine().unwrap();

    // Answers like a repo on `main` with three changed paths. Note the leading
    // space on the second status entry: unstaged, and the column must survive
    // untrimmed or the path shifts by one.
    let host = MockHost::new("").replying(&[
        ("git rev-parse --is-inside-work-tree", "true\n"),
        ("git rev-parse --abbrev-ref HEAD", "main\n"),
        ("git rev-list --left-right --count @{u}...HEAD", "2\t5\n"),
        (
            "git status --porcelain",
            "M  src/a.rs\n M src/b.rs\n?? new.txt\n",
        ),
    ]);
    let execs = host.execs.clone();
    let commands = host.commands.clone();
    let host = Box::new(host);
    let mut plugin = PluginInstance::new(
        &eng,
        &wasm,
        &["process".to_string(), "commands".to_string()],
        host,
    )
    .expect("instantiate git");

    // The panel reports the branch, the ahead/behind split, and every change.
    let tree = plugin.render("{}").unwrap();
    let node: Value = serde_json::from_str(&tree).unwrap();
    assert_eq!(node["title"], "Git \u{b7} main");
    let blocks = node["blocks"].as_array().unwrap();
    let text = tree.as_str();
    assert!(blocks.iter().any(|b| b["value"] == "main"), "{tree}");
    assert!(blocks.iter().any(|b| b["value"] == "5 / 2"), "{tree}");
    assert!(text.contains("Changes (3)"), "{tree}");
    // The untrimmed status column: " M src/b.rs" must yield the path, not "M".
    assert!(text.contains("src/b.rs"), "{tree}");
    assert!(text.contains("new.txt"), "{tree}");

    // A button that mutates the repo goes through exec...
    plugin.on_ui_event("{\"id\":\"stage_all\"}").unwrap();
    assert!(
        execs.lock().unwrap().iter().any(|e| e == "git add -A"),
        "{:?}",
        execs.lock().unwrap()
    );
    // ...and one that belongs in the terminal goes through run-command.
    plugin.on_ui_event("{\"id\":\"log\"}").unwrap();
    assert_eq!(commands.lock().unwrap().len(), 1);
    assert!(commands.lock().unwrap()[0].starts_with("git log"));

    // The status tool returns the same state as structured data.
    let out = plugin.call_tool("status", "{}").unwrap().unwrap();
    let value: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(value["branch"], "main");
    assert_eq!(value["changes"].as_array().unwrap().len(), 3);
    assert_eq!(value["changes"][2]["path"], "new.txt");
    assert_eq!(value["changes"][2]["code"], "??");
}

/// Load a ported bundled plugin against a host that answers `exec` from a table.
fn ported(name: &str, replies: &[(&'static str, &'static str)]) -> (PluginInstance, MockHost) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../plugins/{name}/plugin.wasm"));
    let wasm = std::fs::read(&path).unwrap_or_else(|_| panic!("{name} plugin.wasm"));
    let eng = engine().unwrap();
    let host = MockHost::new("").replying(replies);
    let handle = MockHost {
        commands: host.commands.clone(),
        screen: String::new(),
        execs: host.execs.clone(),
        replies: Vec::new(),
    };
    let plugin = PluginInstance::new(
        &eng,
        &wasm,
        &["process".to_string(), "commands".to_string()],
        Box::new(host),
    )
    .unwrap_or_else(|e| panic!("instantiate {name}: {e}"));
    (plugin, handle)
}

#[test]
fn docker_plugin_lists_containers_through_host_process() {
    let (mut plugin, handle) = ported(
        "docker",
        &[
            ("docker version --format {{.Server.Version}}", "27.0.3\n"),
            (
                "docker ps -a --format {{.Names}}\t{{.Status}}\t{{.Image}}",
                "web\tUp 3 hours\tnginx\ndb\tExited (0) 2 days ago\tpostgres\n",
            ),
        ],
    );

    let tree = plugin.render("{}").unwrap();
    assert!(tree.contains("Containers (2)"), "{tree}");
    // A running container badges up, a stopped one off.
    assert!(tree.contains("\"up\""), "{tree}");
    assert!(tree.contains("\"off\""), "{tree}");
    assert!(tree.contains("web"), "{tree}");

    // The live view belongs in a tab, not the panel.
    plugin.on_ui_event("{\"id\":\"stats\"}").unwrap();
    assert_eq!(
        handle.commands.lock().unwrap().as_slice(),
        &["docker stats".to_string()]
    );

    let out = plugin.call_tool("containers", "{}").unwrap().unwrap();
    let value: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(value["containers"][0]["name"], "web");
    assert_eq!(value["containers"][0]["running"], true);
    assert_eq!(value["containers"][1]["running"], false);
}

/// With no docker daemon answering, the panel says so rather than rendering an
/// empty container list as though everything were fine.
#[test]
fn docker_plugin_reports_a_missing_daemon() {
    let (mut plugin, _) = ported("docker", &[]);
    let tree = plugin.render("{}").unwrap();
    assert!(tree.contains("not available or not running"), "{tree}");
}

#[test]
fn sysinfo_plugin_reads_host_stats_through_host_process() {
    // macOS phrases it "load averages:", Linux "load average:" — the plugin
    // matches the stem, so both must land.
    for uptime in [
        "12:01  up 3 days, 22:15, 4 users, load averages: 2.31 2.10 1.98",
        " 12:01:00 up 3 days, 22:15,  4 users,  load average: 2.31, 2.10, 1.98",
    ] {
        let (mut plugin, _) = ported(
            "sysinfo",
            &[
                ("uptime", uptime),
                ("hostname", "studio.local\n"),
                (
                    "df -h .",
                    "Filesystem Size Used Avail Capacity\n/dev/disk3s5 926Gi 412Gi 500Gi 46%\n",
                ),
            ],
        );
        let out = plugin.call_tool("stats", "{}").unwrap().unwrap();
        let value: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(value["disk"]["size"], "926Gi", "{out}");
        assert_eq!(value["disk"]["avail"], "500Gi", "{out}");
        assert!(
            value["load"]
                .as_str()
                .unwrap_or_default()
                .starts_with("2.31"),
            "{out}"
        );
    }
}

#[test]
fn sysinfo_plugin_opens_the_monitor_in_a_split() {
    let (mut plugin, handle) = ported("sysinfo", &[]);
    plugin.on_ui_event("{\"id\":\"monitor\"}").unwrap();
    let commands = handle.commands.lock().unwrap();
    assert_eq!(commands.len(), 1);
    assert!(commands[0].contains("btop"), "{commands:?}");
}

/// promptdesigner keeps its design in the ungated per-plugin storage and drives
/// the shell through `run-command`, so it needs no filesystem grant at all.
#[test]
fn promptdesigner_persists_its_design_and_applies_visibly() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../plugins/promptdesigner/plugin.wasm");
    let wasm = std::fs::read(path).expect("promptdesigner plugin.wasm");
    let eng = engine().unwrap();

    // A host with a real key/value store, so state survives across calls the way
    // it does in the app.
    struct StoreHost {
        store: Arc<Mutex<std::collections::HashMap<String, String>>>,
        commands: Arc<Mutex<Vec<String>>>,
    }
    impl AppHost for StoreHost {
        fn log(&mut self, _level: LogLevel, _message: String) {}
        fn storage_get(&mut self, key: String) -> Option<String> {
            self.store.lock().unwrap().get(&key).cloned()
        }
        fn storage_set(&mut self, key: String, value: String) {
            self.store.lock().unwrap().insert(key, value);
        }
        fn run_command(&mut self, text: String, _target: CommandTarget) -> Result<(), String> {
            self.commands.lock().unwrap().push(text);
            Ok(())
        }
        fn send_input(&mut self, _bytes: Vec<u8>) -> Result<(), String> {
            Ok(())
        }
        fn read_screen(&mut self, _lines: u32) -> Result<String, String> {
            Err("no screen".into())
        }
        fn selection(&mut self) -> Option<String> {
            None
        }
        fn fetch(&mut self, _r: HttpRequest) -> Result<HttpResponse, String> {
            Err("no network".into())
        }
        fn read_file(&mut self, _p: String) -> Result<Vec<u8>, String> {
            Err("no fs".into())
        }
        fn write_file(&mut self, _p: String, _d: Vec<u8>) -> Result<(), String> {
            Err("no fs".into())
        }
        fn clipboard_read(&mut self) -> Result<String, String> {
            Err("no clipboard".into())
        }
        fn clipboard_write(&mut self, _t: String) -> Result<(), String> {
            Ok(())
        }
        fn notify(&mut self, _t: String, _b: String) {}
        fn exec(&mut self, _r: ExecRequest) -> Result<ExecOutput, String> {
            panic!("promptdesigner must not need host-process");
        }
    }

    let store = Arc::new(Mutex::new(std::collections::HashMap::new()));
    let commands = Arc::new(Mutex::new(Vec::new()));
    let host = Box::new(StoreHost {
        store: store.clone(),
        commands: commands.clone(),
    });
    // `commands` alone — no filesystem, no process.
    let mut plugin = PluginInstance::new(&eng, &wasm, &["commands".to_string()], host)
        .expect("instantiate promptdesigner");

    // The preview paints one coloured, monospaced segment per part.
    let tree = plugin.render("{}").unwrap();
    let node: Value = serde_json::from_str(&tree).unwrap();
    assert_eq!(node["title"], "Prompt Designer");
    let preview = &node["blocks"][1]["children"];
    assert!(preview[0]["mono"].as_bool().unwrap(), "{tree}");
    assert_eq!(preview[0]["color"], "cyan");
    // The git segment is yellow regardless of the chosen colour, matching the
    // snippet it generates.
    assert!(
        preview
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["color"] == "yellow"),
        "{tree}"
    );

    // A toggle persists: the label flips and stays flipped on the next render.
    assert!(tree.contains("directory: on"));
    plugin.on_ui_event("{\"id\":\"toggle:cwd\"}").unwrap();
    assert!(plugin.render("{}").unwrap().contains("directory: off"));
    assert!(store.lock().unwrap().contains_key("design"));

    // A colour choice reaches both the preview and the generated snippet.
    plugin.on_ui_event("{\"id\":\"color:green\"}").unwrap();
    let out = plugin
        .call_tool("snippet", "{\"shell\":\"zsh\"}")
        .unwrap()
        .unwrap();
    let value: Value = serde_json::from_str(&out).unwrap();
    assert!(
        value["snippet"].as_str().unwrap().contains("%F{green}"),
        "{out}"
    );

    // Applying runs a visible command rather than writing files itself.
    plugin.on_ui_event("{\"id\":\"apply:zsh\"}").unwrap();
    let ran = commands.lock().unwrap();
    assert_eq!(ran.len(), 1);
    assert!(ran[0].contains(".zshrc"), "{ran:?}");
    assert!(ran[0].contains("prompt-designer"), "{ran:?}");

    // A stored design naming a symbol outside the allowlist must not reach the
    // snippet — storage is not a trust boundary.
    store.lock().unwrap().insert(
        "design".to_string(),
        "{\"symbol\":\"; rm -rf /\",\"color\":\"$(id)\"}".to_string(),
    );
    let out = plugin.call_tool("snippet", "{}").unwrap().unwrap();
    assert!(!out.contains("rm -rf"), "{out}");
    assert!(!out.contains("$(id)"), "{out}");
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
    let out = plugin
        .call_tool("grep", "{\"query\":\"error\"}")
        .unwrap()
        .unwrap();
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

/// Fuel bounds how long a guest runs but says nothing about how much it can
/// allocate. Without the store's memory ceiling this test would not fail — it
/// would take the test process down with it, which is exactly what it would do
/// to the terminal.
#[test]
fn a_guest_cannot_allocate_the_host_to_death() {
    let eng = engine().unwrap();
    let host = Box::new(MockHost::new(""));
    let mut plugin = PluginInstance::new(&eng, &fixture(), &granted(), host).unwrap();
    // Generous fuel: the point is that memory stops it, not the clock.
    plugin.set_fuel_budget(20_000_000_000);
    let result = plugin.call_tool("glutton", "{}");
    assert!(
        result.is_err(),
        "an unbounded allocator must be stopped by the store, not by the OS"
    );
}
