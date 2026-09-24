use super::*;

#[test]
fn argv_runs_the_hidden_vm_mode() {
  assert_eq!(
    argv("/Apps/sinclair", "debian:latest", "bash -l", true, "sinclair-debian-1"),
    vec![
      "/Apps/sinclair",
      "_vm",
      "run",
      "--persist",
      "--name",
      "sinclair-debian-1",
      "debian:latest",
      "bash",
      "-l",
    ]
  );
}

#[test]
fn argv_round_trips_through_the_parser() {
  let argv = argv("/x", "alpine", "sh", false, "a");
  let parsed = run::parse(&argv[3..]).unwrap();
  assert_eq!(parsed.image, "alpine");
  assert_eq!(parsed.command, vec!["sh"]);
  assert!(!parsed.persist);
}
