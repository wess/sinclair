//! Small-model boundary. Today the helpers are deterministic and fast; the
//! Candle touchpoint keeps the crate ready for a real local generator.

#[cfg(feature = "candle")]
use candle_core::{Device, Tensor};

pub fn explain(text: &str) -> String {
    let text = text.trim();
    if text.is_empty() {
        return "No terminal output was available to explain.".to_string();
    }
    let lower = text.to_ascii_lowercase();
    let mut lines = Vec::new();
    if lower.contains("permission denied") || lower.contains("eacces") {
        lines.push("This looks like a permission or credential failure.");
        lines.push("Check file permissions, login state, tokens, or the target account.");
    } else if lower.contains("not found") || lower.contains("command not found") {
        lines.push("Something referenced by the command was not found.");
        lines.push(
            "Check spelling, PATH, installed tools, and relative paths from this pane's cwd.",
        );
    } else if lower.contains("timed out") || lower.contains("timeout") {
        lines.push("This looks like a network or service timeout.");
        lines.push("Retry after checking connectivity, VPN/proxy state, and service health.");
    } else if lower.contains("failed") || lower.contains("error") || lower.contains("panic") {
        lines.push("The command reported a failure.");
        lines.push("Start with the first error line, then check the command arguments and local environment.");
    } else {
        lines.push("This output does not contain an obvious failure signature.");
        lines.push("Use the exact command, cwd, and the surrounding output to narrow it down.");
    }
    let excerpt = text
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or(text);
    format!("{}\n\nKey line:\n{}", lines.join("\n"), excerpt.trim())
}

pub fn compose(request: &str) -> String {
    let request = request.trim();
    let lower = request.to_ascii_lowercase();
    let command = if lower.contains("large") && lower.contains("file") {
        "find . -type f -size +100M -print"
    } else if lower.contains("modified") && lower.contains("week") {
        "find . -type f -mtime -7 -print"
    } else if lower.contains("port") && lower.contains("listen") {
        "lsof -iTCP -sTCP:LISTEN -n -P"
    } else if lower.contains("disk") && (lower.contains("usage") || lower.contains("space")) {
        "du -sh ./* 2>/dev/null | sort -h"
    } else if lower.contains("git") && lower.contains("changed") {
        "git status --short"
    } else if lower.contains("search") || lower.contains("find text") {
        "rg '<pattern>'"
    } else if lower.contains("test") && lower.contains("bun") {
        "bun test"
    } else {
        request
    };
    command.to_string()
}

#[cfg(feature = "candle")]
pub fn candleprobe() -> bool {
    Tensor::from_vec(vec![1f32, 0f32], (2,), &Device::Cpu)
        .and_then(|tensor| tensor.to_vec1::<f32>())
        .map(|v| v == [1.0, 0.0])
        .unwrap_or(false)
}

#[cfg(not(feature = "candle"))]
pub fn candleprobe() -> bool {
    false
}

#[cfg(test)]
#[path = "../tests/model.rs"]
mod tests;
