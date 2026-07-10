use super::*;

#[test]
fn open_pair_yields_a_tty_slave() {
    let pair = open_pair().expect("open pty pair");
    assert!(rustix::termios::isatty(&pair.slave));
    assert!(rustix::termios::isatty(&pair.master));
}

#[test]
fn both_fds_are_close_on_exec() {
    // Neither fd may leak into concurrently spawned processes, which would
    // hold the slave open and block EOF on the master forever.
    let pair = open_pair().expect("open pty pair");
    for fd in [&pair.master, &pair.slave] {
        let flags = rustix::io::fcntl_getfd(fd).expect("getfd");
        assert!(flags.contains(FdFlags::CLOEXEC));
    }
}

#[test]
fn winsize_round_trips_through_slave() {
    let pair = open_pair().expect("open pty pair");
    let ws = crate::winsize::Winsize::new(101, 31);
    rustix::termios::tcsetwinsize(&pair.slave, ws.to_termios()).expect("set winsize");
    let got = rustix::termios::tcgetwinsize(&pair.master).expect("get winsize");
    assert_eq!(got.ws_col, 101);
    assert_eq!(got.ws_row, 31);
}

#[test]
fn spawn_child_rejects_empty_argv() {
    let pair = open_pair().expect("open pty pair");
    let opts = SpawnOptions {
        argv: vec![],
        ..SpawnOptions::default()
    };
    let err = spawn_child(&opts, &pair).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
}
