use super::*;

#[test]
fn parses_block_tree_response() {
    let json = r#"{
      "title": "Git",
      "blocks": [
        {"type": "section", "title": "Branch"},
        {"type": "text", "text": "main", "dimmed": true},
        {"type": "divider"},
        {"type": "kv", "key": "ahead", "value": "2"},
        {"type": "button", "id": "commit", "label": "Commit", "variant": "filled"},
        {"type": "row", "children": [{"type": "badge", "label": "M"}]}
      ],
      "run": [{"text": "git status", "target": "pane"}]
    }"#;
    let r: Response = serde_json::from_str(json).unwrap();
    assert_eq!(r.title.as_deref(), Some("Git"));
    assert_eq!(r.blocks.len(), 6);
    assert_eq!(r.run.len(), 1);
    assert_eq!(r.run[0].text, "git status");
    assert_eq!(r.run[0].target.as_deref(), Some("pane"));
    match &r.blocks[0] {
        Block::Section { title } => assert_eq!(title, "Branch"),
        _ => panic!("expected section"),
    }
    match &r.blocks[5] {
        Block::Row { children } => assert_eq!(children.len(), 1),
        _ => panic!("expected row"),
    }
}

#[test]
fn empty_object_is_empty_response() {
    let r: Response = serde_json::from_str("{}").unwrap();
    assert!(r.blocks.is_empty());
    assert!(r.run.is_empty());
    assert!(r.title.is_none());
}
