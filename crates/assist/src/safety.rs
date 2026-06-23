//! Paste-risk analysis for commands before they reach the pty.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PasteRisk {
    pub level: RiskLevel,
    pub reasons: Vec<String>,
}

impl PasteRisk {
    pub fn risky(&self) -> bool {
        matches!(self.level, RiskLevel::Medium | RiskLevel::High)
    }
}

pub fn analyze(text: &str) -> PasteRisk {
    let lower = text.to_ascii_lowercase();
    let mut reasons = Vec::new();
    if text.lines().filter(|line| !line.trim().is_empty()).count() > 1 {
        reasons.push("multiple commands".to_string());
    }
    for needle in ["rm -rf", "mkfs", "dd if=", ":(){", "chmod -r", "chown -r"] {
        if lower.contains(needle) {
            reasons.push(format!("contains `{needle}`"));
        }
    }
    for needle in ["sudo ", "curl ", "wget ", " | sh", " | bash", "> /dev/"] {
        if lower.contains(needle) {
            reasons.push(format!("contains `{}`", needle.trim()));
        }
    }
    let level = if reasons.iter().any(|r| {
        r.contains("rm -rf")
            || r.contains("mkfs")
            || r.contains("dd if=")
            || r.contains("/dev/")
            || r.contains(":(){")
    }) {
        RiskLevel::High
    } else if reasons.is_empty() {
        RiskLevel::Low
    } else {
        RiskLevel::Medium
    };
    PasteRisk { level, reasons }
}

#[cfg(test)]
#[path = "../tests/safety.rs"]
mod tests;
