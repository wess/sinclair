    use super::*;

    fn path() -> PathBuf {
        PathBuf::from("/tmp/tool/plugin.toml")
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
id = tools
[[command]]
id = top
run = top
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
id = tools
bogus = true
[[command]]
id = ok
run = echo ok
[[command]]
id = Bad
run = echo bad
"#,
        );
        assert_eq!(diags.len(), 2);
        let plugin = plugin.unwrap();
        assert_eq!(plugin.commands.len(), 1);
        assert_eq!(plugin.commands[0].id, "ok");
    }

    #[test]
    fn missing_plugin_id_skips_plugin() {
        let (plugin, diags) = parse(path(), "[[command]]\nid = ok\nrun = echo ok\n");
        assert!(plugin.is_none());
        assert!(diags.iter().any(|d| d.message == "missing id"));
    }
