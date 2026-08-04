use super::*;

fn req(program: &str, args: &[&str]) -> ExecRequest {
    ExecRequest {
        program: program.to_string(),
        args: args.iter().map(|a| a.to_string()).collect(),
        cwd: None,
    }
}

#[test]
fn exec_captures_stdout_and_status() {
    let out = exec(req("echo", &["hello"]), None).expect("echo runs");
    assert_eq!(out.status, 0);
    assert_eq!(out.stdout.trim(), "hello");
    assert!(out.stderr.is_empty());
}

/// A non-zero exit is data, not an error: `git status` in a non-repo tells the
/// plugin something, so the host must hand back the status rather than
/// collapsing it into `Err`.
#[test]
fn exec_reports_failure_status_without_erroring() {
    let out = exec(req("sh", &["-c", "echo oops >&2; exit 3"]), None).expect("sh runs");
    assert_eq!(out.status, 3);
    assert_eq!(out.stderr.trim(), "oops");
}

/// Args are passed to the program directly, never through a shell — so a value
/// carrying shell syntax stays one literal argument.
#[test]
fn exec_does_not_interpret_args_as_shell() {
    let out = exec(req("echo", &["a; rm -rf /", "$HOME", "`id`"]), None).unwrap();
    assert_eq!(out.stdout.trim(), "a; rm -rf / $HOME `id`");
}

#[test]
fn exec_runs_in_the_given_directory() {
    let dir = std::env::temp_dir().canonicalize().unwrap();
    let mut r = req("pwd", &[]);
    r.cwd = Some(dir.to_string_lossy().into_owned());
    let out = exec(r, None).unwrap();
    assert_eq!(
        std::path::Path::new(out.stdout.trim()).canonicalize().unwrap(),
        dir
    );
}

/// With no cwd in the request, the host's default applies.
#[test]
fn exec_falls_back_to_the_default_directory() {
    let dir = std::env::temp_dir().canonicalize().unwrap();
    let out = exec(req("pwd", &[]), Some(dir.clone())).unwrap();
    assert_eq!(
        std::path::Path::new(out.stdout.trim()).canonicalize().unwrap(),
        dir
    );
}

#[test]
fn exec_reports_a_missing_program() {
    let err = exec(req("definitely-not-a-real-program-xyz", &[]), None).unwrap_err();
    assert!(err.contains("definitely-not-a-real-program-xyz"), "{err}");
    assert!(err.contains("PATH"), "{err}");
}

#[test]
fn exec_rejects_an_empty_program() {
    assert!(exec(req("  ", &[]), None).is_err());
}

/// Output past the cap is truncated rather than handed to the guest whole.
#[test]
fn exec_clamps_huge_output() {
    let out = exec(
        req("sh", &["-c", &format!("yes x | head -c {}", EXEC_MAX_OUTPUT * 2)]),
        None,
    )
    .unwrap();
    assert_eq!(out.stdout.len(), EXEC_MAX_OUTPUT);
}

/// A program that never exits is killed at the deadline instead of pinning the
/// caller forever. `status` is -1 because a signal, not an exit code, ended it.
#[test]
fn exec_kills_a_program_that_overruns() {
    let start = std::time::Instant::now();
    let out = exec(req("sh", &["-c", "sleep 600"]), None).expect("spawns");
    assert_eq!(out.status, -1);
    assert!(
        start.elapsed() < EXEC_TIMEOUT + Duration::from_secs(10),
        "took {:?}",
        start.elapsed()
    );
}

#[test]
fn clamp_cuts_on_a_char_boundary() {
    // A multi-byte char straddling the cap must not be split into a replacement
    // char; the truncation backs up to the boundary instead.
    let mut bytes = vec![b'a'; EXEC_MAX_OUTPUT - 1];
    bytes.extend_from_slice("é".as_bytes()); // two bytes, so it straddles
    let text = clamp(&bytes);
    assert!(!text.contains('\u{fffd}'), "clamped output has a replacement char");
    assert_eq!(text.len(), EXEC_MAX_OUTPUT - 1);
}

#[test]
fn scoped_rejects_escapes() {
    let root = std::path::Path::new("/tmp/plug");
    assert!(scoped(root, "notes.txt").is_ok());
    assert!(scoped(root, "../etc/passwd").is_err());
    assert!(scoped(root, "/etc/passwd").is_err());
}
