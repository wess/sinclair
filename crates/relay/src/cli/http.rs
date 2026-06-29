use anyhow::{anyhow, Result};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::time::Duration;

/// Minimal blocking HTTP/1.1 client for localhost control calls, avoids pulling
/// in a full HTTP client crate for a few JSON round-trips.
pub fn get(addr: &str, path: &str) -> Result<String> {
    request(addr, "GET", path, None, 35)
}

/// Open a long-lived GET (no read timeout) and return a reader positioned at the
/// response body, for streaming endpoints like `/control/events` (SSE). The
/// caller reads body lines until the connection closes.
pub fn open_stream(addr: &str, path: &str) -> Result<BufReader<TcpStream>> {
    let stream =
        TcpStream::connect(addr).map_err(|e| anyhow!("cannot reach server at {addr}: {e}"))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    let req = format!(
        "GET {path} HTTP/1.1\r\nHost: {addr}\r\n{}Accept: text/event-stream\r\n\r\n",
        auth_header(addr)
    );
    let mut stream = stream;
    stream.write_all(req.as_bytes())?;
    stream.flush()?;

    let mut reader = BufReader::new(stream);
    // Consume the status line + headers, stopping at the blank separator.
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return Err(anyhow!("server closed before response"));
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
    }
    Ok(reader)
}

/// `Authorization: Bearer …\r\n` for `addr`, or empty when no matching token.
fn auth_header(addr: &str) -> String {
    let norm = |a: &str| a.replace("0.0.0.0", "127.0.0.1");
    super::paths::read_info()
        .ok()
        .filter(|i| norm(&i.addr) == norm(addr) && !i.token.is_empty())
        .map(|i| format!("Authorization: Bearer {}\r\n", i.token))
        .unwrap_or_default()
}

pub fn post(addr: &str, path: &str, body: &str) -> Result<String> {
    request(addr, "POST", path, Some(body), 35)
}

/// POST with an explicit read timeout (seconds), for slow backends like Ollama.
pub fn post_timeout(addr: &str, path: &str, body: &str, secs: u64) -> Result<String> {
    request(addr, "POST", path, Some(body), secs)
}

fn request(addr: &str, method: &str, path: &str, body: Option<&str>, read_secs: u64) -> Result<String> {
    let mut stream = TcpStream::connect(addr)
        .map_err(|e| anyhow!("cannot reach server at {addr}: {e}"))?;
    stream.set_read_timeout(Some(Duration::from_secs(read_secs)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;

    let body = body.unwrap_or("");
    let auth = auth_header(addr);
    let req = format!(
        "{method} {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n{auth}\
         Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(req.as_bytes())?;
    stream.flush()?;

    let mut raw = Vec::new();
    stream.read_to_end(&mut raw)?;
    let text = String::from_utf8_lossy(&raw);
    match text.split_once("\r\n\r\n") {
        Some((_, body)) => Ok(body.to_string()),
        None => Ok(String::new()),
    }
}
