use super::*;

fn strs(v: &[&str]) -> Vec<String> {
  v.iter().map(|s| s.to_string()).collect()
}

#[test]
fn parse_flags_image_and_command() {
  let args = parse(&strs(&["--persist", "--name", "box", "debian:12", "bash", "-l"])).unwrap();
  assert_eq!(
    args,
    Args {
      image: "debian:12".into(),
      command: strs(&["bash", "-l"]),
      name: "box".into(),
      persist: true,
    }
  );
}

#[test]
fn parse_names_the_machine_after_the_image() {
  assert_eq!(parse(&strs(&["alpine"])).unwrap().name, "alpine");
}

#[test]
fn parse_rejects_unknown_flags_and_no_image() {
  assert!(parse(&strs(&["--nope", "alpine"])).is_err());
  assert!(parse(&strs(&["--persist"])).is_err());
}

#[test]
fn guest_prefers_the_given_command() {
  let config = json!({"config": {"Cmd": ["/bin/sh"]}});
  let g = guest(&config, &strs(&["bash", "-l"]), "m", "xterm");
  assert_eq!(g["exec"], "bash");
  assert_eq!(g["argv"], json!(["-l"]));
}

#[test]
fn guest_falls_back_to_entrypoint_and_cmd() {
  let config = json!({"config": {"Entrypoint": ["/init"], "Cmd": ["--x"]}});
  let g = guest(&config, &[], "m", "xterm");
  assert_eq!(g["exec"], "/init");
  assert_eq!(g["argv"], json!(["--x"]));
}

#[test]
fn guest_defaults_to_sh() {
  assert_eq!(guest(&json!({}), &[], "m", "xterm")["exec"], "/bin/sh");
}

#[test]
fn guest_env_keeps_image_values_and_fills_defaults() {
  let config = json!({"config": {"Env": ["PATH=/custom", "LANG=C.UTF-8"]}});
  let g = guest(&config, &[], "box", "xterm-256color");
  let env: Vec<&str> = g["env"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
  assert!(env.contains(&"PATH=/custom"));
  assert!(env.contains(&"LANG=C.UTF-8"));
  assert!(env.contains(&"HOME=/root"));
  assert!(env.contains(&"TERM=xterm-256color"));
  assert!(env.contains(&"HOSTNAME=box"));
  assert_eq!(env.iter().filter(|e| e.starts_with("PATH=")).count(), 1);
}

#[test]
fn guest_workdir_from_image_or_root_home() {
  assert_eq!(guest(&json!({"config": {"WorkingDir": "/app"}}), &[], "m", "t")["workdir"], "/app");
  assert_eq!(guest(&json!({}), &[], "m", "t")["workdir"], "/root");
}
