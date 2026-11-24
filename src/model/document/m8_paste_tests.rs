//! Regression tests for Milestone 8 paste and link items (W7, W13, W15a).

use super::*;

fn doc(content: &str) -> DocumentTab {
    DocumentTab::new("m8p".into(), "m8p.md".into(), content.into(), None, None)
}

fn lone_lf(s: &str) -> bool {
    s.match_indices('\n')
        .any(|(i, _)| i == 0 || s.as_bytes()[i - 1] != b'\r')
}

#[test]
fn w15a_table_row_in_crlf_note_uses_crlf() {
    let mut d = doc("| A | B |\r\n| --- | --- |\r\n| 1 | 2 |\r\n");
    d.place(2, false);
    d.apply_format_in_mode("table", ViewMode::Wysiwyg);
    assert!(d.content.len() > 40, "a row was added");
    assert!(!lone_lf(&d.content), "mixed endings: {:?}", d.content);
}

#[test]
fn w15a_new_table_in_crlf_note_uses_crlf() {
    let mut d = doc("intro\r\nmore text\r\n");
    d.place(5, false);
    d.apply_format_in_mode("table", ViewMode::Wysiwyg);
    assert!(d.content.contains("Column 1"));
    assert!(!lone_lf(&d.content), "mixed endings: {:?}", d.content);
}

// ---- W13: WYSIWYG-aware paste ----

fn paste(d: &mut DocumentTab, text: &str, mode: ViewMode) {
    let range = d.selection_range();
    d.paste_text_in_mode(range, text, mode);
}

#[test]
fn w13_crlf_note_gets_crlf_for_pasted_lf() {
    let mut d = doc("a\r\nb\r\n");
    d.place(1, false);
    paste(&mut d, "x\ny\r\nz\rw", ViewMode::Wysiwyg);
    assert_eq!(d.content, "ax\r\ny\r\nz\r\nw\r\nb\r\n");
}

#[test]
fn w13_lf_note_gets_lf_for_pasted_crlf() {
    let mut d = doc("a\nb\n");
    d.place(1, false);
    paste(&mut d, "x\r\ny", ViewMode::Raw);
    assert_eq!(d.content, "ax\ny\nb\n");
}

#[test]
fn w13_table_cell_paste_escapes_pipes_and_flattens_newlines() {
    let mut d = doc("| A | B |\n| --- | --- |\n| 1 | 2 |\n");
    let at = d.content.find("1 |").unwrap() + 1;
    d.place(at, false);
    paste(&mut d, "x|y\nz\n", ViewMode::Wysiwyg);
    assert_eq!(d.content, "| A | B |\n| --- | --- |\n| 1x\\|y z | 2 |\n");
}

#[test]
fn w13_table_cell_paste_keeps_already_escaped_pipe() {
    let mut d = doc("| A | B |\n| --- | --- |\n| 1 | 2 |\n");
    let at = d.content.find("1 |").unwrap() + 1;
    d.place(at, false);
    paste(&mut d, "a\\|b", ViewMode::Wysiwyg);
    assert!(d.content.contains("1a\\|b"), "{:?}", d.content);
    assert!(!d.content.contains("\\\\|"), "{:?}", d.content);
}

#[test]
fn w13_partial_selection_over_styled_text_keeps_markers_balanced() {
    // Select "llo** wor" across the closing marker of a bold span.
    let mut d = doc("**Hello** world");
    d.caret.range = 4..13;
    d.caret.reversed = false;
    paste(&mut d, "XY", ViewMode::Wysiwyg);
    assert_eq!(d.content.matches("**").count() % 2, 0, "{:?}", d.content);
    assert!(d.content.contains("XY"), "{:?}", d.content);
    assert!(d.content.contains("ld"), "{:?}", d.content);
}

#[test]
fn w13_markdown_text_paste_is_inserted_verbatim() {
    let mut d = doc("one two");
    d.caret.range = 0..3;
    d.caret.reversed = false;
    paste(&mut d, "**bold** and `code`", ViewMode::Wysiwyg);
    assert_eq!(d.content, "**bold** and `code` two");
}

#[test]
fn w13_raw_mode_paste_is_a_plain_edit() {
    let mut d = doc("| A |\n| - |\n| 1 |\n");
    d.place(2, false);
    paste(&mut d, "x|y", ViewMode::Raw);
    assert_eq!(d.content, "| x|yA |\n| - |\n| 1 |\n");
}

#[test]
fn w13_paste_into_a_missing_table_cell_fills_it_and_escapes_the_pipe() {
    let md = "| a | b |\n| --- | --- |\n| 1 |";
    let mut d = doc(md);
    d.place(md.len(), false);
    d.pending_table_col = Some(1);
    paste(&mut d, "x|y\nz", ViewMode::Wysiwyg);
    assert_eq!(d.content, "| a | b |\n| --- | --- |\n| 1 | x\\|y z |");
}
