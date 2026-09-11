use super::*;

#[test]
fn a_dump_under_the_ceiling_is_kept_whole() {
  let dump = "one\r\ntwo\r\n".to_string();
  assert_eq!(trim_to_bytes(dump.clone(), 1024), dump);
}

#[test]
fn an_oversized_dump_keeps_whole_trailing_rows() {
  let dump = "aaaa\r\nbbbb\r\ncccc\r\n".to_string();
  let kept = trim_to_bytes(dump, 12);
  // Never a partial row, and always the newest ones.
  assert_eq!(kept, "cccc\r\n");
}

#[test]
fn a_single_oversized_row_is_dropped_rather_than_cut() {
  let dump = format!("{}\r\n", "x".repeat(100));
  assert_eq!(trim_to_bytes(dump, 10), "");
}

#[test]
fn trimming_multibyte_text_does_not_panic() {
  let dump = "日本語日本語\r\n語語語語語語\r\n".to_string();
  let kept = trim_to_bytes(dump, 20);
  assert!(kept.is_empty() || kept.ends_with("\r\n"));
}
