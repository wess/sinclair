use super::*;

#[test]
fn keybind_entries_use_plugin_actions() {
    let plugins = vec![Plugin {
        id: "tools".to_string(),
        name: "Tools".to_string(),
        version: "0.1.0".to_string(),
        description: None,
        path: std::path::PathBuf::from("/tmp/tools"),
        commands: vec![Command {
            id: "top".to_string(),
            title: "Top".to_string(),
            run: "top".to_string(),
            mode: CommandMode::Tab,
            keybind: Some("cmd+shift+t".to_string()),
        }],
    }];
    assert_eq!(
        keybinds(&plugins),
        vec!["cmd+shift+t=plugin_command:tools/top".to_string()]
    );
    assert_eq!(command(&plugins, "tools/top").unwrap().1.run, "top");
    assert!(command(&plugins, "tools/missing").is_none());
}
