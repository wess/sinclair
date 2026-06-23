use super::*;

/// Read from the master until EOF or EIO (Linux reports EIO once the
/// child side of the pty is gone).
fn read_to_end(pty: &Pty) -> String {
    let mut out = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        match pty.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => out.extend_from_slice(&buf[..n]),
            Err(e) if e.raw_os_error() == Some(rustix::io::Errno::IO.raw_os_error()) => break,
            Err(e) => panic!("read failed: {e}"),
        }
    }
    String::from_utf8_lossy(&out).replace('\r', "")
}

#[test]
fn echo_hello_round_trips() {
    let opts = SpawnOptions {
        winsize: Winsize::new(20, 5),
        ..SpawnOptions::command(vec!["/bin/echo".to_string(), "hello".to_string()])
    };
    let mut pty = Pty::spawn(&opts).expect("spawn echo");
    assert!(pty.child_pid() > 0);
    let output = read_to_end(&pty);
    let status = pty.wait().expect("wait echo");
    assert!(status.success());
    assert!(output.contains("hello"), "output was: {output:?}");
}

#[test]
fn stty_reports_initial_winsize() {
    let opts = SpawnOptions {
        winsize: Winsize::new(80, 24),
        ..SpawnOptions::command(vec![
            "/bin/sh".to_string(),
            "-c".to_string(),
            "stty size".to_string(),
        ])
    };
    let mut pty = Pty::spawn(&opts).expect("spawn stty");
    let output = read_to_end(&pty);
    let status = pty.wait().expect("wait stty");
    assert!(status.success());
    assert!(output.contains("24 80"), "output was: {output:?}");
}

#[test]
fn write_reaches_child_stdin() {
    let opts = SpawnOptions {
        winsize: Winsize::new(80, 24),
        ..SpawnOptions::command(vec![
            "/bin/sh".to_string(),
            "-c".to_string(),
            "read line; echo got:$line".to_string(),
        ])
    };
    let mut pty = Pty::spawn(&opts).expect("spawn reader");
    pty.write(b"ping\n").expect("write to pty");
    let output = read_to_end(&pty);
    pty.wait().expect("wait reader");
    assert!(output.contains("got:ping"), "output was: {output:?}");
}

#[test]
fn resize_is_visible_to_child() {
    let opts = SpawnOptions {
        winsize: Winsize::new(80, 24),
        ..SpawnOptions::command(vec![
            "/bin/sh".to_string(),
            "-c".to_string(),
            "sleep 1; stty size".to_string(),
        ])
    };
    let mut pty = Pty::spawn(&opts).expect("spawn sleeper");
    pty.resize(Winsize::new(132, 43)).expect("resize");
    let output = read_to_end(&pty);
    pty.wait().expect("wait sleeper");
    assert!(output.contains("43 132"), "output was: {output:?}");
}

#[test]
fn kill_terminates_child() {
    let opts = SpawnOptions::command(vec![
        "/bin/sh".to_string(),
        "-c".to_string(),
        "sleep 30".to_string(),
    ]);
    let mut pty = Pty::spawn(&opts).expect("spawn sleeper");
    pty.kill().expect("kill child");
    let status = pty.wait().expect("wait killed child");
    assert!(!status.success());
}

#[test]
fn cloned_reader_reads_output() {
    use std::io::Read;
    let opts = SpawnOptions::command(vec!["/bin/echo".to_string(), "clone".to_string()]);
    let pty = Pty::spawn(&opts).expect("spawn echo");
    let mut reader = pty.try_clone_reader().expect("clone reader");
    let mut out = Vec::new();
    let mut buf = [0u8; 1024];
    loop {
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => out.extend_from_slice(&buf[..n]),
            Err(_) => break,
        }
    }
    let text = String::from_utf8_lossy(&out).replace('\r', "");
    assert!(text.contains("clone"), "output was: {text:?}");
}
