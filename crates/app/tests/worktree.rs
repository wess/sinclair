use super::*;

#[test]
fn parses_porcelain_list() {
    let text = "\
worktree /repo
HEAD abc123
branch refs/heads/main

worktree /repo/../wt-feature
HEAD def456
branch refs/heads/feature

worktree /repo/detached
HEAD 999aaa
detached
";
    let list = parse_list(text);
    assert_eq!(list.len(), 3);
    assert_eq!(list[0].path, std::path::PathBuf::from("/repo"));
    assert_eq!(list[0].branch.as_deref(), Some("main"));
    assert_eq!(list[1].branch.as_deref(), Some("feature"));
    assert_eq!(list[2].branch, None, "detached worktree has no branch");
}

#[test]
fn resolve_absolute_vs_relative() {
    let base = std::path::Path::new("/home/me/repo");
    assert_eq!(resolve(base, "/tmp/wt"), std::path::PathBuf::from("/tmp/wt"));
    assert_eq!(
        resolve(base, "../wt"),
        std::path::PathBuf::from("/home/me/repo/../wt")
    );
}

/// End-to-end against a real throwaway repository: create a worktree on a new
/// branch, see it in the list, then remove it.
#[test]
fn create_list_remove_roundtrip() {
    use std::process::Command;
    let base = std::env::temp_dir().join(format!("sinclair-wt-{}", std::process::id()));
    let repo = base.join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let run = |args: &[&str]| {
        Command::new("git").current_dir(&repo).args(args).output().unwrap()
    };
    // A minimal committed repo so HEAD exists for `worktree add`.
    run(&["init", "-q"]);
    run(&["config", "user.email", "t@example.com"]);
    run(&["config", "user.name", "t"]);
    std::fs::write(repo.join("f"), "x").unwrap();
    run(&["add", "-A"]);
    run(&["commit", "-qm", "init"]);

    let wt = create(&repo, "../wt-feature", Some("feature")).expect("create worktree");
    assert!(wt.join(".git").exists() || wt.exists(), "worktree checked out");

    let worktrees = list(&repo).expect("list worktrees");
    assert!(
        worktrees.iter().any(|w| w.branch.as_deref() == Some("feature")),
        "created branch appears in the worktree list: {worktrees:?}"
    );

    remove(&repo, "../wt-feature").expect("remove worktree");
    let after = list(&repo).expect("list worktrees after remove");
    assert!(
        !after.iter().any(|w| w.branch.as_deref() == Some("feature")),
        "removed worktree is gone: {after:?}"
    );

    let _ = std::fs::remove_dir_all(&base);
}
