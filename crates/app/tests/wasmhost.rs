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

fn get(url: &str) -> HttpRequest {
    HttpRequest {
        url: url.to_string(),
        method: "GET".to_string(),
        headers: Vec::new(),
        body: None,
    }
}

#[test]
fn fetch_refuses_plaintext_and_non_http_schemes() {
    assert!(fetch(get("http://example.com")).is_err());
    assert!(fetch(get("file:///etc/passwd")).is_err());
    assert!(fetch(get("ftp://example.com")).is_err());
}

/// A header carrying CR/LF could inject a second header or a whole request, so
/// it is rejected rather than sanitized.
#[test]
fn fetch_rejects_header_injection() {
    for (name, value) in [
        ("X-A\r\nX-Evil", "1"),
        ("X-A", "1\r\nX-Evil: 2"),
        ("X-A\nX-Evil", "1"),
        ("X-A: b", "1"),
        ("", "1"),
    ] {
        let mut r = get("https://example.com");
        r.headers = vec![(name.to_string(), value.to_string())];
        assert!(fetch(r).is_err(), "accepted header {name:?}: {value:?}");
    }
}

#[test]
fn fetch_rejects_a_bad_method() {
    let mut r = get("https://example.com");
    r.method = "GET /x HTTP/1.1".to_string();
    assert!(fetch(r).is_err());
}

/// Following redirects leaves several header blocks in the dump; the last one
/// is the response the plugin actually got.
#[test]
fn final_status_takes_the_last_hop() {
    let dump = "HTTP/2 301\r\nlocation: https://b\r\n\r\nHTTP/2 200\r\ncontent-type: application/json\r\n\r\n";
    assert_eq!(final_status(dump), 200);
    assert_eq!(final_status("HTTP/1.1 404 Not Found\r\n\r\n"), 404);
    assert_eq!(final_status(""), 0);
}

#[test]
fn headers_come_from_the_last_hop_lowercased() {
    let dump = "HTTP/2 301\r\nLocation: https://b\r\n\r\nHTTP/2 200\r\nContent-Type: application/json\r\nETag: \"x\"\r\n\r\n";
    let headers = parse_headers(dump);
    assert!(headers.contains(&("content-type".into(), "application/json".into())));
    assert!(headers.contains(&("etag".into(), "\"x\"".into())));
    // The redirect hop's headers are not the response's.
    assert!(!headers.iter().any(|(k, _)| k == "location"), "{headers:?}");
}

#[test]
fn scoped_rejects_escapes() {
    let root = std::path::Path::new("/tmp/plug");
    assert!(scoped(root, "notes.txt").is_ok());
    assert!(scoped(root, "../etc/passwd").is_err());
    assert!(scoped(root, "/etc/passwd").is_err());
}

/// `exec` is driven straight from a panel render on the UI thread, so it must
/// not linger once the program has finished. The watchdog used to be a polling
/// sleeper that could only be joined when it next woke, which held the caller
/// for up to a poll interval after the work was already done — four execs in a
/// panel refresh meant a visible stall.
#[test]
fn exec_returns_promptly_once_the_program_exits() {
    let start = std::time::Instant::now();
    for _ in 0..8 {
        exec(req("true", &[]), None).expect("true runs");
    }
    let each = start.elapsed() / 8;
    assert!(
        each < Duration::from_millis(25),
        "each exec took {each:?}; the watchdog is holding the caller after exit"
    );
}
