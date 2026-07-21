use super::*;

/// `Progress` takes a percentage, not a 0..1 fraction — feeding it a fraction
/// renders a bar that never visibly leaves the left edge.
#[test]
fn download_progress_is_a_percentage_across_most_of_the_bar() {
    assert_eq!(percent(&Stage::Downloading { done: 0, total: 100 }), 0.0);
    assert!((percent(&Stage::Downloading { done: 50, total: 100 }) - 42.5).abs() < 0.01);
    assert!((percent(&Stage::Downloading { done: 100, total: 100 }) - 85.0).abs() < 0.01);
}

#[test]
fn every_stage_stays_in_percentage_range() {
    for stage in [
        Stage::Downloading { done: 1, total: 2 },
        Stage::Preparing,
        Stage::Installing,
        Stage::Verifying,
    ] {
        let p = percent(&stage);
        assert!((0.0..=100.0).contains(&p), "{stage:?} -> {p}");
    }
}

#[test]
fn stages_after_the_download_only_move_forward() {
    let done = percent(&Stage::Downloading { done: 100, total: 100 });
    assert!(done < percent(&Stage::Preparing));
    assert!(percent(&Stage::Preparing) < percent(&Stage::Installing));
    assert!(percent(&Stage::Installing) < percent(&Stage::Verifying));
}

#[test]
fn an_unknown_total_holds_the_bar_at_zero() {
    // Better a bar that hasn't moved than one inventing progress it can't know.
    assert_eq!(percent(&Stage::Downloading { done: 900, total: 0 }), 0.0);
}

#[test]
fn overlong_downloads_cannot_overflow_the_bar() {
    assert!(percent(&Stage::Downloading { done: 500, total: 100 }) <= 85.0);
}

#[test]
fn only_the_download_reports_byte_counts() {
    assert_eq!(detail(&Stage::Downloading { done: 5_000_000, total: 20_000_000 }), "5.0 MB of 20.0 MB");
    assert_eq!(detail(&Stage::Downloading { done: 5, total: 0 }), "");
    assert_eq!(detail(&Stage::Preparing), "");
    assert_eq!(detail(&Stage::Verifying), "");
}

#[test]
fn sizes_are_decimal_mb_to_match_what_github_reports() {
    // The real 1.27.7 dmg: GitHub's release page and Finder both call this
    // 20.3 MB. Dividing by 1 MiB would render "19.4 MB" for the same file and
    // read as a stalled or mismatched download.
    assert_eq!(detail(&Stage::Downloading { done: 0, total: 20_314_688 }), "0.0 MB of 20.3 MB");
}

#[test]
fn only_the_working_phase_is_busy() {
    // What stops Update & Restart from starting a second install over the
    // first: every other phase accepts the click, Working never does. Calls the
    // real predicate — asserting on `Phase` literals instead would pass even
    // with `busy` inverted.
    assert!(busy(&Phase::Working(Stage::Preparing)));
    assert!(!busy(&Phase::Idle));
    assert!(!busy(&Phase::Failed("boom".into())));
}

#[test]
fn the_action_label_tracks_phase_and_installability() {
    assert_eq!(action_label(&Phase::Idle, true), "Update & Restart");
    // No installable asset: the button must not promise an install it can't do.
    assert_eq!(action_label(&Phase::Idle, false), "Open Download");
    assert_eq!(action_label(&Phase::Working(Stage::Installing), true), "Updating…");
    // A failure has to stay retryable rather than dead-ending the window.
    assert_eq!(action_label(&Phase::Failed("boom".into()), true), "Try Again");
}
