use super::*;

#[test]
fn groups_blocks_by_prompt_lines() {
    let lines = vec![
        Line {
            number: 0,
            text: "$ cargo test".into(),
            prompt: true,
        },
        Line {
            number: 1,
            text: "running".into(),
            prompt: false,
        },
        Line {
            number: 2,
            text: "$ bun test".into(),
            prompt: true,
        },
    ];
    let got = blocks(&lines);
    assert_eq!(got.len(), 2);
    assert_eq!((got[0].start, got[0].end), (0, 1));
    assert_eq!((got[1].start, got[1].end), (2, 2));
}

#[test]
fn ranks_related_failures() {
    let lines = vec![
        Line {
            number: 0,
            text: "$ deploy".into(),
            prompt: true,
        },
        Line {
            number: 1,
            text: "permission denied for token".into(),
            prompt: false,
        },
        Line {
            number: 2,
            text: "$ ls".into(),
            prompt: true,
        },
    ];
    let hits = search("auth error", &lines, 3);
    assert_eq!(hits[0].block.start, 0);
}
