use super::*;
use crate::winsize::Winsize;

/// Read from the pump until EOF or EIO (Linux reports EIO once the
/// child side of the pty is gone).
fn read_to_end(pump: &Pump) -> String {
    let mut out = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        let ready = pump.wait(false).expect("wait");
        if !ready.readable {
            continue;
        }
        match pump.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => out.extend_from_slice(&buf[..n]),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
            Err(e) if e.raw_os_error() == Some(rustix::io::Errno::IO.raw_os_error()) => break,
            Err(e) => panic!("read failed: {e}"),
        }
    }
    String::from_utf8_lossy(&out).replace('\r', "")
}

fn command(args: &[&str]) -> SpawnOptions {
    SpawnOptions::command(args.iter().map(|s| s.to_string()).collect())
}

#[test]
fn echo_hello_round_trips() {
    let opts = SpawnOptions {
        winsize: Winsize::new(20, 5),
        ..command(&["/bin/echo", "hello"])
    };
    let mut pty = Pty::spawn(&opts).expect("spawn echo");
    assert!(pty.child_pid() > 0);
    let (pump, _waker) = pty.pump().expect("pump");
    let output = read_to_end(&pump);
    let status = pty.wait().expect("wait echo");
    assert!(status.success());
    assert!(output.contains("hello"), "output was: {output:?}");
}

#[test]
fn stty_reports_initial_winsize() {
    let opts = SpawnOptions {
        winsize: Winsize::new(80, 24),
        ..command(&["/bin/sh", "-c", "stty size"])
    };
    let mut pty = Pty::spawn(&opts).expect("spawn stty");
    let (pump, _waker) = pty.pump().expect("pump");
    let output = read_to_end(&pump);
    let status = pty.wait().expect("wait stty");
    assert!(status.success());
    assert!(output.contains("24 80"), "output was: {output:?}");
}

#[test]
fn write_reaches_child_stdin() {
    let opts = command(&["/bin/sh", "-c", "read line; echo got:$line"]);
    let mut pty = Pty::spawn(&opts).expect("spawn reader");
    let (pump, _waker) = pty.pump().expect("pump");
    let ready = pump.wait(true).expect("wait writable");
    assert!(ready.writable);
    assert_eq!(pump.write(b"ping\n").expect("write to pty"), 5);
    let output = read_to_end(&pump);
    pty.wait().expect("wait reader");
    assert!(output.contains("got:ping"), "output was: {output:?}");
}

#[test]
fn kill_terminates_child() {
    let mut pty = Pty::spawn(&command(&["/bin/sh", "-c", "sleep 30"])).expect("spawn sleeper");
    pty.kill().expect("kill child");
    let status = pty.wait().expect("wait killed child");
    assert!(!status.success());
}

#[test]
fn drop_kills_and_reaps_the_child() {
    let pty = Pty::spawn(&command(&["/bin/sh", "-c", "sleep 30"])).expect("spawn sleeper");
    let pid = rustix::process::Pid::from_raw(pty.child_pid() as i32).expect("pid");
    drop(pty);
    // Reaped: signal 0 no longer finds the pid.
    assert!(rustix::process::test_kill_process(pid).is_err());
}
