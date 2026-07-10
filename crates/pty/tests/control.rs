use super::*;
use crate::{Pty, SpawnOptions};

fn command(args: &[&str]) -> SpawnOptions {
    SpawnOptions::command(args.iter().map(|s| s.to_string()).collect())
}

#[test]
fn resize_is_visible_to_child() {
    let mut pty = Pty::spawn(&command(&["/bin/sh", "-c", "sleep 1; stty size"])).expect("spawn");
    let control = pty.control().expect("control");
    control.resize(Winsize::new(132, 43)).expect("resize");
    let (pump, _waker) = pty.pump().expect("pump");
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
            Err(_) => break,
        }
    }
    pty.wait().expect("wait");
    let output = String::from_utf8_lossy(&out).replace('\r', "");
    assert!(output.contains("43 132"), "output was: {output:?}");
}

#[test]
fn resize_carries_pixel_dimensions() {
    let pty = Pty::spawn(&command(&["/bin/sh", "-c", "sleep 30"])).expect("spawn");
    let control = pty.control().expect("control");
    control
        .resize(Winsize::with_cell_size(100, 40, 7, 15))
        .expect("resize");
    let got = rustix::termios::tcgetwinsize(&control.master).expect("get winsize");
    assert_eq!(got.ws_col, 100);
    assert_eq!(got.ws_row, 40);
    assert_eq!(got.ws_xpixel, 700);
    assert_eq!(got.ws_ypixel, 600);
}

#[test]
fn foreground_running_false_for_bare_child() {
    // A lone long-lived child is itself the terminal's foreground group, so
    // there is no process "beyond the shell" to warn about.
    let pty = Pty::spawn(&SpawnOptions::command(vec!["/bin/cat".to_string()])).expect("spawn cat");
    let control = pty.control().expect("control");
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert!(!control.foreground_running());
}

#[test]
fn hangup_terminates_the_group() {
    let mut pty = Pty::spawn(&command(&["/bin/sh", "-c", "sleep 30"])).expect("spawn");
    let control = pty.control().expect("control");
    control.hangup().expect("hangup");
    let status = pty.wait().expect("wait");
    assert!(!status.success());
}

#[test]
fn kill_terminates_a_hangup_ignoring_group() {
    let mut pty =
        Pty::spawn(&command(&["/bin/sh", "-c", "trap '' HUP; sleep 30"])).expect("spawn");
    let control = pty.control().expect("control");
    std::thread::sleep(std::time::Duration::from_millis(100)); // let the trap install
    control.hangup().expect("hangup");
    control.kill().expect("kill");
    let status = pty.wait().expect("wait");
    assert!(!status.success());
}
