//! The pieces of the automation surface that name a pane: resolving a `pane`
//! argument (see [`crate::target`]), listing panes with their ids and status,
//! and recording pane events into the shared [`crate::journal`].

use super::*;
use crate::target;

impl WorkspaceView {
  /// The item a request's `pane` argument names, or the focused item when it
  /// names none.
  pub(crate) fn target_item(&self, args: &Value, cx: &App) -> Result<ItemId, String> {
    let Some(spec) = args.get("pane").and_then(Value::as_str) else {
      return Ok(self.group.read(cx).active_item());
    };
    let items = self.group.read(cx).items();
    let candidates: Vec<target::Candidate> = {
      let map = self.items.borrow();
      items
        .iter()
        .map(|id| target::Candidate {
          token: map.get(id).map_or(0, |it| it.pane_token),
          title: map.get(id).map(|it| it.content.title(cx)).unwrap_or_default(),
        })
        .collect()
    };
    target::resolve(spec, &candidates).map(|i| items[i])
  }

  /// The terminal view behind a request's `pane` argument.
  pub(crate) fn target_terminal(
    &self,
    args: &Value,
    cx: &App,
  ) -> Result<Entity<TerminalView>, String> {
    let item = self.target_item(args, cx)?;
    self
      .items
      .borrow()
      .get(&item)
      .and_then(|it| it.content.as_terminal().cloned())
      .ok_or_else(|| "that pane is not a terminal".to_string())
  }

  /// Whether a pane with this token lives in this window.
  pub(crate) fn has_pane_token(&self, token: u64) -> bool {
    self.items.borrow().values().any(|it| it.pane_token == token)
  }

  /// Every tab in strip order with its id, position, and what it is doing.
  pub(crate) fn list_items(&self, cx: &App) -> Value {
    let group = self.group.read(cx);
    let active = group.active_item();
    let items = group.items();
    let map = self.items.borrow();
    let rows: Vec<Value> = items
      .iter()
      .enumerate()
      .filter_map(|(i, id)| {
        let it = map.get(id)?;
        let status = it.content.status_board(cx);
        let headline = status.headline().map(record_json);
        Some(json!({
          "id": target::id(it.pane_token),
          "index": i + 1,
          "title": it.content.title(cx),
          "cwd": it.content.cwd_path(cx).map(|p| p.to_string_lossy().into_owned()),
          "active": *id == active,
          "running": it.content.has_running_process(cx),
          "signal": it.signal(cx).map(|s| s.label()),
          "status": headline,
          "agent": it.agent.map(|a| a.label()),
        }))
      })
      .collect();
    json!({ "panes": rows })
  }

  /// A pane's status records, for `wait --status` and the `status` op.
  pub(crate) fn pane_status(&self, args: &Value, cx: &App) -> Result<Value, String> {
    let item = self.target_item(args, cx)?;
    let map = self.items.borrow();
    let it = map.get(&item).ok_or("that pane is gone")?;
    let board = it.content.status_board(cx);
    Ok(json!({
      "id": target::id(it.pane_token),
      "signal": it.signal(cx).map(|s| s.label()),
      "state": board.headline().map(|r| r.state.label()),
      "running": it.content.has_running_process(cx),
      "records": board.records().iter().map(record_json).collect::<Vec<_>>(),
    }))
  }

  /// Record a pane event into the shared journal.
  pub(crate) fn journal(&self, item: ItemId, kind: &'static str, data: Value, cx: &mut App) {
    let Some(token) = self.items.borrow().get(&item).map(|it| it.pane_token) else {
      return;
    };
    if token == 0 {
      return;
    }
    if !cx.has_global::<crate::journal::Global>() {
      cx.set_global(crate::journal::Global::default());
    }
    cx.global_mut::<crate::journal::Global>()
      .0
      .push(token, kind, data, crate::journal::now_ms());
  }

  /// The journal entries a client asked for.
  pub(crate) fn events_since(&self, args: &Value, cx: &App) -> Result<Value, String> {
    let pane = match args.get("pane").and_then(Value::as_str) {
      Some(_) => {
        let item = self.target_item(args, cx)?;
        self.items.borrow().get(&item).map(|it| it.pane_token)
      }
      None => None,
    };
    Ok(read_journal(args, pane, cx))
  }
}

/// Journal entries after `args.after` for every pane.
pub(crate) fn journal_since(args: &Value, cx: &App) -> Value {
  read_journal(args, None, cx)
}

fn read_journal(args: &Value, pane: Option<u64>, cx: &App) -> Value {
  let Some(journal) = cx.try_global::<crate::journal::Global>() else {
    return json!({ "events": [], "next": 0, "lost": false });
  };
  let Some(after) = args.get("after").and_then(Value::as_u64) else {
    return json!({ "events": [], "next": journal.0.head(), "lost": false });
  };
  let (events, next, lost) = journal.0.since(after, pane);
  json!({
    "events": events.iter().map(|e| e.to_json()).collect::<Vec<_>>(),
    "next": next,
    "lost": lost,
  })
}

/// A status record as JSON, its text disarmed.
pub(crate) fn record_json(r: &vt::status::Record) -> Value {
  json!({
    "id": r.id,
    "state": r.state.label(),
    "kind": r.kind.map(|k| k.label()),
    "progress": r.progress,
    "app": r.app,
    "title": r.title.as_deref().map(vt::status::disarm),
    "msg": r.msg.as_deref().map(vt::status::disarm),
  })
}
