use super::*;

#[test]
fn resolve_defaults_addr_and_db() {
    let args = ServeArgs {
        addr: None,
        db: None,
    };
    let (addr, db) = resolve(&args);
    assert_eq!(addr, "127.0.0.1:7777");
    assert!(db.ends_with("relay.db"));
}

#[test]
fn split_addr_parses_host_and_numeric_port() {
    assert_eq!(split_addr("127.0.0.1:7777"), Some(("127.0.0.1", 7777)));
    assert_eq!(split_addr("[::1]:7777"), Some(("[::1]", 7777)));
    assert_eq!(split_addr("localhost:0"), Some(("localhost", 0)));
    assert_eq!(split_addr("localhost"), None);
    assert_eq!(split_addr("host:notaport"), None);
}

#[test]
fn health_marker_accepts_current_and_legacy_bodies() {
    assert!(health_marker(HEALTH_BODY));
    assert!(health_marker("relay 0.1.0"));
    assert!(health_marker("ok"));
    assert!(health_marker(" ok \n"));
    // A foreign service's response must not read as a healthy relay.
    assert!(!health_marker("<html>It works!</html>"));
    assert!(!health_marker("404 page not found"));
    assert!(!health_marker(""));
}

#[test]
fn server_record_from_an_older_daemon_still_parses() {
    let json = r#"{"pid": 1, "addr": "127.0.0.1:7777", "db": "relay.db"}"#;
    let info: paths::ServerInfo = serde_json::from_str(json).unwrap();
    assert_eq!(info.addr, "127.0.0.1:7777");
    assert_eq!(info.requested, "");
    assert_eq!(info.token, "");
}

#[tokio::test]
async fn bind_scan_takes_a_free_port_as_asked() {
    let (l, bound) = bind_scan("127.0.0.1:0").await.unwrap();
    let port = l.local_addr().unwrap().port();
    assert_ne!(port, 0);
    assert_eq!(bound, format!("127.0.0.1:{port}"));
}

#[tokio::test]
async fn bind_scan_walks_past_a_taken_port() {
    // Occupy a port, then ask bind_scan for it: it should land nearby instead
    // of failing, and report the address it actually bound.
    let taken = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = taken.local_addr().unwrap().port();
    let (l, bound) = bind_scan(&format!("127.0.0.1:{port}")).await.unwrap();
    let got = l.local_addr().unwrap().port();
    assert_ne!(got, port);
    assert!(got > port && got < port + PORT_SCAN);
    assert_eq!(bound, format!("127.0.0.1:{got}"));
}
