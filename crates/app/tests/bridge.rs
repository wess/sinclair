use super::*;
use futures::StreamExt;

#[test]
fn forwards_in_order_and_closes() {
    let (tx, rx) = std::sync::mpsc::channel();
    tx.send(Event::Wakeup).unwrap();
    tx.send(Event::TitleChanged("t".to_string())).unwrap();
    tx.send(Event::Exit(Some(0))).unwrap();
    drop(tx);
    let mut stream = forward(rx);
    let collected = futures::executor::block_on(async {
        let mut seen = Vec::new();
        while let Some(event) = stream.next().await {
            seen.push(event);
        }
        seen
    });
    assert_eq!(
        collected,
        vec![
            Event::Wakeup,
            Event::TitleChanged("t".to_string()),
            Event::Exit(Some(0)),
        ]
    );
}
