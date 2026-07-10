use super::*;
use crate::{Pty, SpawnOptions};

fn sleeper() -> Pty {
    let opts = SpawnOptions::command(vec![
        "/bin/sh".to_string(),
        "-c".to_string(),
        "sleep 30".to_string(),
    ]);
    Pty::spawn(&opts).expect("spawn sleeper")
}

#[test]
fn waker_interrupts_wait() {
    let pty = sleeper();
    let (pump, waker) = pty.pump().expect("pump");
    let poker = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(50));
        waker.wake();
        waker // keep the write end alive past the wake
    });
    let ready = pump.wait(false).expect("wait");
    assert!(ready.woken);
    assert!(!ready.readable);
    poker.join().expect("join");
}

#[test]
fn idle_pty_is_writable_when_asked() {
    let pty = sleeper();
    let (pump, _waker) = pty.pump().expect("pump");
    let ready = pump.wait(true).expect("wait");
    assert!(ready.writable);
}

#[test]
fn read_would_block_without_output() {
    let pty = sleeper();
    let (pump, _waker) = pty.pump().expect("pump");
    let mut buf = [0u8; 16];
    let err = pump.read(&mut buf).expect_err("no output yet");
    assert_eq!(err.kind(), io::ErrorKind::WouldBlock);
}

#[test]
fn wake_is_sticky_until_waited() {
    let pty = sleeper();
    let (pump, waker) = pty.pump().expect("pump");
    waker.wake();
    waker.wake(); // coalesced into the same pending wake
    let ready = pump.wait(false).expect("wait");
    assert!(ready.woken);
}
