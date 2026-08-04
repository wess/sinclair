use super::*;

#[test]
fn metadata_events_are_bounded_without_losing_wakeup_or_exit() {
    let counters = Arc::new(Counters::default());
    let (sender, receiver) = event_channel(Arc::clone(&counters));

    for _ in 0..MAX_QUEUED_METADATA_EVENTS + 4 {
        sender.send(Event::Bell).unwrap();
    }
    sender.send(Event::Wakeup).unwrap();
    sender.send(Event::Exit(Some(0))).unwrap();

    assert_eq!(receiver.len(), EVENT_CHANNEL_CAPACITY);
    assert_eq!(
        counters.dropped_events.load(Ordering::Relaxed),
        4,
        "overflowing metadata should be counted"
    );
    let events: Vec<_> = receiver.try_iter().collect();
    assert!(events.contains(&Event::Wakeup));
    assert!(events.contains(&Event::Exit(Some(0))));
}
