//! Regression tests for Milestone 8 block-level editing (fences, dividers,
//! word navigation, list indent, numbering) and undo coalescing.

use super::*;

fn doc(content: &str) -> DocumentTab {
    DocumentTab::new("m8b".into(), "m8b.md".into(), content.into(), None, None)
}

fn wys(content: &str) -> DocumentTab {
    let mut d = doc(content);
    d.set_view_mode(ViewMode::Wysiwyg);
    d
}

#[test]
fn w8_enter_on_unclosed_fence_inserts_closing_fence() {
    let mut d = wys("```rust");
    d.place(7, false);
    d.smart_newline(false);
    assert_eq!(d.content, "```rust\n\n```");
    assert_eq!(d.cursor_offset(), 8);
}

#[test]
fn w8_matches_fence_char_and_length() {
    let mut d = wys("~~~~");
    d.place(4, false);
    d.smart_newline(false);
    assert_eq!(d.content, "~~~~\n\n~~~~");
    assert_eq!(d.cursor_offset(), 5);

    let mut d = wys("intro\n\n`````py");
    d.place(d.content.len(), false);
    d.smart_newline(false);
    assert_eq!(d.content, "intro\n\n`````py\n\n`````");
    assert_eq!(d.cursor_offset(), "intro\n\n`````py\n".len());
}

#[test]
fn w8_closed_fence_gets_plain_newline() {
    let mut d = wys("```\ncode\n```");
    d.place(3, false);
    d.smart_newline(false);
    assert_eq!(d.content, "```\n\ncode\n```");
}

#[test]
fn w8_crlf_and_indent_preserved() {
    let mut d = wys("a\r\n  ```js");
    d.place(d.content.len(), false);
    d.smart_newline(false);
    assert_eq!(d.content, "a\r\n  ```js\r\n  \r\n  ```");
    assert_eq!(d.cursor_offset(), "a\r\n  ```js\r\n  ".len());
}

// ---- W9: dividers and closing code fences in WYSIWYG ----

const HR_DOC: &str = "a\n\n---\n\nb";

fn hr_end() -> usize {
    HR_DOC.find("---").unwrap() + 3
}

#[test]
fn w9_typing_at_divider_opens_paragraph_line() {
    for caret in [hr_end(), hr_end() - 1] {
        let mut d = wys(HR_DOC);
        d.place(caret, false);
        d.insert_text_in_mode(caret..caret, "x", ViewMode::Wysiwyg);
        assert_eq!(d.content, "a\n\n---\nx\n\nb", "caret {caret}");
        assert_eq!(d.cursor_offset(), "a\n\n---\nx".len());
    }
}

#[test]
fn w9_typing_at_divider_start_still_goes_below_it() {
    // Text above a divider with no blank line would turn it into a setext
    // heading in other Markdown tools, so it always opens the line below.
    let mut d = wys(HR_DOC);
    let start = HR_DOC.find("---").unwrap();
    d.place(start, false);
    d.insert_text_in_mode(start..start, "x", ViewMode::Wysiwyg);
    assert_eq!(d.content, "a\n\n---\nx\n\nb");
    assert_eq!(d.cursor_offset(), "a\n\n---\nx".len());
}

#[test]
fn w9_input_method_text_opens_a_line_after_the_divider() {
    let mut d = wys(HR_DOC);
    let end = hr_end();
    assert!(d.range_on_block_marker(&(end..end)));
    let at = d.picture_open_line(&(end..end)).unwrap();
    assert_eq!(d.content, "a\n\n---\n\n\nb");
    assert_eq!(at, end + 1..end + 1);
    d.cancel_picture_ime_line();
    assert_eq!(d.content, HR_DOC);
    // A plain paragraph is not a marker.
    assert!(!d.range_on_block_marker(&(0..0)));
}

#[test]
fn w9_raw_mode_still_edits_divider_text() {
    let mut d = doc(HR_DOC);
    d.set_view_mode(ViewMode::Raw);
    d.place(hr_end(), false);
    d.insert_text_in_mode(hr_end()..hr_end(), "x", ViewMode::Raw);
    assert_eq!(d.content, "a\n\n---x\n\nb");
}

#[test]
fn w9_typing_at_closing_fence_goes_after_it() {
    let src = "```\ncode\n```\nb";
    let fence = src.rfind("```").unwrap();
    for caret in [fence, fence + 1, fence + 3] {
        let mut d = wys(src);
        d.place(caret, false);
        d.insert_text_in_mode(caret..caret, "x", ViewMode::Wysiwyg);
        assert_eq!(d.content, "```\ncode\n```\nx\nb", "caret {caret}");
        assert_eq!(d.cursor_offset(), "```\ncode\n```\nx".len());
    }
}

#[test]
fn w9_enter_inside_divider_opens_line_after_without_splitting() {
    let mut d = wys(HR_DOC);
    let caret = hr_end() - 1;
    d.place(caret, false);
    d.smart_newline(false);
    assert_eq!(d.content, "a\n\n---\n\n\nb");
    assert_eq!(d.cursor_offset(), hr_end() + 1);
}

#[test]
fn w9_enter_inside_closing_fence_opens_line_after() {
    let src = "```\ncode\n```";
    let mut d = wys(src);
    d.place(src.len() - 1, false);
    d.smart_newline(false);
    assert_eq!(d.content, "```\ncode\n```\n");
    assert_eq!(d.cursor_offset(), src.len() + 1);
}

#[test]
fn w9_backspace_below_divider_removes_divider_not_merges() {
    let src = "a\n\n---\nb";
    let mut d = wys(src);
    d.place(src.len() - 1, false);
    d.smart_backspace_in_mode(ViewMode::Wysiwyg);
    assert_eq!(d.content, "a\n\nb");
    assert_eq!(d.cursor_offset(), 3);
}

#[test]
fn w9_backspace_on_divider_line_removes_it() {
    let src = "a\n---\nb";
    let mut d = wys(src);
    d.place(5, false);
    d.smart_backspace_in_mode(ViewMode::Wysiwyg);
    assert_eq!(d.content, "a\nb");
    assert_eq!(d.cursor_offset(), 2);
}

#[test]
fn w9_backspace_below_closing_fence_never_merges() {
    let src = "```\ncode\n```\nb";
    let mut d = wys(src);
    d.place(src.len() - 1, false);
    d.smart_backspace_in_mode(ViewMode::Wysiwyg);
    assert_eq!(d.content, src);
    assert_eq!(d.cursor_offset(), "```\ncode".len());
}

#[test]
fn w9_backspace_at_first_code_line_never_joins_opening_fence() {
    let src = "x\n```\ncode\n```";
    let mut d = wys(src);
    d.place(6, false);
    d.smart_backspace_in_mode(ViewMode::Wysiwyg);
    assert_eq!(d.content, src);
}

#[test]
fn w9_delete_before_divider_removes_divider_not_merges() {
    let src = "a\n---\nb";
    let mut d = wys(src);
    d.place(1, false);
    d.delete_forward_in_mode(ViewMode::Wysiwyg);
    assert_eq!(d.content, "a\nb");
    assert_eq!(d.cursor_offset(), 1);
}

#[test]
fn w9_delete_before_closing_fence_never_merges() {
    let src = "```\ncode\n```";
    let mut d = wys(src);
    d.place(8, false);
    d.delete_forward_in_mode(ViewMode::Wysiwyg);
    assert_eq!(d.content, src);
}

// ---- W10: word navigation and deletion on visible text ----

fn visible_all(d: &mut DocumentTab) -> String {
    let all = 0..d.content.len();
    let content = d.content.clone();
    d.parsed().visible_text(&content, all)
}

#[test]
fn w10_ctrl_left_from_after_closer_lands_on_visible_word_start() {
    let src = "a **bold word** z";
    let mut d = wys(src);
    d.place(src.find(" z").unwrap(), false);
    d.move_word_left(false);
    assert_eq!(d.cursor_offset(), src.find("word").unwrap());
    d.move_word_left(false);
    assert_eq!(d.cursor_offset(), src.find("bold").unwrap());
}

#[test]
fn w10_ctrl_right_from_before_opener_lands_on_visible_word_end() {
    let src = "**bold** x";
    let mut d = wys(src);
    d.place(0, false);
    d.move_word_right(false);
    assert_eq!(d.cursor_offset(), 6);
}

#[test]
fn w10_ctrl_shift_arrows_select_visible_words() {
    let src = "a **bold word** z";
    let mut d = wys(src);
    d.place(src.find(" z").unwrap(), false);
    d.move_word_left(true);
    assert_eq!(d.selected_visible_text(), "word");
}

#[test]
fn w10_ctrl_backspace_keeps_span_valid() {
    let src = "**hello world**";
    let mut d = wys(src);
    d.place(13, false);
    d.delete_word_left();
    assert_eq!(visible_all(&mut d).trim_end(), "hello");
    assert!(
        !d.content.contains(" **"),
        "closer after space: {:?}",
        d.content
    );
}

#[test]
fn w10_ctrl_delete_removes_visible_word_only() {
    let src = "x **hello** world";
    let mut d = wys(src);
    d.place(src.find("hello").unwrap(), false);
    d.delete_word_right();
    assert!(!d.content.contains("hello"), "{:?}", d.content);
    assert_eq!(visible_all(&mut d), "x  world");
    assert!(d.content.ends_with(" world"));
}

const TABLE: &str = "| a b | c d |\n| --- | --- |\n| e f | g h |";

#[test]
fn w10_word_ops_stay_inside_table_cell() {
    // Ctrl+Delete at the end of a cell must not eat the pipe.
    let mut d = wys(TABLE);
    d.place(TABLE.find("a b").unwrap() + 3, false);
    d.delete_word_right();
    assert_eq!(d.content, TABLE);

    // Ctrl+Backspace at the start of a cell does nothing.
    let mut d = wys(TABLE);
    d.place(TABLE.find("c d").unwrap(), false);
    d.delete_word_left();
    assert_eq!(d.content, TABLE);

    // Ctrl+Left inside a cell stops at the cell's words.
    let mut d = wys(TABLE);
    d.place(TABLE.find("c d").unwrap() + 3, false);
    d.move_word_left(false);
    assert_eq!(d.cursor_offset(), TABLE.find("d |").unwrap());
    d.move_word_left(false);
    assert_eq!(d.cursor_offset(), TABLE.find("c d").unwrap());

    // Ctrl+Backspace deletes a word inside the cell only.
    let mut d = wys(TABLE);
    d.place(TABLE.find("c d").unwrap() + 3, false);
    d.delete_word_left();
    assert_eq!(d.content, TABLE.replacen("c d", "c ", 1));
}

// ---- W11: list Tab nesting ----

#[test]
fn w11_tab_indents_to_preceding_marker_width() {
    for (src, expect) in [
        ("1. a\n2. b", "1. a\n   2. b"),
        ("10. a\n11. b", "10. a\n    11. b"),
        ("- a\n- b", "- a\n  - b"),
        ("- a\n  - b\n- c", "- a\n  - b\n  - c"),
    ] {
        let mut d = wys(src);
        d.place(src.len(), false);
        d.indent();
        assert_eq!(d.content, expect, "{src:?}");
        let parsed = d.parsed();
        let last = parsed.lines.last().unwrap();
        assert!(matches!(last.kind, LineKind::ListItem { .. }));
    }
}

#[test]
fn w11_shift_tab_outdents_to_parent_indent() {
    for (src, expect) in [
        ("1. a\n   2. b", "1. a\n2. b"),
        ("10. a\n    11. b", "10. a\n11. b"),
        ("- a\n  - b", "- a\n- b"),
    ] {
        let mut d = wys(src);
        d.place(src.len(), false);
        d.outdent();
        assert_eq!(d.content, expect, "{src:?}");
    }
}

#[test]
fn w11_first_item_keeps_default_indent() {
    let mut d = wys("1. a");
    d.place(4, false);
    d.indent();
    assert_eq!(d.content, "  1. a");
}

#[test]
fn w11_tab_keeps_caret_on_same_text() {
    let mut d = wys("1. a\n2. bc");
    d.place(8, false);
    d.indent();
    assert_eq!(d.content, "1. a\n   2. bc");
    assert_eq!(d.cursor_offset(), 11);
}
