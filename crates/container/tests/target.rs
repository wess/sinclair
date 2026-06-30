use super::*;
use crate::profile::Profile;

fn profile(persist: Option<bool>) -> Profile {
    Profile {
        label: "Debian".to_string(),
        image: "debian:latest".to_string(),
        command: "bash".to_string(),
        persist,
    }
}

#[test]
fn ephemeral_argv() {
    // Ephemeral containers keep a name too, so the tab can remove them on close.
    let t = Target::from_profile(
        Engine::Docker,
        &profile(None),
        false,
        Some("prompt-debian-1".to_string()),
    );
    assert!(!t.persist);
    assert_eq!(t.name.as_deref(), Some("prompt-debian-1"));
    assert_eq!(
        t.argv(),
        vec![
            "docker",
            "run",
            "--rm",
            "-it",
            "--name",
            "prompt-debian-1",
            "debian:latest",
            "bash"
        ]
    );
}

#[test]
fn persistent_argv_named() {
    let t = Target::from_profile(
        Engine::Podman,
        &profile(None),
        true,
        Some("prompt-debian-1".to_string()),
    );
    assert!(t.persist);
    assert_eq!(
        t.argv(),
        vec![
            "podman",
            "run",
            "-it",
            "--name",
            "prompt-debian-1",
            "debian:latest",
            "bash"
        ]
    );
}

#[test]
fn profile_persist_overrides_default() {
    // Profile pins ephemeral even though the default is persist.
    let t = Target::from_profile(Engine::Docker, &profile(Some(false)), true, None);
    assert!(!t.persist);
    assert!(t.argv().contains(&"--rm".to_string()));
}

#[test]
fn multiword_command_splits() {
    let mut p = profile(None);
    p.command = "sh -l".to_string();
    let t = Target::from_profile(Engine::Docker, &p, false, None);
    let argv = t.argv();
    assert_eq!(&argv[argv.len() - 2..], &["sh", "-l"]);
}
