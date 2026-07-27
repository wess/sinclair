use super::*;

const SEP: &str = "\u{1f}";

fn row(id: &str, name: &str, state: &str, labels: &str) -> String {
    format!("{id}{SEP}{name}{SEP}img{SEP}{state}{SEP}{labels}\n")
}

#[test]
fn find_argv_filters_on_the_devcontainer_label() {
    let argv = find_argv(Engine::Docker, "/Users/wess/code/api/");
    assert!(argv.contains(&"-a".to_string()));
    assert!(argv.contains(&"label=devcontainer.local_folder=/Users/wess/code/api".to_string()));
    assert!(argv.last().unwrap().contains("{{.Labels}}"));
}

#[test]
fn sinclair_owned_is_recognised() {
    let out = row("abc", "sinclair-sbx-api-1", "running", "sinclair.owner=sinclair,sinclair.sandbox=1");
    let found = parse_found(&out);
    assert_eq!(found[0].owner, Owner::Sinclair);
    assert!(found[0].owner.may_remove());
    assert!(found[0].state.is_running());
}

#[test]
fn a_vscode_container_is_foreign_and_untouchable() {
    // No sinclair.owner label: VS Code built it, so the user's editor is very
    // likely attached and Sinclair must not stop it.
    let out = row(
        "def",
        "vsc-api-1234",
        "running",
        "devcontainer.local_folder=/Users/wess/code/api,devcontainer.config_file=/Users/wess/code/api/.devcontainer/devcontainer.json",
    );
    let found = parse_found(&out);
    assert_eq!(found[0].owner, Owner::Foreign);
    assert!(!found[0].owner.may_remove());
    assert_eq!(
        found[0].config_file.as_deref(),
        Some("/Users/wess/code/api/.devcontainer/devcontainer.json")
    );
}

#[test]
fn best_prefers_running_then_our_own() {
    let out = format!(
        "{}{}",
        row("stopped", "a", "exited", "sinclair.owner=sinclair"),
        row("live", "b", "running", "devcontainer.local_folder=/x")
    );
    let found = parse_found(&out);
    assert_eq!(best(&found).unwrap().id, "live");
}

#[test]
fn best_prefers_our_own_among_running() {
    let out = format!(
        "{}{}",
        row("foreign", "a", "running", "devcontainer.local_folder=/x"),
        row("ours", "b", "running", "sinclair.owner=sinclair")
    );
    assert_eq!(best(&parse_found(&out)).unwrap().id, "ours");
}

#[test]
fn nothing_found_is_none() {
    assert!(best(&parse_found("")).is_none());
    assert!(parse_found("\n  \n").is_empty());
}

#[test]
fn labels_parse() {
    let l = parse_labels("a=1,b=two words,c");
    assert_eq!(l[0], ("a".to_string(), "1".to_string()));
    assert_eq!(l[1], ("b".to_string(), "two words".to_string()));
    assert_eq!(l[2], ("c".to_string(), String::new()));
}
