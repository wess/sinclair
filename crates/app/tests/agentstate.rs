use super::*;

#[test]
fn parses_canonical_and_aliases() {
    assert_eq!(AgentState::parse("working"), Some(AgentState::Working));
    assert_eq!(AgentState::parse("BUSY"), Some(AgentState::Working));
    assert_eq!(AgentState::parse(" blocked "), Some(AgentState::Blocked));
    assert_eq!(AgentState::parse("waiting"), Some(AgentState::Blocked));
    assert_eq!(AgentState::parse("done"), Some(AgentState::Done));
    assert_eq!(AgentState::parse("Complete"), Some(AgentState::Done));
    assert_eq!(AgentState::parse("idle"), Some(AgentState::Idle));
    assert_eq!(AgentState::parse("nonsense"), None);
    assert_eq!(AgentState::parse(""), None);
}

#[test]
fn label_round_trips_through_parse() {
    for st in [
        AgentState::Working,
        AgentState::Blocked,
        AgentState::Done,
        AgentState::Idle,
    ] {
        assert_eq!(AgentState::parse(st.label()), Some(st));
    }
}
