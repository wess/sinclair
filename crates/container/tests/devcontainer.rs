use super::*;

const PROJECT: &str = "/Users/wess/code/api";

#[test]
fn config_paths_cover_both_layouts() {
    let p = config_paths("/repo/");
    assert_eq!(p[0], "/repo/.devcontainer/devcontainer.json");
    assert_eq!(p[1], "/repo/.devcontainer.json");
}

#[test]
fn comments_and_trailing_commas_are_accepted() {
    let dc = parse(
        r#"{
            // the project's dev image
            "image": "node:22",
            "remoteUser": "node",
        }"#,
        PROJECT,
    )
    .unwrap();
    assert_eq!(dc.image.as_deref(), Some("node:22"));
    assert_eq!(dc.remote_user.as_deref(), Some("node"));
}

#[test]
fn default_workspace_folder_follows_the_spec() {
    let dc = parse(r#"{"image":"x"}"#, PROJECT).unwrap();
    assert_eq!(dc.workspace_folder_for(PROJECT), "/workspaces/api");
    assert!(!dc.is_identity_mapped(PROJECT));
}

#[test]
fn identity_mapping_is_detected() {
    let dc = parse(
        r#"{
            "workspaceMount": "source=${localWorkspaceFolder},target=${localWorkspaceFolder},type=bind",
            "workspaceFolder": "${localWorkspaceFolder}"
        }"#,
        PROJECT,
    )
    .unwrap();
    assert_eq!(dc.workspace_folder_for(PROJECT), PROJECT);
    assert!(dc.is_identity_mapped(PROJECT));
    assert!(dc
        .workspace_mount
        .unwrap()
        .contains("source=/Users/wess/code/api,target=/Users/wess/code/api"));
}

#[test]
fn shutdown_action_decides_whether_closing_the_editor_kills_agents() {
    assert!(parse(r#"{"image":"x"}"#, PROJECT).unwrap().stops_on_close());
    assert!(parse(r#"{"shutdownAction":"stopContainer"}"#, PROJECT)
        .unwrap()
        .stops_on_close());
    assert!(!parse(r#"{"shutdownAction":"none"}"#, PROJECT)
        .unwrap()
        .stops_on_close());
}

#[test]
fn env_merges_with_remote_winning() {
    let dc = parse(
        r#"{"containerEnv":{"A":"1","B":"2"},"remoteEnv":{"B":"override"}}"#,
        PROJECT,
    )
    .unwrap();
    assert!(dc.env.contains(&("A".to_string(), "1".to_string())));
    assert!(dc.env.contains(&("B".to_string(), "override".to_string())));
    assert_eq!(dc.env.iter().filter(|(k, _)| k == "B").count(), 1);
}

#[test]
fn basename_variables_resolve() {
    let dc = parse(
        r#"{"containerEnv":{"NAME":"${localWorkspaceFolderBasename}"},"runArgs":["--name","${containerWorkspaceFolderBasename}"]}"#,
        PROJECT,
    )
    .unwrap();
    assert_eq!(dc.env[0].1, "api");
    assert_eq!(dc.run_args[1], "api");
}

#[test]
fn dockerfile_build_is_read() {
    let dc = parse(r#"{"build":{"dockerfile":"Dockerfile"}}"#, PROJECT).unwrap();
    assert_eq!(dc.dockerfile.as_deref(), Some("Dockerfile"));
}

#[test]
fn post_create_normalises_all_three_shapes() {
    assert_eq!(
        parse(r#"{"postCreateCommand":"npm i"}"#, PROJECT)
            .unwrap()
            .post_create,
        vec!["npm i"]
    );
    assert_eq!(
        parse(r#"{"postCreateCommand":["npm","i"]}"#, PROJECT)
            .unwrap()
            .post_create,
        vec!["npm i"]
    );
    assert_eq!(
        parse(
            r#"{"postCreateCommand":{"deps":"npm i","build":"npm run build"}}"#,
            PROJECT
        )
        .unwrap()
        .post_create
        .len(),
        2
    );
}

#[test]
fn malformed_json_is_a_friendly_error() {
    let err = parse("{ nope", PROJECT).unwrap_err();
    assert!(err.contains("line"));
}

#[test]
fn a_non_object_is_rejected() {
    assert!(parse("[]", PROJECT).is_err());
}
