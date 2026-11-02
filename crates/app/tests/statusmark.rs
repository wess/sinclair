use super::*;

fn record(state: State, kind: Option<Kind>, progress: Option<u8>) -> Record {
  Record {
    id: String::new(),
    state,
    kind,
    progress,
    app: None,
    title: None,
    msg: None,
    seq: 0,
  }
}

#[test]
fn records_map_to_signals() {
  assert_eq!(
    Signal::from_record(&record(State::Blocked, Some(Kind::Auth), None)),
    Signal::Blocked(Some(Kind::Auth))
  );
  assert_eq!(
    Signal::from_record(&record(State::Working, None, Some(40))),
    Signal::Working(Some(40))
  );
  assert_eq!(Signal::from_agent(AgentState::Done), Signal::Done);
}

#[test]
fn each_blocked_kind_has_its_own_glyph() {
  let glyphs: Vec<_> = [Kind::Permission, Kind::Question, Kind::Auth]
    .into_iter()
    .map(|k| Signal::Blocked(Some(k)).glyph())
    .collect();
  assert_eq!(glyphs, vec![Some("!"), Some("?"), Some("*")]);
  assert_eq!(Signal::Working(None).glyph(), None);
  assert_eq!(Signal::Error.glyph(), Some("x"));
}

#[test]
fn urgency_orders_blocked_first() {
  let mut s = [
    Signal::Idle,
    Signal::Done,
    Signal::Working(None),
    Signal::Error,
    Signal::Blocked(None),
  ];
  s.sort_by_key(|s| std::cmp::Reverse(s.urgency()));
  assert_eq!(s[0], Signal::Blocked(None));
  assert_eq!(s[4], Signal::Idle);
}

#[test]
fn labels_read_naturally() {
  assert_eq!(Signal::Working(Some(40)).label(), "working 40%");
  assert_eq!(
    Signal::Blocked(Some(Kind::Permission)).label(),
    "blocked \u{00b7} permission"
  );
}

#[test]
fn describe_strips_invisible_formatting() {
  assert_eq!(
    describe(None, Some("deploy"), Some("ok\u{202E}gnp")),
    "deploy: okgnp"
  );
  assert_eq!(describe(Some("EU"), Some("deploy"), None), "EU");
  assert_eq!(describe(None, None, None), "");
}

#[test]
fn inbox_groups_then_ranks_then_keeps_tab_order() {
  let signals = [
    Signal::Idle,
    Signal::Working(None),
    Signal::Done,
    Signal::Blocked(None),
    Signal::Error,
    Signal::Done,
  ];
  assert_eq!(inbox(&signals), vec![3, 4, 2, 5, 1, 0]);
  assert_eq!(Bucket::of(Signal::Done), Bucket::NeedsYou);
}
