    use super::*;

    fn path() -> PathBuf {
        PathBuf::from("/tmp/tool/plugin.toml")
    }

    #[test]
    fn parses_runtime_and_panel() {
        let src = r#"
id = "git"
name = "Git"

[runtime]
command = "bun run plugin.ts"

[panel]
id = "git"
title = "Git"
icon = "G"
"#;
        let (plugin, diags) = parse(path(), src);
        assert!(diags.is_empty(), "{diags:?}");
        let plugin = plugin.unwrap();
        let runtime = plugin.runtime.expect("runtime");
        assert_eq!(runtime.command, "bun run plugin.ts");
        let panel = plugin.panel.expect("panel");
        assert_eq!(panel.id, "git");
        assert_eq!(panel.title, "Git");
        assert_eq!(panel.icon, "G");
    }

    #[test]
    fn panel_defaults_from_plugin() {
        let src = r#"
id = "todos"
name = "Todos"

[runtime]
command = "./todos"

[panel]
"#;
        let (plugin, diags) = parse(path(), src);
        assert!(diags.is_empty(), "{diags:?}");
        let plugin = plugin.unwrap();
        let panel = plugin.panel.expect("panel");
        assert_eq!(panel.id, "todos");
        assert_eq!(panel.title, "Todos");
        assert_eq!(panel.icon, "\u{25c9}");
    }

    #[test]
    fn runtime_without_command_is_diagnosed() {
        let src = r#"
id = "bad"
[runtime]
"#;
        let (plugin, diags) = parse(path(), src);
        assert!(plugin.is_some());
        assert!(plugin.unwrap().runtime.is_none());
        assert!(diags.iter().any(|d| d.message.contains("[runtime] requires")));
    }

    #[test]
    fn parses_plugin_commands() {
        let src = r#"
id = "tools"
name = "Tools"
version = "1.2.3"
description = "Useful commands"

[[command]]
id = "logs"
title = "Tail logs"
run = "tail -f /tmp/app.log"
mode = "split-right"
keybind = "cmd+shift+l"
"#;
        let (plugin, diags) = parse(path(), src);
        assert!(diags.is_empty(), "{diags:?}");
        let plugin = plugin.unwrap();
        assert_eq!(plugin.id, "tools");
        assert_eq!(plugin.name, "Tools");
        assert_eq!(plugin.version, "1.2.3");
        assert_eq!(plugin.description.as_deref(), Some("Useful commands"));
        assert_eq!(plugin.path, PathBuf::from("/tmp/tool"));
        assert_eq!(plugin.commands.len(), 1);
        assert_eq!(plugin.commands[0].id, "logs");
        assert_eq!(plugin.commands[0].mode, CommandMode::SplitRight);
        assert_eq!(plugin.commands[0].keybind.as_deref(), Some("cmd+shift+l"));
    }

    #[test]
    fn defaults_optional_fields() {
        let (plugin, diags) = parse(
            path(),
            r#"
id = "tools"
[[command]]
id = "top"
run = "top"
"#,
        );
        assert!(diags.is_empty(), "{diags:?}");
        let plugin = plugin.unwrap();
        assert_eq!(plugin.name, "tools");
        assert_eq!(plugin.version, "0.0.0");
        assert_eq!(plugin.commands[0].title, "top");
        assert_eq!(plugin.commands[0].mode, CommandMode::Pane);
    }

    #[test]
    fn reports_bad_manifest_but_keeps_good_commands() {
        let (plugin, diags) = parse(
            path(),
            r#"
id = "tools"
bogus = true
[[command]]
id = "ok"
run = "echo ok"
[[command]]
id = "Bad"
run = "echo bad"
"#,
        );
        // `bogus` is an unknown key and ignored; the bad command id is the one
        // diagnostic, and the good command is kept.
        assert_eq!(diags.len(), 1, "{diags:?}");
        let plugin = plugin.unwrap();
        assert_eq!(plugin.commands.len(), 1);
        assert_eq!(plugin.commands[0].id, "ok");
    }

    #[test]
    fn missing_plugin_id_skips_plugin() {
        let (plugin, diags) = parse(path(), "[[command]]\nid = \"ok\"\nrun = \"echo ok\"\n");
        assert!(plugin.is_none());
        assert!(diags.iter().any(|d| d.message == "missing id"));
    }

    #[test]
    fn parses_webview_contribution() {
        let (plugin, diags) = parse(
            path(),
            r#"
id = "dash"
name = "Dash"
[webview]
id = "board"
title = "Board"
icon = "◱"
placement = "window"
entry = "index.html"
"#,
        );
        assert!(diags.is_empty(), "{diags:?}");
        let wv = plugin.unwrap().webview.unwrap();
        assert_eq!(wv.id, "board");
        assert_eq!(wv.title, "Board");
        assert_eq!(wv.placement, Placement::Window);
        assert_eq!(wv.source, WebviewSource::Entry("index.html".to_string()));
    }

    #[test]
    fn webview_defaults_to_plugin_id_name_and_panel() {
        let (plugin, diags) = parse(
            path(),
            "id = \"dash\"\nname = \"Dash\"\n[webview]\nurl = \"https://example.com\"\n",
        );
        assert!(diags.is_empty(), "{diags:?}");
        let wv = plugin.unwrap().webview.unwrap();
        assert_eq!(wv.id, "dash"); // falls back to plugin id
        assert_eq!(wv.title, "Dash"); // falls back to plugin name
        assert_eq!(wv.placement, Placement::Panel); // default
        assert_eq!(
            wv.source,
            WebviewSource::Url("https://example.com".to_string())
        );
    }

    #[test]
    fn webview_requires_a_source() {
        let (plugin, diags) = parse(path(), "id = \"dash\"\n[webview]\ntitle = \"X\"\n");
        assert!(plugin.unwrap().webview.is_none());
        assert!(diags
            .iter()
            .any(|d| d.message.contains("requires a `url` or `entry`")));
    }

    #[test]
    fn webview_rejects_both_sources() {
        let (plugin, diags) = parse(
            path(),
            "id = \"dash\"\n[webview]\nurl = \"https://x\"\nentry = \"i.html\"\n",
        );
        assert!(plugin.unwrap().webview.is_none());
        assert!(diags.iter().any(|d| d.message.contains("exactly one")));
    }

    #[test]
    fn webview_bad_placement_reports_and_defaults_to_panel() {
        let (plugin, diags) = parse(
            path(),
            "id = \"dash\"\n[webview]\nplacement = \"floating\"\nurl = \"https://x\"\n",
        );
        assert!(diags.iter().any(|d| d.message.contains("placement")));
        assert_eq!(
            plugin.unwrap().webview.unwrap().placement,
            Placement::Panel
        );
    }

    #[test]
    fn parses_triggers() {
        let (plugin, diags) = parse(
            path(),
            r#"
id = "watch"
[[trigger]]
on = "command_finished"
when = "nonzero"
notify = "A command failed"
[[trigger]]
on = "dir_changed"
run = "direnv reload"
target = "background"
[[trigger]]
on = "bell"
invoke = "onBell"
"#,
        );
        assert!(diags.is_empty(), "{diags:?}");
        let t = plugin.unwrap().triggers;
        assert_eq!(t.len(), 3);
        assert_eq!(t[0].on, "command_finished");
        assert_eq!(t[0].when.as_deref(), Some("nonzero"));
        assert_eq!(t[0].action, TriggerAction::Notify { text: "A command failed".into() });
        assert_eq!(
            t[1].action,
            TriggerAction::Run { text: "direnv reload".into(), target: TriggerTarget::Background }
        );
        assert_eq!(t[2].action, TriggerAction::Invoke { method: "onBell".into() });
    }

    #[test]
    fn trigger_run_defaults_to_background() {
        let (plugin, diags) =
            parse(path(), "id = \"w\"\n[[trigger]]\non = \"exit\"\nrun = \"say done\"\n");
        assert!(diags.is_empty(), "{diags:?}");
        assert_eq!(
            plugin.unwrap().triggers[0].action,
            TriggerAction::Run { text: "say done".into(), target: TriggerTarget::Background }
        );
    }

    #[test]
    fn trigger_unknown_event_is_dropped() {
        let (plugin, diags) =
            parse(path(), "id = \"w\"\n[[trigger]]\non = \"blinked\"\nnotify = \"x\"\n");
        assert!(plugin.unwrap().triggers.is_empty());
        assert!(diags.iter().any(|d| d.message.contains("unknown trigger event")));
    }

    #[test]
    fn trigger_needs_exactly_one_action() {
        let (none_plugin, none_diags) =
            parse(path(), "id = \"w\"\n[[trigger]]\non = \"bell\"\n");
        assert!(none_plugin.unwrap().triggers.is_empty());
        assert!(none_diags.iter().any(|d| d.message.contains("exactly one action")));

        let (two_plugin, two_diags) = parse(
            path(),
            "id = \"w\"\n[[trigger]]\non = \"bell\"\nrun = \"a\"\nnotify = \"b\"\n",
        );
        assert!(two_plugin.unwrap().triggers.is_empty());
        assert!(two_diags.iter().any(|d| d.message.contains("exactly one action")));
    }

    #[test]
    fn parses_tools() {
        let (plugin, diags) = parse(
            path(),
            r#"
id = "db"
[runtime]
command = "bun run plugin.ts"
[[tool]]
id = "query"
description = "Run a SQL query."
[[tool.param]]
name = "sql"
type = "string"
description = "The SQL to run"
required = true
[[tool.param]]
name = "limit"
type = "integer"
description = "Max rows"
[[tool]]
id = "tables"
description = "List tables."
"#,
        );
        assert!(diags.is_empty(), "{diags:?}");
        let tools = plugin.unwrap().tools;
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].id, "query");
        assert_eq!(tools[0].description, "Run a SQL query.");
        assert_eq!(tools[0].params.len(), 2);
        assert_eq!(tools[0].params[0].name, "sql");
        assert_eq!(tools[0].params[0].kind, "string");
        assert!(tools[0].params[0].required);
        assert_eq!(tools[0].params[1].kind, "integer");
        assert!(!tools[0].params[1].required);
        assert!(tools[1].params.is_empty());
    }

    #[test]
    fn parses_legacy_pipe_params() {
        // The `name | type | desc | required` pipe form still works as an array.
        let (plugin, diags) = parse(
            path(),
            r#"
id = "db"
[runtime]
command = "./db"
[[tool]]
id = "query"
description = "Run a query."
param = ["sql | string | The SQL | required", "limit | integer | Max rows"]
"#,
        );
        assert!(diags.is_empty(), "{diags:?}");
        let tools = plugin.unwrap().tools;
        assert_eq!(tools[0].params.len(), 2);
        assert_eq!(tools[0].params[0].name, "sql");
        assert!(tools[0].params[0].required);
        assert_eq!(tools[0].params[1].kind, "integer");
    }

    #[test]
    fn parses_wasm_runtime() {
        let (plugin, diags) = parse(
            path(),
            "id = \"w\"\n[runtime]\ntype = \"wasm\"\nwasm = \"plugin.wasm\"\n",
        );
        assert!(diags.is_empty(), "{diags:?}");
        let rt = plugin.unwrap().runtime.unwrap();
        assert_eq!(rt.kind, RuntimeKind::Wasm);
        assert_eq!(rt.wasm.as_deref(), Some("plugin.wasm"));
    }

    #[test]
    fn wasm_runtime_without_module_is_diagnosed() {
        let (plugin, diags) = parse(path(), "id = \"w\"\n[runtime]\ntype = \"wasm\"\n");
        assert!(plugin.unwrap().runtime.is_none());
        assert!(diags.iter().any(|d| d.message.contains("wasm")));
    }

    #[test]
    fn process_runtime_is_the_default() {
        let (plugin, _) = parse(path(), "id = \"p\"\n[runtime]\ncommand = \"bun run x.ts\"\n");
        assert_eq!(plugin.unwrap().runtime.unwrap().kind, RuntimeKind::Process);
    }

    #[test]
    fn parses_capabilities() {
        let (plugin, diags) = parse(
            path(),
            "id = \"x\"\ncapabilities = [\"network\", \"filesystem\", \"network\"]\n",
        );
        assert!(diags.is_empty(), "{diags:?}");
        // Deduplicated, known-only.
        assert_eq!(plugin.unwrap().capabilities, vec!["network", "filesystem"]);
    }

    #[test]
    fn unknown_capability_is_diagnosed() {
        let (plugin, diags) = parse(path(), "id = \"x\"\ncapabilities = [\"root\"]\n");
        assert!(plugin.unwrap().capabilities.is_empty());
        assert!(diags.iter().any(|d| d.message.contains("unknown capability")));
    }

    #[test]
    fn tool_without_runtime_is_diagnosed() {
        let (plugin, diags) = parse(
            path(),
            "id = \"x\"\n[[tool]]\nid = \"q\"\ndescription = \"d\"\n",
        );
        // The plugin still loads, but the tool is dropped with a diagnostic.
        assert!(plugin.unwrap().tools.is_empty());
        assert!(diags.iter().any(|d| d.message.contains("[runtime]")));
    }

    #[test]
    fn bundled_manifests_parse_cleanly() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins");
        let mut checked = 0;
        for entry in std::fs::read_dir(&root).expect("plugins dir") {
            let manifest = entry.unwrap().path().join("plugin.toml");
            if !manifest.is_file() {
                continue;
            }
            let text = std::fs::read_to_string(&manifest).unwrap();
            let (plugin, diags) = parse(manifest.clone(), &text);
            assert!(diags.is_empty(), "{}: {diags:?}", manifest.display());
            assert!(plugin.is_some(), "{} did not parse", manifest.display());
            checked += 1;
        }
        assert!(checked >= 5, "expected the bundled plugins, checked {checked}");
    }
