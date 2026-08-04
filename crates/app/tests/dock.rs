use super::*;

/// A resolver over the built-in tokens plus two fake plugin panels, so the
/// index-vs-token distinction can actually be exercised.
fn resolver(installed: &'static [&'static str]) -> impl Fn(&str) -> Option<SidebarPanel> {
    move |token: &str| match token.strip_prefix("plugin:") {
        Some(id) => installed
            .iter()
            .position(|p| *p == id)
            .map(SidebarPanel::Plugin),
        None => SidebarPanel::from_id(token),
    }
}

/// The matching token encoder for `resolver`.
fn tokenizer(installed: &'static [&'static str]) -> impl Fn(SidebarPanel) -> String {
    move |panel: SidebarPanel| match panel {
        SidebarPanel::Plugin(i) => format!("plugin:{}", installed.get(i).copied().unwrap_or("?")),
        other => other.id().to_string(),
    }
}

const NONE: &[&str] = &[];

fn compose_left(tokens: &[&str], collapsed: &[&str]) -> Dock {
    let tokens: Vec<String> = tokens.iter().map(|s| s.to_string()).collect();
    let collapsed: Vec<String> = collapsed.iter().map(|s| s.to_string()).collect();
    compose(
        SidebarSide::Left,
        &tokens,
        &collapsed,
        260.0,
        resolver(NONE),
        tokenizer(NONE),
    )
}

fn docks_of(left: &[SidebarPanel], right: &[SidebarPanel]) -> Docks {
    let mk = |panels: &[SidebarPanel]| Dock {
        sections: panels
            .iter()
            .map(|p| DockSection::new(*p, true))
            .collect(),
        width: 260.0,
        open: false,
    };
    [mk(left), mk(right)]
}

#[test]
fn the_two_sides_default_differently() {
    // The whole point of the overhaul: an untouched install must not show the
    // same dock twice.
    let left = defaults(SidebarSide::Left);
    let right = defaults(SidebarSide::Right);
    assert_ne!(left, right);
    assert!(left.contains(&SidebarPanel::Terminals));
    assert!(right.contains(&SidebarPanel::Agents));
    assert!(left.iter().all(|p| !right.contains(p)), "a section is in both defaults");
}

#[test]
fn empty_config_means_defaults_not_an_empty_dock() {
    let dock = compose_left(&[], &[]);
    let panels: Vec<SidebarPanel> = dock.sections.iter().map(|s| s.panel).collect();
    assert_eq!(panels, defaults(SidebarSide::Left));
}

#[test]
fn an_emptied_dock_stays_empty() {
    // `none` is how the designer records "I removed everything", so it must not
    // fall back to the defaults the way an unset key does.
    let dock = compose_left(&[EMPTY], &[]);
    assert!(dock.sections.is_empty());
}

#[test]
fn config_order_is_dock_order() {
    let dock = compose_left(&["relay", "terminals", "activity"], &[]);
    let panels: Vec<SidebarPanel> = dock.sections.iter().map(|s| s.panel).collect();
    assert_eq!(
        panels,
        [
            SidebarPanel::Relay,
            SidebarPanel::Terminals,
            SidebarPanel::Activity
        ]
    );
}

#[test]
fn collapsed_tokens_start_collapsed() {
    let dock = compose_left(&["terminals", "layouts"], &["layouts"]);
    assert!(dock.sections[0].expanded);
    assert!(!dock.sections[1].expanded);
}

#[test]
fn an_unresolvable_token_is_skipped_not_fatal() {
    // An uninstalled plugin, or a token from a newer version of the app.
    let dock = compose_left(&["terminals", "plugin:gone", "activity"], &[]);
    let panels: Vec<SidebarPanel> = dock.sections.iter().map(|s| s.panel).collect();
    assert_eq!(panels, [SidebarPanel::Terminals, SidebarPanel::Activity]);
}

#[test]
fn a_plugin_section_follows_its_id_not_its_index() {
    // The regression this design exists to prevent: uninstalling the plugin
    // that happened to be first must not silently rebind a saved slot to a
    // different plugin.
    const BEFORE: &[&str] = &["alpha", "beta"];
    const AFTER: &[&str] = &["beta"];

    let tokens = vec!["plugin:beta".to_string()];
    let before = compose(
        SidebarSide::Left,
        &tokens,
        &[],
        260.0,
        resolver(BEFORE),
        tokenizer(BEFORE),
    );
    let after = compose(
        SidebarSide::Left,
        &tokens,
        &[],
        260.0,
        resolver(AFTER),
        tokenizer(AFTER),
    );

    // Different indices either side of the uninstall...
    assert_eq!(before.sections[0].panel, SidebarPanel::Plugin(1));
    assert_eq!(after.sections[0].panel, SidebarPanel::Plugin(0));
    // ...but both still mean the plugin called "beta".
    assert_eq!(tokenizer(BEFORE)(before.sections[0].panel), "plugin:beta");
    assert_eq!(tokenizer(AFTER)(after.sections[0].panel), "plugin:beta");
}

#[test]
fn reveal_finds_a_section_on_the_opposite_side() {
    // `sidebar:left:containers` must keep working once Containers has been
    // moved to the right dock.
    let mut docks = docks_of(&[SidebarPanel::Terminals], &[SidebarPanel::Containers]);
    let out = reveal(&mut docks, SidebarSide::Left, SidebarPanel::Containers);
    assert_eq!(out.side, SidebarSide::Right);
    assert!(out.expanded && !out.added);
    assert!(docks[SidebarSide::Right.index()].open);
    assert!(!docks[SidebarSide::Left.index()].open, "the wrong dock was opened");
}

#[test]
fn revealing_an_open_expanded_section_collapses_it() {
    let mut docks = docks_of(&[SidebarPanel::Terminals], &[]);
    docks[0].open = true;
    let out = reveal(&mut docks, SidebarSide::Left, SidebarPanel::Terminals);
    assert!(!out.expanded, "a second press should collapse");
    assert!(!docks[0].sections[0].expanded);
}

#[test]
fn revealing_into_a_closed_dock_opens_rather_than_toggling_off() {
    // The dock is closed but the section is marked expanded from a prior
    // session; a keypress here means "show me", not "toggle off".
    let mut docks = docks_of(&[SidebarPanel::Terminals], &[]);
    assert!(!docks[0].open);
    let out = reveal(&mut docks, SidebarSide::Left, SidebarPanel::Terminals);
    assert!(out.expanded);
    assert!(docks[0].open);
}

#[test]
fn revealing_an_unplaced_section_adds_it_to_the_named_side() {
    let mut docks = docks_of(&[SidebarPanel::Terminals], &[SidebarPanel::Agents]);
    let out = reveal(&mut docks, SidebarSide::Right, SidebarPanel::Notes);
    assert!(out.added);
    assert_eq!(out.side, SidebarSide::Right);
    assert!(docks[SidebarSide::Right.index()].holds(SidebarPanel::Notes));
}

#[test]
fn toggling_a_side_open_expands_something() {
    // Opening a dock whose sections are all collapsed would show a stack of
    // headers and nothing else.
    let mut docks = docks_of(&[SidebarPanel::Terminals, SidebarPanel::Layouts], &[]);
    docks[0].sections.iter_mut().for_each(|s| s.expanded = false);
    toggle_side(&mut docks, SidebarSide::Left);
    assert!(docks[0].open);
    assert!(docks[0].sections[0].expanded);
    toggle_side(&mut docks, SidebarSide::Left);
    assert!(!docks[0].open);
}

#[test]
fn move_to_transfers_between_docks() {
    let mut docks = docks_of(
        &[SidebarPanel::Terminals, SidebarPanel::Layouts],
        &[SidebarPanel::Agents],
    );
    move_to(&mut docks, SidebarSide::Left, 1, SidebarSide::Right, 0);
    let left: Vec<SidebarPanel> = docks[0].sections.iter().map(|s| s.panel).collect();
    let right: Vec<SidebarPanel> = docks[1].sections.iter().map(|s| s.panel).collect();
    assert_eq!(left, [SidebarPanel::Terminals]);
    assert_eq!(right, [SidebarPanel::Layouts, SidebarPanel::Agents]);
}

#[test]
fn move_to_clamps_and_ignores_bad_indices() {
    let mut docks = docks_of(&[SidebarPanel::Terminals], &[]);
    move_to(&mut docks, SidebarSide::Left, 9, SidebarSide::Right, 0);
    assert_eq!(docks[0].sections.len(), 1, "an out-of-range move mutated a dock");
    move_to(&mut docks, SidebarSide::Left, 0, SidebarSide::Right, 99);
    assert!(docks[1].holds(SidebarPanel::Terminals));
}

#[test]
fn reorder_moves_within_a_dock_and_stops_at_the_ends() {
    let mut docks = docks_of(&[SidebarPanel::Terminals, SidebarPanel::Layouts], &[]);
    reorder(&mut docks, SidebarSide::Left, 0, -1); // already top
    let panels: Vec<SidebarPanel> = docks[0].sections.iter().map(|s| s.panel).collect();
    assert_eq!(panels, [SidebarPanel::Terminals, SidebarPanel::Layouts]);
    reorder(&mut docks, SidebarSide::Left, 0, 1);
    let panels: Vec<SidebarPanel> = docks[0].sections.iter().map(|s| s.panel).collect();
    assert_eq!(panels, [SidebarPanel::Layouts, SidebarPanel::Terminals]);
}

#[test]
fn add_refuses_to_place_a_section_twice() {
    let mut docks = docks_of(&[SidebarPanel::Terminals], &[]);
    add(&mut docks, SidebarSide::Right, SidebarPanel::Terminals);
    assert!(!docks[1].holds(SidebarPanel::Terminals));
    assert_eq!(docks[0].sections.len(), 1);
}

#[test]
fn available_excludes_whatever_is_placed() {
    let docks = docks_of(&[SidebarPanel::Terminals], &[SidebarPanel::Agents]);
    let pool = available(&docks);
    assert!(!pool.contains(&SidebarPanel::Terminals));
    assert!(!pool.contains(&SidebarPanel::Agents));
    assert!(pool.contains(&SidebarPanel::Notes));
}

#[test]
fn a_new_plugin_panel_lands_on_the_right_collapsed() {
    let mut docks = docks_of(&[SidebarPanel::Terminals], &[SidebarPanel::Agents]);
    place_unplaced(&mut docks, &[SidebarPanel::Plugin(0)]);

    let right = SidebarSide::Right.index();
    assert!(docks[right].holds(SidebarPanel::Plugin(0)));
    // Collapsed, so a newly installed plugin announces itself without stealing
    // room from what the user was already looking at.
    let i = docks[right].find(SidebarPanel::Plugin(0)).unwrap();
    assert!(!docks[right].sections[i].expanded);

    // Idempotent: a plugin reload must not stack duplicates.
    let before = docks[right].sections.len();
    place_unplaced(&mut docks, &[SidebarPanel::Plugin(0)]);
    assert_eq!(docks[right].sections.len(), before);
}

#[test]
fn composition_round_trips_through_tokens() {
    let docks = docks_of(
        &[SidebarPanel::Terminals, SidebarPanel::Worktrees],
        &[SidebarPanel::Agents],
    );
    let tokens = tokens_of(&docks[0], tokenizer(NONE));
    assert_eq!(tokens, ["terminals", "worktrees"]);
    let back = compose(
        SidebarSide::Left,
        &tokens,
        &[],
        260.0,
        resolver(NONE),
        tokenizer(NONE),
    );
    let panels: Vec<SidebarPanel> = back.sections.iter().map(|s| s.panel).collect();
    assert_eq!(panels, [SidebarPanel::Terminals, SidebarPanel::Worktrees]);
}

#[test]
fn an_emptied_dock_round_trips_as_empty() {
    let docks = docks_of(&[], &[]);
    let tokens = tokens_of(&docks[0], tokenizer(NONE));
    assert_eq!(tokens, [EMPTY]);
    let back = compose(
        SidebarSide::Left,
        &tokens,
        &[],
        260.0,
        resolver(NONE),
        tokenizer(NONE),
    );
    assert!(back.sections.is_empty(), "an emptied dock came back as the defaults");
}

#[test]
fn collapsed_tokens_gathers_from_both_docks() {
    let mut docks = docks_of(
        &[SidebarPanel::Terminals, SidebarPanel::Layouts],
        &[SidebarPanel::Agents],
    );
    docks[0].sections[1].expanded = false;
    docks[1].sections[0].expanded = false;
    let mut got = collapsed_tokens(&docks, tokenizer(NONE));
    got.sort();
    assert_eq!(got, ["agents", "layouts"]);
}

#[test]
fn every_builtin_token_round_trips() {
    for panel in SidebarPanel::ALL {
        assert_eq!(
            SidebarPanel::from_id(panel.id()),
            Some(panel),
            "`{}` does not round-trip through from_id",
            panel.id()
        );
    }
}

#[test]
fn builtin_ids_and_labels_are_unique() {
    for (i, a) in SidebarPanel::ALL.iter().enumerate() {
        for b in &SidebarPanel::ALL[i + 1..] {
            assert_ne!(a.id(), b.id(), "duplicate section id `{}`", a.id());
            assert_ne!(a.label(), b.label(), "duplicate section label `{}`", a.label());
        }
    }
}

#[test]
fn no_builtin_token_collides_with_the_empty_sentinel() {
    assert!(SidebarPanel::from_id(EMPTY).is_none());
}

#[test]
fn toggle_at_matches_reveal_without_the_token_round_trip() {
    // The rail and the section headers click through `toggle_at`; keybinds and
    // config actions go through `reveal`. They must agree, or the same section
    // would behave differently depending on how it was opened.
    let panels = [SidebarPanel::Terminals, SidebarPanel::Layouts];
    let mut by_index = docks_of(&panels, &[]);
    let mut by_token = docks_of(&panels, &[]);

    for _ in 0..3 {
        let a = toggle_at(&mut by_index, SidebarSide::Left, 1).unwrap();
        let b = reveal(&mut by_token, SidebarSide::Left, SidebarPanel::Layouts);
        assert_eq!(a.0, SidebarPanel::Layouts);
        assert_eq!(a.1, b.expanded, "index and token paths disagreed on expansion");
        assert_eq!(by_index, by_token, "index and token paths diverged");
    }
}

#[test]
fn toggle_at_ignores_an_index_past_the_end() {
    let mut docks = docks_of(&[SidebarPanel::Terminals], &[]);
    assert!(toggle_at(&mut docks, SidebarSide::Left, 7).is_none());
    assert!(!docks[0].open, "a bad index opened the dock");
}

#[test]
fn every_builtin_has_an_uppercase_label() {
    // `label_upper` is a hand-maintained constant per variant (it is on the
    // repaint path, so it cannot allocate); this is what catches a new section
    // that forgot one.
    for panel in SidebarPanel::ALL {
        assert_eq!(
            panel.label_upper(),
            panel.label().to_uppercase(),
            "`{}` has a stale or missing uppercase label",
            panel.id()
        );
    }
}
