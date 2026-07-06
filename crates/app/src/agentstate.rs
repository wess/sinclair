//! An agent's self-reported semantic work state, surfaced as a tab and sidebar
//! status dot. Reported by an agent's hooks over the single-instance socket (see
//! `agenthooks.rs`) and mirrored from the mesh vocabulary used by the `relay`
//! `report_status` tool.

/// The four semantic states an agent can be in, "who's blocked / working / done
/// / idle" at a glance across the whole session.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AgentState {
    /// Actively doing work (a turn is running).
    Working,
    /// Waiting on the user: a permission prompt, a question, or input.
    Blocked,
    /// Finished its task; awaiting the next instruction.
    Done,
    /// Registered but not doing anything right now.
    Idle,
}

impl AgentState {
    /// Parse a reported status label (case-insensitive), accepting a few common
    /// aliases. Unknown labels return `None` so the caller can clear the state.
    pub fn parse(s: &str) -> Option<AgentState> {
        match s.trim().to_ascii_lowercase().as_str() {
            "working" | "busy" | "running" | "thinking" | "active" => Some(AgentState::Working),
            "blocked" | "waiting" | "input" | "attention" | "permission" => {
                Some(AgentState::Blocked)
            }
            "done" | "finished" | "complete" | "completed" | "stop" => Some(AgentState::Done),
            "idle" | "ready" | "waiting_for_task" => Some(AgentState::Idle),
            _ => None,
        }
    }

    /// The canonical lowercase label.
    pub fn label(self) -> &'static str {
        match self {
            AgentState::Working => "working",
            AgentState::Blocked => "blocked",
            AgentState::Done => "done",
            AgentState::Idle => "idle",
        }
    }

    /// A filled-circle emoji for plain-text surfaces (sidebar rows).
    pub fn glyph(self) -> &'static str {
        match self {
            AgentState::Blocked => "\u{1f534}", // 🔴
            AgentState::Working => "\u{1f7e1}", // 🟡
            AgentState::Done => "\u{1f535}",    // 🔵
            AgentState::Idle => "\u{1f7e2}",    // 🟢
        }
    }

    /// The dot color, as an RGB the caller converts into the gpui/theme space.
    pub fn color(self) -> theme::Rgb {
        match self {
            AgentState::Blocked => theme::Rgb::new(240, 90, 90),
            AgentState::Working => theme::Rgb::new(240, 180, 30),
            AgentState::Done => theme::Rgb::new(80, 150, 240),
            AgentState::Idle => theme::Rgb::new(90, 190, 120),
        }
    }
}

#[cfg(test)]
#[path = "../tests/agentstate.rs"]
mod tests;
