use super::*;

#[test]
fn composes_common_commands() {
    assert_eq!(
        compose("show listening ports"),
        "lsof -iTCP -sTCP:LISTEN -n -P"
    );
    assert_eq!(compose("run bun tests"), "bun test");
}

#[test]
fn explains_permission_errors() {
    assert!(explain("permission denied").contains("permission"));
}

#[test]
fn candle_backend_is_available() {
    assert!(candleprobe());
}
