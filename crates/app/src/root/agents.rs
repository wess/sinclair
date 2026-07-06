//! Per-pane agent state: applying a hook's status report to the pane it names,
//! and the small read helpers the tab strip and sidebar use to draw the status
//! dot. Reports arrive over the single-instance socket (see `agenthooks.rs`) and
//! are addressed by the pane token injected into each session's environment.

use super::*;

impl WorkspaceView {
    /// Apply an agent status report to the pane bearing `token`, if it lives in
    /// this window. Returns whether the token was found here (so the caller can
    /// stop searching other windows). Sets the semantic state that drives the
    /// tab/sidebar dot and remembers any reported native session id for resume.
    pub(crate) fn apply_agent_report(
        &mut self,
        token: u64,
        state: &str,
        session: Option<&str>,
        cx: &mut Context<Self>,
    ) -> bool {
        if token == 0 {
            return false;
        }
        {
            let mut items = self.items.borrow_mut();
            let Some(it) = items.values_mut().find(|it| it.pane_token == token) else {
                return false;
            };
            it.agent = crate::agentstate::AgentState::parse(state);
            if let Some(s) = session.filter(|s| !s.trim().is_empty()) {
                it.agent_session = Some(s.to_string());
            }
        }
        cx.notify();
        true
    }

    /// Record the launch command for `item` (agent panes) so a restored session
    /// can relaunch — and resume — the agent.
    pub(crate) fn set_item_command(&self, item: ItemId, command: &str) {
        if let Some(it) = self.items.borrow_mut().get_mut(&item) {
            it.command = Some(command.to_string());
        }
    }

    /// This window's panes with their titles and reported agent states, for the
    /// `agent_states` inspect verb.
    pub(crate) fn agent_states(&self, cx: &App) -> Value {
        let states: Vec<Value> = self
            .group
            .read(cx)
            .items()
            .into_iter()
            .filter_map(|id| {
                let items = self.items.borrow();
                let it = items.get(&id)?;
                Some(json!({
                    "title": it.content.title(cx),
                    "state": it.agent.map(|s| s.label()),
                    "session": it.agent_session,
                }))
            })
            .collect();
        json!({ "panes": states })
    }
}
