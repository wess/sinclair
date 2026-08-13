use super::*;

#[test]
fn composes_common_commands() {
    assert_eq!(
        compose_match("show listening ports").as_deref(),
        Some("lsof -iTCP -sTCP:LISTEN -n -P")
    );
    assert_eq!(compose_match("run bun tests").as_deref(), Some("bun test"));
}

#[test]
fn compose_match_makes_a_miss_explicit() {
    assert_eq!(compose_match("write my novel"), None);
    assert_eq!(compose_match(""), None);
}

#[test]
fn compose_shim_echoes_a_miss() {
    assert_eq!(
        compose("show listening ports"),
        "lsof -iTCP -sTCP:LISTEN -n -P"
    );
    assert_eq!(compose("  write my novel "), "write my novel");
}

#[test]
fn explains_permission_errors() {
    assert!(explain("permission denied").contains("permission"));
}

#[cfg(feature = "candle")]
#[test]
fn candle_backend_is_available() {
    assert!(candleprobe());
}

#[cfg(not(feature = "candle"))]
#[test]
fn candle_probe_reports_absence() {
    assert!(!candleprobe());
}
