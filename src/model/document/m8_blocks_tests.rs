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
