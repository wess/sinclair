//! Context extraction and semantic-ish ranking over terminal output.

use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub number: usize,
    pub text: String,
    pub prompt: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub start: usize,
    pub end: usize,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub block: Block,
    pub score: f32,
}

pub fn blocks(lines: &[Line]) -> Vec<Block> {
    let mut out = Vec::new();
    let mut current: Option<Block> = None;
    for line in lines.iter().filter(|line| !line.text.trim().is_empty()) {
        if line.prompt || current.is_none() {
            if let Some(block) = current.take() {
                out.push(block);
            }
            current = Some(Block {
                start: line.number,
                end: line.number,
                text: line.text.clone(),
            });
        } else if let Some(block) = current.as_mut() {
            block.end = line.number;
            if !block.text.is_empty() {
                block.text.push('\n');
            }
            block.text.push_str(&line.text);
        }
    }
    if let Some(block) = current {
        out.push(block);
    }
    out
}

pub fn blocktext(lines: &[Line], start: usize, end: usize) -> String {
    lines
        .iter()
        .filter(|line| line.number >= start && line.number <= end)
        .map(|line| line.text.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn lastblock(lines: &[Line]) -> Option<Block> {
    blocks(lines)
        .into_iter()
        .rev()
        .find(|block| !block.text.trim().is_empty())
}

pub fn search(query: &str, lines: &[Line], limit: usize) -> Vec<Hit> {
    let query = query.trim();
    if query.is_empty() {
        return Vec::new();
    }
    let qvec = vector(query);
    let mut hits: Vec<Hit> = blocks(lines)
        .into_iter()
        .filter_map(|block| {
            let score = cosine(&qvec, &vector(&block.text));
            (score > 0.05).then_some(Hit { block, score })
        })
        .collect();
    hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.block.start.cmp(&a.block.start))
    });
    hits.truncate(limit);
    hits
}

fn vector(text: &str) -> HashMap<String, f32> {
    let mut map = HashMap::new();
    for token in expand(tokens(text)) {
        *map.entry(token).or_insert(0.0) += 1.0;
    }
    map
}

fn tokens(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-' && c != '/')
        .map(str::trim)
        .filter(|s| s.len() > 1)
        .map(str::to_ascii_lowercase)
}

fn expand(tokens: impl Iterator<Item = String>) -> Vec<String> {
    let mut out = Vec::new();
    for token in tokens {
        out.push(token.clone());
        for synonym in synonyms(&token) {
            out.push(synonym.to_string());
        }
        for gram in trigrams(&token) {
            out.push(gram);
        }
    }
    out
}

fn trigrams(token: &str) -> Vec<String> {
    let chars: Vec<char> = token.chars().collect();
    if chars.len() < 4 {
        return Vec::new();
    }
    chars
        .windows(3)
        .map(|w| w.iter().collect::<String>())
        .map(|s| format!("tri:{s}"))
        .collect()
}

fn synonyms(token: &str) -> &'static [&'static str] {
    match token {
        "fail" | "failed" | "failure" | "error" | "err" => {
            &["exception", "panic", "fatal", "denied", "missing"]
        }
        "warn" | "warning" => &["deprecated", "caution"],
        "auth" | "login" => &["token", "credential", "permission", "denied"],
        "test" | "tests" => &["spec", "assert", "coverage"],
        "build" => &["compile", "bundle", "release"],
        "migration" | "migrate" => &["database", "schema", "sql"],
        "network" => &["timeout", "connection", "dns", "proxy"],
        _ => &[],
    }
}

fn cosine(a: &HashMap<String, f32>, b: &HashMap<String, f32>) -> f32 {
    let keys: HashSet<&String> = a.keys().chain(b.keys()).collect();
    let mut dot = 0.0;
    let mut aa = 0.0;
    let mut bb = 0.0;
    for key in keys {
        let x = *a.get(key).unwrap_or(&0.0);
        let y = *b.get(key).unwrap_or(&0.0);
        dot += x * y;
        aa += x * x;
        bb += y * y;
    }
    if aa == 0.0 || bb == 0.0 {
        0.0
    } else {
        dot / (aa.sqrt() * bb.sqrt())
    }
}

#[cfg(test)]
#[path = "../tests/context.rs"]
mod tests;
