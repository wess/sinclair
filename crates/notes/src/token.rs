//! Per-session auth token for the vault API.
//!
//! The server binds loopback TCP, which is *not* per-user isolated — any local
//! user can connect. Without a gate, the vault API (arbitrary file read/write
//! under a caller-chosen root) is reachable cross-user. So every `/api` and
//! `/ws` request must carry a bearer token minted per server session and shared
//! with the in-app web client via a 0600 file only our uid can read.

use std::io::Read;
use std::path::PathBuf;

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
}

fn config_dir() -> PathBuf {
    home().join(".config").join("sinclair").join("notes")
}

/// Where `{ port, pid, token }` is recorded for the app to read back.
pub fn info_path() -> PathBuf {
    config_dir().join("server.json")
}

/// A fresh 128-bit token as lowercase hex, from the OS CSPRNG.
pub fn mint() -> String {
    let mut buf = [0u8; 16];
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        if f.read_exact(&mut buf).is_ok() {
            return hex(&buf);
        }
    }
    // /dev/urandom should never fail on a Unix host; mix time+pid as a last
    // resort so we never hand out an empty (i.e. unguarded) token.
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
        ^ (std::process::id() as u128);
    hex(&n.to_le_bytes())
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        s.push(DIGITS[(b >> 4) as usize] as char);
        s.push(DIGITS[(b & 0xf) as usize] as char);
    }
    s
}

/// Record `{ port, pid, token }` at [`info_path`], readable only by us (0600).
/// Called only by the instance that won the port bind, so a losing duplicate
/// never clobbers the live server's token.
pub fn write_info(port: u16, token: &str) {
    let _ = std::fs::create_dir_all(config_dir());
    let body =
        serde_json::json!({ "port": port, "pid": std::process::id(), "token": token }).to_string();
    let _ = write_private(&info_path(), body.as_bytes());
}

/// Write `body` at `path` readable only by us. The file is born 0600 (the mode
/// is set at creation, not chmodded after) so there is no window where another
/// user can read the token; any existing file is removed first so a stale
/// wider mode can't carry over.
fn write_private(path: &std::path::Path, body: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let _ = std::fs::remove_file(path);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)?.write_all(body)
}

/// Length-independent comparison so a wrong token leaks no timing signal.
pub fn constant_time_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b) {
        diff |= x ^ y;
    }
    diff == 0
}

/// The bearer token presented on a request: `Authorization: Bearer <t>` or a
/// `?token=<t>` query param (WebSocket upgrades can't set headers).
pub fn token_from_query(query: &str) -> Option<&str> {
    query.split('&').find_map(|kv| kv.strip_prefix("token="))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_time_eq_matches_only_identical() {
        assert!(constant_time_eq("abc123", "abc123"));
        assert!(!constant_time_eq("abc123", "abc124"));
        assert!(!constant_time_eq("abc", "abc123")); // length mismatch
        assert!(!constant_time_eq("", "x"));
    }

    #[test]
    fn mint_is_nonempty_hex() {
        let t = mint();
        assert_eq!(t.len(), 32);
        assert!(t.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(mint(), mint()); // fresh each call
    }

    #[cfg(unix)]
    #[test]
    fn write_private_is_owner_only_from_birth() {
        use std::os::unix::fs::PermissionsExt;
        let path = std::env::temp_dir().join(format!("sinclairtoken{}", std::process::id()));
        // Even a stale world-readable file must come out 0600, not chmod-raced.
        std::fs::write(&path, b"stale").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();

        write_private(&path, b"{\"token\":\"t\"}").unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        assert_eq!(std::fs::read(&path).unwrap(), b"{\"token\":\"t\"}");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn token_from_query_extracts_param() {
        assert_eq!(token_from_query("token=deadbeef"), Some("deadbeef"));
        assert_eq!(token_from_query("a=1&token=abc&b=2"), Some("abc"));
        assert_eq!(token_from_query("a=1&b=2"), None);
        assert_eq!(token_from_query(""), None);
    }
}
