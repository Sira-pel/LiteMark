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
