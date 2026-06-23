use super::*;

fn marked(ch: char) -> Row {
    let mut r = Row::new(2);
    r.cells[0].ch = ch;
    r
}

#[test]
fn pushes_in_order() {
    let mut sb = Scrollback::new(10);
    sb.push(marked('a'));
    sb.push(marked('b'));
    assert_eq!(sb.len(), 2);
    assert_eq!(sb.get(0).unwrap().text(), "a");
    assert_eq!(sb.get(1).unwrap().text(), "b");
}

#[test]
fn evicts_oldest_at_limit() {
    let mut sb = Scrollback::new(2);
    sb.push(marked('a'));
    sb.push(marked('b'));
    sb.push(marked('c'));
    assert_eq!(sb.len(), 2);
    assert_eq!(sb.get(0).unwrap().text(), "b");
    assert_eq!(sb.get(1).unwrap().text(), "c");
}

#[test]
fn zero_limit_stores_nothing() {
    let mut sb = Scrollback::new(0);
    sb.push(marked('a'));
    assert!(sb.is_empty());
}

#[test]
fn clear_empties() {
    let mut sb = Scrollback::new(5);
    sb.push(marked('a'));
    sb.clear();
    assert!(sb.is_empty());
    assert_eq!(sb.limit(), 5);
}

#[test]
fn resize_rows_changes_width() {
    let mut sb = Scrollback::new(5);
    sb.push(marked('a'));
    sb.resize_rows(7);
    assert_eq!(sb.get(0).unwrap().len(), 7);
}
