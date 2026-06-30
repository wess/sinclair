use super::*;

#[test]
fn binary_and_label() {
    assert_eq!(Engine::Docker.binary(), "docker");
    assert_eq!(Engine::Podman.binary(), "podman");
    assert_eq!(Engine::Docker.label(), "Docker");
}

#[test]
fn parse_pref() {
    assert_eq!(Engine::parse("docker"), Some(Engine::Docker));
    assert_eq!(Engine::parse(" Podman "), Some(Engine::Podman));
    assert_eq!(Engine::parse("auto"), None);
    assert_eq!(Engine::parse(""), None);
    assert_eq!(Engine::parse("lxc"), None);
}
