//! The `notes` sidecar: a self-contained vault server for Sinclair's Notes
//! surface. Serves an embedded markdown-notes web app plus a vault API over
//! HTTP, with a WebSocket for external-change push. No runtime dependencies
//! (replaces the old Bun server).
//!
//! Two modes:
//!
//! - **Host-managed** — when `SINCLAIR_SERVICE_PORT` and
//!   `SINCLAIR_SERVICE_TOKEN` are set, the host (the app) reserved the port and
//!   minted the token. Bind exactly that port, write no descriptor files, and
//!   let the host own teardown (it SIGTERMs us when the last surface closes; a
//!   parent watch catches a host that died without cleanup).
//! - **Standalone** — `notes serve [PORT]` (or `notes [PORT]`) with no env;
//!   default port 4319, mint a token, publish `server.json`, and reap ourselves
//!   when idle.

mod server;
mod token;
mod vault;

/// The fixed standalone default port.
const DEFAULT_PORT: u16 = 4319;

fn main() {
    // Accept `notes serve [port]` and a bare `notes [port]`.
    let mut args = std::env::args().skip(1).peekable();
    if args.peek().map(String::as_str) == Some("serve") {
        args.next();
    }
    let arg_port: Option<u16> = args.next().and_then(|s| s.parse().ok());

    let hosted = hosted_env(
        std::env::var("SINCLAIR_SERVICE_PORT").ok().as_deref(),
        std::env::var("SINCLAIR_SERVICE_TOKEN").ok().as_deref(),
    );

    let rt = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("notes: runtime: {e}");
            std::process::exit(1);
        }
    };
    match hosted {
        Some((port, token)) => rt.block_on(server::run(port, token, true)),
        None => {
            // Mint the session token up front; the server records it (with the
            // pid) only once it wins the port bind, so a losing duplicate can't
            // overwrite a live server's token file.
            let auth = token::mint();
            rt.block_on(server::run(arg_port.unwrap_or(DEFAULT_PORT), auth, false));
        }
    }
}

/// Host-managed mode: both handoff variables present and usable. The port must
/// be a concrete one the host reserved (0 would put us on a port the host can't
/// know), and the token non-empty (an empty token would leave the API open).
fn hosted_env(port: Option<&str>, token: Option<&str>) -> Option<(u16, String)> {
    let port: u16 = port?.trim().parse().ok().filter(|p| *p != 0)?;
    let token = token?.trim();
    (!token.is_empty()).then(|| (port, token.to_string()))
}

#[cfg(test)]
mod tests {
    use super::hosted_env;

    #[test]
    fn hosted_needs_both_variables() {
        assert_eq!(hosted_env(None, None), None);
        assert_eq!(hosted_env(Some("4321"), None), None);
        assert_eq!(hosted_env(None, Some("deadbeef")), None);
    }

    #[test]
    fn hosted_parses_the_handoff() {
        assert_eq!(
            hosted_env(Some("4321"), Some("deadbeef")),
            Some((4321, "deadbeef".to_string()))
        );
        assert_eq!(
            hosted_env(Some(" 4321 "), Some(" deadbeef ")),
            Some((4321, "deadbeef".to_string()))
        );
    }

    #[test]
    fn hosted_rejects_unusable_values() {
        assert_eq!(hosted_env(Some("0"), Some("deadbeef")), None);
        assert_eq!(hosted_env(Some("nope"), Some("deadbeef")), None);
        assert_eq!(hosted_env(Some("70000"), Some("deadbeef")), None);
        assert_eq!(hosted_env(Some("4321"), Some("")), None);
        assert_eq!(hosted_env(Some("4321"), Some("   ")), None);
    }
}
