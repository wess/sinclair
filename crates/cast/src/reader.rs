//! Reading `.cast` files back into events - the inverse of [`crate::Recorder`].
//!
//! An asciinema v2 file is a JSON object header line followed by one JSON array
//! per event (`[time, code, data]`). We parse the header we care about and every
//! event, classifying the one-letter code. Malformed lines are skipped rather
//! than aborting the load, so a truncated recording still replays what it has.

use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::Path;

/// The recording header: geometry plus optional title and unix timestamp.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub width: usize,
    pub height: usize,
    pub title: Option<String>,
    pub timestamp: Option<u64>,
}

/// The kind of an event, from its one-letter asciinema code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    /// `"o"` - data written to the terminal (what you see). The only kind we
    /// replay when rendering.
    Output,
    /// `"i"` - data typed by the user.
    Input,
    /// `"r"` - a resize, with data like `"80x24"`.
    Resize,
    /// `"m"` - a named marker.
    Marker,
    /// Any other, unrecognized code.
    Other,
}

impl EventKind {
    fn from_code(code: &str) -> EventKind {
        match code {
            "o" => EventKind::Output,
            "i" => EventKind::Input,
            "r" => EventKind::Resize,
            "m" => EventKind::Marker,
            _ => EventKind::Other,
        }
    }
}

/// One recorded event at `time` seconds from the start.
#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub time: f64,
    pub kind: EventKind,
    pub data: String,
}

/// A parsed recording: its header and every event in file order.
#[derive(Debug, Clone, PartialEq)]
pub struct Cast {
    pub header: Header,
    pub events: Vec<Event>,
}

impl Cast {
    /// Parse a `.cast` file from disk.
    pub fn read(path: impl AsRef<Path>) -> io::Result<Cast> {
        let file = File::open(path)?;
        parse(BufReader::new(file))
    }

    /// Output events only, in order - the stream a renderer replays.
    pub fn output(&self) -> impl Iterator<Item = &Event> {
        self.events.iter().filter(|e| e.kind == EventKind::Output)
    }
}

/// Parse a recording from any reader. The first non-blank line must be the
/// header object; every line after it is treated as an event array and skipped
/// if it does not parse.
pub fn parse<R: BufRead>(reader: R) -> io::Result<Cast> {
    let mut lines = reader.lines();

    let header = loop {
        match lines.next() {
            Some(line) => {
                let line = line?;
                if line.trim().is_empty() {
                    continue;
                }
                break parse_header(&line)?;
            }
            None => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "cast file is empty",
                ))
            }
        }
    };

    let mut events = Vec::new();
    for line in lines {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if let Some(event) = parse_event(&line) {
            events.push(event);
        }
    }

    Ok(Cast { header, events })
}

fn parse_header(line: &str) -> io::Result<Header> {
    let value: serde_json::Value = serde_json::from_str(line)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("bad header: {e}")))?;
    let width = value["width"].as_u64().unwrap_or(80) as usize;
    let height = value["height"].as_u64().unwrap_or(24) as usize;
    let title = value["title"].as_str().map(str::to_owned);
    let timestamp = value["timestamp"].as_u64();
    Ok(Header {
        width: width.max(1),
        height: height.max(1),
        title,
        timestamp,
    })
}

fn parse_event(line: &str) -> Option<Event> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    let arr = value.as_array()?;
    let time = arr.first()?.as_f64()?;
    let code = arr.get(1)?.as_str()?;
    let data = arr.get(2)?.as_str()?.to_owned();
    Some(Event {
        time,
        kind: EventKind::from_code(code),
        data,
    })
}

#[cfg(test)]
#[path = "../tests/reader.rs"]
mod tests;
