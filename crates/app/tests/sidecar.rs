use super::*;

#[test]
fn mint_token_is_nonempty_hex() {
    let t = mint_token();
    assert_eq!(t.len(), 32);
    assert!(t.chars().all(|c| c.is_ascii_hexdigit()));
    assert_ne!(mint_token(), mint_token());
}

#[test]
fn reserved_port_is_free_for_the_child() {
    // Once a reservation is dropped, the child can bind the port immediately.
    // Parallel tests can legitimately snipe a freed ephemeral port, so allow
    // a few fresh reservations before calling the contract broken.
    let rebound = (0..5).any(|_| {
        let reservation = reserve_port().unwrap();
        let port = reservation.local_addr().unwrap().port();
        assert_ne!(port, 0);
        drop(reservation);
        std::net::TcpListener::bind(("127.0.0.1", port)).is_ok()
    });
    assert!(
        rebound,
        "no reserved port was rebindable across five attempts"
    );
}

#[test]
fn wait_ready_fails_when_the_port_is_squatted_and_the_child_exits() {
    // Someone else owns the port; our child (per the sidecar contract) exits
    // when it can't bind. The boot must fail rather than trust the squatter.
    let squatter = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = squatter.local_addr().unwrap().port();
    let mut child = std::process::Command::new("sh")
        .args(["-c", "sleep 0.2; exit 1"])
        .spawn()
        .unwrap();
    let err = wait_ready(&mut child, port, "tok").unwrap_err();
    assert!(
        err.contains("something else may be listening") || err.contains("exited"),
        "{err}"
    );
}

#[test]
fn acquire_reports_a_service_that_dies_at_startup() {
    let dir = std::env::temp_dir().join("sinclairsidecarexit");
    std::fs::create_dir_all(&dir).unwrap();
    let err = acquire("test.exit", "false", &dir).unwrap_err();
    assert!(err.contains("exited during startup"), "{err}");
    // A failed start takes no reference and leaves nothing registered.
    assert!(!SERVICES.lock().unwrap().contains_key("test.exit"));
}

#[test]
fn acquire_rejects_an_empty_command() {
    let dir = std::env::temp_dir();
    let err = acquire("test.empty", "  ", &dir).unwrap_err();
    assert_eq!(err, "empty service command");
}

#[test]
fn release_of_an_unknown_id_is_a_noop() {
    release("test.never-acquired");
}

#[test]
fn resolve_program_keeps_paths_and_unknown_names() {
    assert_eq!(resolve_program("/usr/bin/env"), "/usr/bin/env");
    assert_eq!(
        resolve_program("definitelynotasibling"),
        "definitelynotasibling"
    );
}

/// A one-shot fake sidecar: answers `/health?challenge=` with the proof that
/// `respond` computes from the (token, nonce) pair, then closes.
fn fake_sidecar(respond: fn(&str) -> Option<String>) -> u16 {
    use std::io::{BufRead, BufReader, Write};
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        // Loop: wait_ready makes a bare liveness connect before the probe,
        // and either may retry; answer every connection until torn down.
        for stream in listener.incoming() {
            let Ok(stream) = stream else { break };
            let mut reader = BufReader::new(match stream.try_clone() {
                Ok(r) => r,
                Err(_) => continue,
            });
            let mut line = String::new();
            if reader.read_line(&mut line).is_err() || line.is_empty() {
                continue;
            }
            let nonce = line
                .split_once("challenge=")
                .and_then(|(_, rest)| rest.split_whitespace().next())
                .unwrap_or_default()
                .to_string();
            let mut stream = stream;
            let _ = match respond(&nonce) {
                Some(proof) => write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nx-sinclair-proof: {proof}\r\ncontent-length: 0\r\n\r\n"
                ),
                None => write!(stream, "HTTP/1.1 200 OK\r\ncontent-length: 0\r\n\r\n"),
            };
        }
    });
    port
}

#[test]
fn handshake_accepts_the_correct_proof() {
    let port = fake_sidecar(|nonce| Some(expected_proof("tok", nonce)));
    let mut child = std::process::Command::new("sleep")
        .arg("5")
        .spawn()
        .unwrap();
    assert_eq!(wait_ready(&mut child, port, "tok"), Ok(()));
    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn handshake_rejects_a_wrong_proof() {
    let port = fake_sidecar(|_| Some("0".repeat(64)));
    let mut child = std::process::Command::new("sleep")
        .arg("5")
        .spawn()
        .unwrap();
    let err = wait_ready(&mut child, port, "tok").unwrap_err();
    assert!(err.contains("readiness handshake"), "{err}");
    let _ = child.wait();
}

#[test]
fn missing_proof_falls_back_to_the_settle_heuristic() {
    // A live child plus a proofless HTTP answer must still pass (legacy path).
    let port = fake_sidecar(|_| None);
    let mut child = std::process::Command::new("sleep")
        .arg("5")
        .spawn()
        .unwrap();
    assert_eq!(wait_ready(&mut child, port, "tok"), Ok(()));
    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn proof_matches_the_notes_server_derivation() {
    // Same construction on both sides: hex(sha256(token ‖ nonce)).
    assert_eq!(
        expected_proof("tok", "nonce"),
        "c53c57297be40c7ca5455250c8934ebb67a78a9afad10a53cce4d23ce7c77781"
    );
}
