#[test]
fn parsed_line_col_matches_line_col() {
    use crate::markdown::parser::parse_document;
    for text in ["", "a", "a\n", "\n\n", "ab\r\ncd\re\n", "héllo\nwörld\n\nx"] {
        let parsed = parse_document(text);
        for offset in 0..=text.len() + 1 {
            assert_eq!(
                super::parsed_line_col(text, &parsed, offset),
                super::line_col(text, offset),
                "{text:?} at {offset}"
            );
        }
    }
}
use super::*;
use crate::ui::{move_vertical_visual, EditorPaint, PaintRow};
use gpui::{point, px, ShapedLine};

fn find_doc(source: &str) -> DocumentTab {
    DocumentTab::new("t".into(), "t.md".into(), source.to_string(), None, None)
}

#[test]
fn find_walks_every_match_selects_it_and_wraps() {
    let mut doc = find_doc("one cat, two Cat, three cat");
    assert_eq!(doc.find_hit_ranges("cat", false).len(), 3);
    assert_eq!(doc.find_hit_ranges("cat", true).len(), 2);

    let mut index = None;
    let mut seen = Vec::new();
    for _ in 0..4 {
        index = doc.select_match("cat", false, index, true);
        seen.push(index.unwrap());
        assert_eq!(doc.selected_text().to_lowercase(), "cat");
    }
    assert_eq!(seen, vec![0, 1, 2, 0]);

    let back = doc.select_match("cat", false, Some(0), false);
    assert_eq!(back, Some(2));
}

#[test]
fn find_with_no_match_selects_nothing() {
    let mut doc = find_doc("hello");
    assert!(doc.find_hit_ranges("zzz", false).is_empty());
    assert_eq!(doc.select_match("zzz", false, None, true), None);
    assert_eq!(doc.selected_text(), "");
}

#[test]
fn replace_current_replaces_one_and_moves_on() {
    let mut doc = find_doc("cat and cat and cat");
    let idx = doc.select_match("cat", false, None, true);
    let next = doc.replace_current_match("cat", "dog", false, idx);
    assert_eq!(doc.content, "dog and cat and cat");
    assert_eq!(next, Some(0));
    assert_eq!(doc.selected_text(), "cat");
}

#[test]
fn replace_all_respects_case() {
    let mut doc = find_doc("cat Cat cat");
    assert!(doc.replace_every_match("cat", "dog", true));
    assert_eq!(doc.content, "dog Cat dog");
    assert!(!doc.replace_every_match("zzz", "x", true));
}

#[test]
fn find_handles_multibyte_text() {
    let mut doc = find_doc("héllo wörld héllo");
    assert_eq!(doc.find_hit_ranges("HÉLLO", false).len(), 2);
    assert!(doc.replace_every_match("héllo", "x", true));
    assert_eq!(doc.content, "x wörld x");
}

#[test]
fn enter_with_selection_leaving_a_table_cell_does_not_panic() {
    let source = "| a | b |\n| --- | --- |\n| hello | world |";
    let mut doc = DocumentTab::new("t".into(), "t.md".into(), source.to_string(), None, None);
    let start = source.find("hello").unwrap() + 2;
    let end = source.find("world").unwrap() + 2;
    doc.caret.range = start..end;
    doc.smart_newline(false);
    assert!(doc.content.starts_with(source));
}

#[test]
fn outdent_selection_into_next_indent_keeps_char_boundaries() {
    let source = "a\n  ✓ b";
    let mut doc = DocumentTab::new("t".into(), "t.md".into(), source.to_string(), None, None);
    // End one byte into the next line's indent.
    doc.caret.range = 0..3;
    doc.outdent();
    let range = doc.caret.range.clone();
    assert!(doc.content.is_char_boundary(range.start));
    assert!(doc.content.is_char_boundary(range.end));
    assert_eq!(doc.content, "a\n✓ b");
    assert_eq!(range, 0..2);
}

#[test]
fn test_compute_stats() {
    let text = "Hello world from LiteMark.\nSecond line with five words.";
    let stats = compute_stats(text);
    assert_eq!(stats.words, 9);
    assert_eq!(stats.lines, 2);
    assert_eq!(stats.chars, text.len());
    assert_eq!(stats.reading_time_min, 1);
}

#[test]
fn test_undo_redo_stack() {
    let mut doc = DocumentTab::new(
        "tab-1".to_string(),
        "Doc.md".to_string(),
        "Initial".to_string(),
        None,
        None,
    );
    assert!(!doc.is_modified);

    doc.set_content("Version 2".to_string());
    assert!(doc.is_modified);
    assert_eq!(doc.content, "Version 2");

    assert!(doc.undo());
    assert_eq!(doc.content, "Initial");

    assert!(doc.redo());
    assert_eq!(doc.content, "Version 2");

    assert!(!doc.redo());
}

#[test]
fn undo_history_is_diff_sized() {
    let mut doc = DocumentTab::new(
        "tab-1".into(),
        "Doc.md".into(),
        "x".repeat(2_000),
        None,
        None,
    );
    for _ in 0..80 {
        let end = doc.content.len();
        // A trailing space keeps each insert as its own undo step.
        doc.edit(end..end, "y ");
    }
    assert!(doc.undo_stored_bytes() < 2_000 * 80);
    assert_eq!(doc.content.len(), 2_160);
    assert!(doc.content.ends_with("y "));
    assert!(doc.undo());
    assert_eq!(doc.content.len(), 2_158);
    for _ in 0..79 {
        assert!(doc.undo());
    }
    assert_eq!(doc.content.chars().last(), Some('x'));
    assert!(!doc.undo());
}

#[test]
fn a_typed_word_is_one_undo_and_replace_all_is_one_step() {
    let mut doc = DocumentTab::new("tab-1".into(), "Doc.md".into(), String::new(), None, None);
    for ch in ["h", "e", "l", "l", "o"] {
        let end = doc.content.len();
        doc.edit(end..end, ch);
    }
    assert_eq!(doc.content, "hello");
    assert!(doc.undo());
    assert_eq!(doc.content, "");
    assert!(!doc.undo());

    doc.edit(0..0, "hello");
    doc.edit(5..5, " ");
    doc.edit(6..6, "world");
    assert_eq!(doc.content, "hello world");
    assert!(doc.undo());
    assert_eq!(doc.content, "hello ");
    assert!(doc.undo());
    assert_eq!(doc.content, "hello");

    doc.edit(0..5, "cat cat");
    assert!(doc.replace_every_match("cat", "dog", true));
    assert_eq!(doc.content, "dog dog");
    assert!(doc.undo());
    assert_eq!(doc.content, "cat cat");
}

#[test]
fn formatting_uses_the_selection_and_clear_keeps_markers() {
    let mut doc = DocumentTab::new(
        "tab-1".into(),
        "Doc.md".into(),
        "Hello world".into(),
        None,
        None,
    );
    doc.caret.range = 6..11;
    doc.apply_format("bold");
    assert_eq!(doc.content, "Hello **world**");
    assert!(!doc.content.ends_with("**bold text**") || doc.content.contains("**world**"));

    let mut doc = DocumentTab::new("tab-1".into(), "Doc.md".into(), "abc".into(), None, None);
    doc.caret.move_to(1);
    doc.apply_format("bold");
    assert_eq!(doc.content, "**abc**");
    // Toggling bold again strips delimiters
    doc.apply_format("bold");
    assert_eq!(doc.content, "abc");

    let mut empty_doc =
        DocumentTab::new("tab-1".into(), "Doc.md".into(), "".into(), None, None);
    empty_doc.apply_format("bold");
    assert_eq!(empty_doc.content, "**bold text**");

    let source = "- item *keep*\n* bullet `code`";
    let mut doc = DocumentTab::new("tab-1".into(), "Doc.md".into(), source.into(), None, None);
    doc.select_all();
    doc.apply_format("clear");
    assert_eq!(doc.content, "- item keep\n* bullet code");
}

#[test]
fn move_line_up_and_down_and_undo() {
    let mut doc = DocumentTab::new(
        "tab-1".into(),
        "Doc.md".into(),
        "First line\nSecond line\nThird line".into(),
        None,
        None,
    );
    // Cursor on Second line
    doc.caret.move_to(15);
    doc.move_line_up();
    assert_eq!(doc.content, "Second line\nFirst line\nThird line");
    assert!(doc.undo());
    assert_eq!(doc.content, "First line\nSecond line\nThird line");

    // Move down
    doc.caret.move_to(15);
    doc.move_line_down();
    assert_eq!(doc.content, "First line\nThird line\nSecond line");
    assert!(doc.undo());
    assert_eq!(doc.content, "First line\nSecond line\nThird line");

    // Edge case: top line cannot move up
    doc.caret.move_to(2);
    doc.move_line_up();
    assert_eq!(doc.content, "First line\nSecond line\nThird line");
}

fn visible_copy(text: &str, range: Range<usize>) -> String {
    let mut doc = DocumentTab::new("tab-1".into(), "Doc.md".into(), text.into(), None, None);
    doc.caret.move_to(range.start);
    doc.caret.select_to(range.end);
    doc.selected_visible_text()
}

#[test]
fn visible_copy_drops_inline_markers() {
    let text = "**bold** and *it* `code` [link](https://x.y) \\*";
    assert_eq!(visible_copy(text, 0..text.len()), "bold and it code link *");
    // Selection inside a bold span: "a **bold** b", "bo" is 4..6.
    assert_eq!(visible_copy("a **bold** b", 3..6), "bo");
}

#[test]
fn visible_copy_rewrites_block_prefixes() {
    let text =
        "# Title\n> quote\n- a\n  - nested\n- [x] done\n- [ ] todo\n3. three\n\n---\nend";
    assert_eq!(
        visible_copy(text, 0..text.len()),
        "Title\nquote\n• a\n  • nested\n☑ done\n☐ todo\n3. three\n\nend"
    );
    // Starting mid item text: no bullet.
    assert_eq!(visible_copy("- apple", 3..7), "pple");
    // Starting on the previous line's newline keeps it.
    assert_eq!(visible_copy("x\n# y", 1..5), "\ny");
}

#[test]
fn visible_copy_skips_fences_and_keeps_tables() {
    let text = "```rs\nlet x = 1;\n```\nafter";
    assert_eq!(visible_copy(text, 0..text.len()), "let x = 1;\nafter");
    let table = "| a | **b** |\n|---|---|\n| 1 | 2 |";
    assert_eq!(visible_copy(table, 0..table.len()), table);
}

#[test]
fn visible_copy_is_source_in_raw_mode_and_plain_text() {
    let text = "**bold**";
    let mut doc = DocumentTab::new("tab-1".into(), "Doc.md".into(), text.into(), None, None);
    doc.view_mode = ViewMode::Raw;
    doc.caret.move_to(0);
    doc.caret.select_to(text.len());
    assert_eq!(doc.selected_visible_text(), text);
    let mut doc = DocumentTab::new("tab-1".into(), "Doc.txt".into(), text.into(), None, None);
    doc.caret.move_to(0);
    doc.caret.select_to(text.len());
    assert_eq!(doc.selected_visible_text(), text);
}

#[test]
fn select_line_up_and_down_multiline() {
    let text = "Line one\nLine two\nLine three";
    let mut doc = DocumentTab::new("tab-1".into(), "Doc.md".into(), text.into(), None, None);
    // "Line one" is 0..8, newline at 8
    // "Line two" is 9..17, newline at 17
    // "Line three" is 18..28
    // Place cursor in the middle of Line two (index 13: "Line |two")
    doc.caret.move_to(13);

    // First Ctrl+Shift+Up selects to beginning of Line two (index 9)
    doc.select_line_up();
    assert_eq!(doc.selection_range(), 9..13);
    assert_eq!(doc.selected_text(), "Line");

    // Second Ctrl+Shift+Up extends beyond to beginning of Line one (index 0)
    doc.select_line_up();
    assert_eq!(doc.selection_range(), 0..13);
    assert_eq!(doc.selected_text(), "Line one\nLine");

    // Third Ctrl+Shift+Up stays at 0 (start of doc)
    doc.select_line_up();
    assert_eq!(doc.selection_range(), 0..13);

    // Now test select_line_down:
    doc.caret.move_to(13);
    // First Ctrl+Shift+Down selects to end of Line two (index 17)
    doc.select_line_down();
    assert_eq!(doc.selection_range(), 13..17);
    assert_eq!(doc.selected_text(), " two");

    // Second Ctrl+Shift+Down extends beyond to end of Line three (index 28)
    doc.select_line_down();
    assert_eq!(doc.selection_range(), 13..28);
    assert_eq!(doc.selected_text(), " two\nLine three");

    // Third Ctrl+Shift+Down stays at doc end
    doc.select_line_down();
    assert_eq!(doc.selection_range(), 13..28);
}

#[test]
fn doc_start_and_end_navigation() {
    let mut doc = DocumentTab::new(
        "tab-1".into(),
        "Doc.md".into(),
        "Alpha\nBeta\nGamma".into(),
        None,
        None,
    );
    doc.caret.move_to(7); // In "Beta"
    doc.move_doc_start(false);
    assert_eq!(doc.cursor_offset(), 0);
    assert!(doc.caret.range.is_empty());

    doc.caret.move_to(7);
    doc.move_doc_start(true);
    assert_eq!(doc.selection_range(), 0..7);

    doc.caret.move_to(7);
    doc.move_doc_end(false);
    assert_eq!(doc.cursor_offset(), 16);
    assert!(doc.caret.range.is_empty());

    doc.caret.move_to(7);
    doc.move_doc_end(true);
    assert_eq!(doc.selection_range(), 7..16);
}

#[test]
fn heading_toggle_and_level_switching() {
    let mut doc = DocumentTab::new(
        "tab-1".into(),
        "Doc.md".into(),
        "My heading\nParagraph".into(),
        None,
        None,
    );
    doc.caret.move_to(3);
    // Apply H1
    doc.apply_format("h1");
    assert_eq!(doc.content, "# My heading\nParagraph");

    // Switch to H2 (should not stack ## #)
    doc.apply_format("h2");
    assert_eq!(doc.content, "## My heading\nParagraph");

    // Toggle H2 off
    doc.apply_format("h2");
    assert_eq!(doc.content, "My heading\nParagraph");
}

#[test]
fn inline_formatting_whitespace_trimming() {
    let mut doc = DocumentTab::new(
        "tab-1".into(),
        "Doc.md".into(),
        "leading  RAM usage  trailing".into(),
        None,
        None,
    );
    // Select "  RAM usage  " (includes leading and trailing spaces)
    doc.caret.range = 7..20;
    doc.apply_format("bold");
    assert_eq!(doc.content, "leading  **RAM usage**  trailing");

    // Toggle off
    doc.caret.range = 7..24;
    doc.apply_format("bold");
    assert_eq!(doc.content, "leading  RAM usage  trailing");
}

#[test]
fn stacking_bold_and_italic_formatting() {
    let mut doc = DocumentTab::new(
        "tab-1".into(),
        "Doc.md".into(),
        "hello world".into(),
        None,
        None,
    );
    doc.caret.range = 6..11; // select "world"
    doc.apply_format("bold");
    assert_eq!(doc.content, "hello **world**");

    // Stack italic on top
    doc.apply_format("italic");
    assert_eq!(doc.content, "hello ***world***");

    // Toggle bold off
    doc.apply_format("bold");
    assert_eq!(doc.content, "hello *world*");

    // Toggle italic off
    doc.apply_format("italic");
    assert_eq!(doc.content, "hello world");
}

#[test]
fn pdf_edits_are_ignored() {
    let mut doc = DocumentTab::new(
        "tab-1".into(),
        "scan.pdf".into(),
        "extracted".into(),
        None,
        None,
    );
    assert!(doc.is_read_only());
    doc.edit(0..9, "overwrite");
    assert_eq!(doc.content, "extracted");
}

#[test]
fn find_replace_moves_through_matches() {
    let mut doc = DocumentTab::new(
        "tab-1".into(),
        "Doc.md".into(),
        "cat cat cat".into(),
        None,
        None,
    );
    let first = doc.select_match("cat", true, None, true).unwrap();
    assert_eq!(first, 0);
    assert_eq!(doc.selected_text(), "cat");
    let second = doc.select_match("cat", true, Some(first), true).unwrap();
    assert_eq!(second, 1);
    doc.replace_every_match("cat", "dog", true);
    assert_eq!(doc.content, "dog dog dog");
    assert!(doc.undo());
    assert_eq!(doc.content, "cat cat cat");
}

#[test]
fn undo_redo_caret_stays_on_a_char_boundary() {
    let mut doc = DocumentTab::new("tab-1".into(), "Doc.md".into(), "ab".into(), None, None);
    doc.edit(0..2, "😀ក");
    assert!(doc.undo());
    assert_eq!(doc.content, "ab");
    assert_eq!(doc.cursor_offset(), 2);
    assert!(doc.content.is_char_boundary(doc.cursor_offset()));
    assert!(doc.redo());
    assert_eq!(doc.content, "😀ក");
    assert_eq!(doc.cursor_offset(), "😀ក".len());
    assert!(doc.content.is_char_boundary(doc.cursor_offset()));
    assert!(doc.content.is_char_boundary(doc.selection_range().start));
    assert!(doc.content.is_char_boundary(doc.selection_range().end));

    doc.caret.range = 2..2;
    doc.move_home(false);
    doc.move_end(false);
    doc.apply_format("bold");
    assert!(doc.content.is_char_boundary(doc.cursor_offset()));
    assert!(doc.content.is_char_boundary(doc.selection_range().start));
    assert!(doc.content.is_char_boundary(doc.selection_range().end));
}

#[test]
fn shift_arrow_extends_from_the_anchor_and_clicks_select_word_and_line() {
    let mut doc = DocumentTab::new(
        "tab-1".into(),
        "Doc.md".into(),
        "abcd\nxy".into(),
        None,
        None,
    );
    doc.caret.move_to(3);
    doc.move_left(true);
    doc.move_left(true);
    assert_eq!(doc.selection_range(), 1..3);
    doc.move_right(true);
    assert_eq!(doc.selection_range(), 2..3);
    doc.move_right(true);
    doc.move_right(true);
    assert_eq!(doc.selected_text(), "d");

    doc.caret.move_to(1);
    doc.select_word();
    assert_eq!(doc.selected_text(), "abcd");
    doc.select_line();
    assert_eq!(doc.selected_text(), "abcd\n");
}

#[test]
fn find_previous_leaves_the_current_match() {
    let mut doc = DocumentTab::new(
        "tab-1".into(),
        "Doc.md".into(),
        "cat cat cat".into(),
        None,
        None,
    );
    let first = doc.select_match("cat", true, None, true).unwrap();
    assert_eq!(first, 0);
    assert_eq!(doc.cursor_offset(), 3);
    let previous = doc.select_match("cat", true, Some(first), false).unwrap();
    assert_eq!(previous, 2);
    assert_eq!(doc.selected_text(), "cat");
    assert_ne!(doc.selection_range().start, 0);
}

#[test]
fn word_navigation_and_word_deletion() {
    let mut doc = DocumentTab::new(
        "tab-1".into(),
        "Doc.md".into(),
        "hello world test".into(),
        None,
        None,
    );
    doc.caret.move_to(0);
    doc.move_word_right(false);
    assert_eq!(doc.cursor_offset(), 5);
    doc.move_word_right(true);
    assert_eq!(doc.selection_range(), 5..11);

    doc.caret.move_to(16);
    doc.delete_word_left();
    assert_eq!(doc.content, "hello world ");
    doc.delete_word_left();
    assert_eq!(doc.content, "hello ");

    doc.select_all();
    assert_eq!(doc.selection_range(), 0..6);
}

#[test]
fn test_smart_newline_bullet_and_task_and_number() {
    // 1. Bullet continuation and exit
    let mut doc = DocumentTab::new("1".into(), "test.md".into(), "- Item 1".into(), None, None);
    doc.caret.move_to(8);
    doc.smart_newline(false);
    assert_eq!(doc.content, "- Item 1\n- ");
    assert_eq!(doc.cursor_offset(), 11);

    // Press Enter again on empty bullet -> exits list
    doc.smart_newline(false);
    assert_eq!(doc.content, "- Item 1\n");
    assert_eq!(doc.cursor_offset(), 9);

    // 2. Numbered list continuation and exit
    let mut doc2 =
        DocumentTab::new("2".into(), "test.md".into(), "1. First".into(), None, None);
    doc2.caret.move_to(8);
    doc2.smart_newline(false);
    assert_eq!(doc2.content, "1. First\n2. ");

    // Press Enter on empty "2. " -> exits list
    doc2.smart_newline(false);
    assert_eq!(doc2.content, "1. First\n");

    // 3. Task list continuation and exit
    let mut doc3 = DocumentTab::new(
        "3".into(),
        "test.md".into(),
        "- [ ] Todo".into(),
        None,
        None,
    );
    doc3.caret.move_to(10);
    doc3.smart_newline(false);
    assert_eq!(doc3.content, "- [ ] Todo\n- [ ] ");

    // Press Enter on empty task -> exits task list
    doc3.smart_newline(false);
    assert_eq!(doc3.content, "- [ ] Todo\n");

    // 4. Shift+Enter -> plain newline
    let mut doc4 = DocumentTab::new("4".into(), "test.md".into(), "- Item".into(), None, None);
    doc4.caret.move_to(6);
    doc4.smart_newline(true);
    assert_eq!(doc4.content, "- Item\n");

    // 5. Heading -> plain newline (does not duplicate #)
    let mut doc5 = DocumentTab::new("5".into(), "test.md".into(), "# Title".into(), None, None);
    doc5.caret.move_to(7);
    doc5.smart_newline(false);
    assert_eq!(doc5.content, "# Title\n");
}

#[test]
fn test_smart_backspace_boundaries() {
    // 1. Heading demotion
    let mut doc =
        DocumentTab::new("1".into(), "test.md".into(), "# Heading".into(), None, None);
    doc.caret.move_to(2); // right after "# "
    doc.smart_backspace();
    assert_eq!(doc.content, "Heading");
    assert_eq!(doc.cursor_offset(), 0);

    let mut doc_h2 = DocumentTab::new(
        "2".into(),
        "test.md".into(),
        "## Heading".into(),
        None,
        None,
    );
    doc_h2.caret.move_to(3); // right after "## "
    doc_h2.smart_backspace();
    assert_eq!(doc_h2.content, "# Heading");

    // 2. Bullet list prefix removal
    let mut doc_bullet =
        DocumentTab::new("3".into(), "test.md".into(), "- Item".into(), None, None);
    doc_bullet.caret.move_to(2); // right after "- "
    doc_bullet.smart_backspace();
    assert_eq!(doc_bullet.content, "Item");

    // 3. Task list prefix demotion
    let mut doc_task = DocumentTab::new(
        "4".into(),
        "test.md".into(),
        "- [ ] Task".into(),
        None,
        None,
    );
    doc_task.caret.move_to(6); // right after "- [ ] "
    doc_task.smart_backspace();
    assert_eq!(doc_task.content, "- Task");

    // 4. Auto-paired brackets deletion
    let mut doc_pair = DocumentTab::new("5".into(), "test.md".into(), "()".into(), None, None);
    doc_pair.caret.move_to(1); // between ( and )
    doc_pair.smart_backspace();
    assert_eq!(doc_pair.content, "");

    // 5. Indented list item outdent on backspace
    let mut doc_indent = DocumentTab::new(
        "6".into(),
        "test.md".into(),
        "  - Subitem".into(),
        None,
        None,
    );
    doc_indent.caret.move_to(4); // right after "  - "
    doc_indent.smart_backspace();
    assert_eq!(doc_indent.content, "- Subitem");
}

#[test]
fn test_indent_and_outdent() {
    let mut doc = DocumentTab::new(
        "1".into(),
        "test.md".into(),
        "- Item 1\n- Item 2".into(),
        None,
        None,
    );
    doc.caret.move_to(0);
    doc.indent();
    assert_eq!(doc.content, "  - Item 1\n- Item 2");

    doc.outdent();
    assert_eq!(doc.content, "- Item 1\n- Item 2");

    // Multi-line selection indent and outdent
    doc.caret.range = 0..doc.content.len();
    doc.indent();
    assert_eq!(doc.content, "  - Item 1\n  - Item 2");

    doc.outdent();
    assert_eq!(doc.content, "- Item 1\n- Item 2");
}

#[test]
fn test_block_prefix_len() {
    assert_eq!(block_prefix_len("# Title"), 2);
    assert_eq!(block_prefix_len("### Title"), 4);
    assert_eq!(block_prefix_len("- Item"), 2);
    assert_eq!(block_prefix_len("  - Sub"), 4);
    assert_eq!(block_prefix_len("- [ ] Task"), 6);
    assert_eq!(block_prefix_len("1. One"), 3);
    assert_eq!(block_prefix_len("10. Ten"), 4);
    assert_eq!(block_prefix_len("> Quote"), 2);
    assert_eq!(block_prefix_len("Plain paragraph"), 0);
}

#[test]
fn test_universal_reversible_inline_and_block_toggling() {
    // 1. Inline toggling with cursor inside word (no selection needed)
    for (cmd, expected_wrapped) in [
        ("bold", "hello **world**"),
        ("italic", "hello *world*"),
        ("underline", "hello <u>world</u>"),
        ("strike", "hello ~~world~~"),
        ("code", "hello `world`"),
        ("link", "hello [world](https://)"),
    ] {
        let mut doc = DocumentTab::new(
            "t".into(),
            "test.md".into(),
            "hello world".into(),
            None,
            None,
        );
        doc.place(8, false); // inside "world"
        doc.apply_format_in_mode(cmd, ViewMode::Wysiwyg);
        assert_eq!(doc.content, expected_wrapped, "apply {cmd} failed");
        // Pressing the exact same command again must cleanly undo/unwrap it!
        doc.apply_format_in_mode(cmd, ViewMode::Wysiwyg);
        assert_eq!(
            doc.content, "hello world",
            "second press of {cmd} did not unwrap"
        );
    }

    // 2. Block toggling ON and OFF idempotently, plus switching between block types
    for (cmd, expected_line) in [
        ("h1", "# My line"),
        ("h2", "## My line"),
        ("h3", "### My line"),
        ("h4", "#### My line"),
        ("h5", "##### My line"),
        ("h6", "###### My line"),
        ("ul", "- My line"),
        ("ol", "1. My line"),
        ("checklist", "- [ ] My line"),
        ("quote", "> My line"),
        ("codeblock", "```\nMy line\n```"),
    ] {
        let mut doc =
            DocumentTab::new("b".into(), "test.md".into(), "My line".into(), None, None);
        doc.place(3, false);
        doc.apply_format_in_mode(cmd, ViewMode::Wysiwyg);
        assert_eq!(doc.content, expected_line, "apply block {cmd} failed");
        // Pressing a second time toggles off back to "My line"
        doc.apply_format_in_mode(cmd, ViewMode::Wysiwyg);
        assert_eq!(
            doc.content, "My line",
            "second press of block {cmd} did not toggle off"
        );
    }

    // 3. Switching directly from Bullet List to Numbered List to Checklist without stacking
    let mut doc =
        DocumentTab::new("s".into(), "test.md".into(), "Task item".into(), None, None);
    doc.place(2, false);
    doc.apply_format_in_mode("ul", ViewMode::Wysiwyg);
    assert_eq!(doc.content, "- Task item");
    assert!(doc.active_formats.ul);
    doc.apply_format_in_mode("ol", ViewMode::Wysiwyg);
    assert_eq!(doc.content, "1. Task item");
    assert!(doc.active_formats.ol);
    assert!(!doc.active_formats.ul);
    doc.apply_format_in_mode("checklist", ViewMode::Wysiwyg);
    assert_eq!(doc.content, "- [ ] Task item");
    assert!(doc.active_formats.checklist);
    assert!(!doc.active_formats.ol);
    doc.apply_format_in_mode("checklist", ViewMode::Wysiwyg);
    assert_eq!(doc.content, "Task item");
    assert!(!doc.active_formats.checklist);
}

#[test]
fn test_wysiwyg_pending_inline_formats_and_typing() {
    let mut doc = DocumentTab::new("p".into(), "test.md".into(), "hello ".into(), None, None);
    doc.place(6, false); // at empty end after space

    // Press Ctrl+B once -> arms pending bold, does not insert placeholder text
    doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
    assert_eq!(doc.content, "hello ");
    assert!(doc.pending_formats.bold);
    assert!(doc.active_formats.bold);

    // Press Ctrl+B again -> disarms pending bold
    doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
    assert_eq!(doc.content, "hello ");
    assert!(!doc.pending_formats.bold);
    assert!(!doc.active_formats.bold);

    // Re-arm Bold + Italic, then type "A" -> wraps in "***A***" and places cursor after A
    doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
    doc.apply_format_in_mode("italic", ViewMode::Wysiwyg);
    assert!(doc.active_formats.bold && doc.active_formats.italic);
    let range = doc.selection_range();
    doc.insert_text_in_mode(range, "A", ViewMode::Wysiwyg);
    assert_eq!(doc.content, "hello ***A***");
    assert_eq!(doc.cursor_offset(), 10); // right after 'A', inside closing '***'
    assert!(doc.active_formats.bold && doc.active_formats.italic);
}

#[test]
fn test_underline_toggle_selection_and_pending_formats() {
    let mut doc = DocumentTab::new(
        "u1".into(),
        "test.md".into(),
        "selected text to underline".into(),
        None,
        None,
    );
    // Select "text" (indices 9..13)
    doc.place(9, false);
    doc.place(13, true);
    doc.apply_format_in_mode("underline", ViewMode::Wysiwyg);
    assert_eq!(doc.content, "selected <u>text</u> to underline");
    assert!(doc.active_formats.underline);

    // Toggle again on selection should unwrap
    doc.apply_format_in_mode("underline", ViewMode::Wysiwyg);
    assert_eq!(doc.content, "selected text to underline");
    assert!(!doc.active_formats.underline);

    // Empty cursor in WYSIWYG mode toggles pending_formats
    let mut doc2 = DocumentTab::new("u2".into(), "test.md".into(), "".into(), None, None);
    doc2.place(0, false);
    doc2.apply_format_in_mode("underline", ViewMode::Wysiwyg);
    assert!(doc2.pending_formats.underline);
    assert!(doc2.active_formats.underline);

    // Disarm pending underline
    doc2.apply_format_in_mode("underline", ViewMode::Wysiwyg);
    assert!(!doc2.pending_formats.underline);
    assert!(!doc2.active_formats.underline);

    // Re-arm and type text -> wraps in <u>...</u>
    doc2.apply_format_in_mode("underline", ViewMode::Wysiwyg);
    let range = doc2.selection_range();
    doc2.insert_text_in_mode(range, "hello", ViewMode::Wysiwyg);
    assert_eq!(doc2.content, "<u>hello</u>");
    assert!(!doc2.pending_formats.underline);
}

/// Type `text` one character at a time at the current caret, as the
/// editor does for real keystrokes.
fn type_keystrokes(doc: &mut DocumentTab, text: &str) {
    for ch in text.chars() {
        let range = doc.selection_range();
        doc.insert_text_in_mode(range, &ch.to_string(), ViewMode::Wysiwyg);
    }
}

fn pending_flag(doc: &DocumentTab, cmd: &str) -> bool {
    match cmd {
        "bold" => doc.pending_formats.bold,
        "italic" => doc.pending_formats.italic,
        "strike" => doc.pending_formats.strike,
        _ => false,
    }
}

fn styled_flag(doc: &mut DocumentTab, cmd: &str) -> bool {
    let kind = match cmd {
        "bold" => InlineTagKind::Bold,
        "italic" => InlineTagKind::Italic,
        "strike" => InlineTagKind::Strike,
        _ => InlineTagKind::Link,
    };
    doc.parsed().all_inline_pairs().any(|p| p.kind == kind)
}

/// Ctrl+B/I/U/Shift+S/` with no selection, type a word, then toggle the
/// same format off: the span must stay formatted and the caret must move
/// past the closing marker so the following words are plain.
#[test]
fn toggle_off_after_typing_keeps_the_span_and_exits_it() {
    for (cmd, prefix, suffix) in [
        ("bold", "**", "**"),
        ("italic", "*", "*"),
        ("underline", "<u>", "</u>"),
        ("strike", "~~", "~~"),
        ("code", "`", "`"),
    ] {
        let mut doc =
            DocumentTab::new("b".into(), "test.md".into(), "This is ".into(), None, None);
        doc.place(8, false);
        doc.apply_format_in_mode(cmd, ViewMode::Wysiwyg);
        type_keystrokes(&mut doc, "very");
        let wrapped = format!("This is {prefix}very{suffix}");
        assert_eq!(doc.content, wrapped, "{cmd}: typing did not wrap");

        // Second toggle: exit the span, do not unwrap it
        doc.apply_format_in_mode(cmd, ViewMode::Wysiwyg);
        assert_eq!(
            doc.content, wrapped,
            "{cmd}: second toggle removed the formatting"
        );
        assert!(!doc.content.contains('\u{200B}'));
        assert_eq!(
            doc.cursor_offset(),
            wrapped.len(),
            "{cmd}: caret did not move past the closing marker"
        );
        assert!(
            !doc.has_pending_inline_formats(),
            "{cmd}: pending formats stayed armed after leaving the span"
        );

        // Further typing is plain
        type_keystrokes(&mut doc, " important");
        assert_eq!(
            doc.content,
            format!("This is {prefix}very{suffix} important"),
            "{cmd}: text after leaving the span is not plain"
        );
    }
}

/// A space typed at the end of a `*`/`**`/`~~` span must not land right
/// before the closer (a closer cannot follow whitespace in Markdown). It
/// goes after the closing marker; the next letter pulls it back inside
/// and extends the span, while stopping after the space keeps it outside.
#[test]
fn space_at_span_end_types_outside_the_closer() {
    for (cmd, open, close) in [
        ("bold", "**", "**"),
        ("italic", "*", "*"),
        ("strike", "~~", "~~"),
    ] {
        let mut doc = DocumentTab::new("s".into(), "test.md".into(), String::new(), None, None);
        doc.apply_format_in_mode(cmd, ViewMode::Wysiwyg);
        type_keystrokes(&mut doc, "read");
        type_keystrokes(&mut doc, " ");
        assert_eq!(
            doc.content,
            format!("{open}read{close} "),
            "{cmd}: space did not move outside the closing marker"
        );
        assert!(!doc.content.contains('\u{200B}'));
        assert!(
            pending_flag(&doc, cmd),
            "{cmd}: format is not armed after the space"
        );
        let parsed = doc.parsed();
        let line = parsed.line_at_offset(0).unwrap();
        assert_eq!(
            line.display_text, "read ",
            "{cmd}: raw markers leaked into the display"
        );

        // The next letter extends the original span, keeping one span
        type_keystrokes(&mut doc, "me");
        assert_eq!(
            doc.content,
            format!("{open}read me{close}"),
            "{cmd}: continued typing did not extend the span"
        );
        assert!(
            styled_flag(&mut doc, cmd),
            "{cmd}: continued typing is not rendered formatted"
        );
    }
}

/// Enter with the caret inside a bold span closes it before the newline
/// and reopens it on the new line; at the end of the span it leaves the
/// format armed so the next line continues bold.
#[test]
fn enter_inside_bold_span_closes_and_reopens_it() {
    let mut doc = DocumentTab::new("e".into(), "test.md".into(), String::new(), None, None);
    doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
    let range = doc.selection_range();
    doc.insert_text_in_mode(range, "first half", ViewMode::Wysiwyg);
    assert_eq!(doc.content, "**first half**");

    // Caret between "first" and " half": split the span around the break
    doc.place(7, false);
    doc.smart_newline(false);
    assert_eq!(
        doc.content, "**first**\n **half**",
        "Enter did not close and reopen the bold span"
    );
    assert_eq!(
        doc.cursor_offset(),
        13,
        "caret is not at the start of the reopened span"
    );
    assert!(
        doc.active_formats.bold,
        "the reopened span is not bold at the caret"
    );

    // Typing at the end of the reopened span stays bold
    doc.place(doc.content.len() - 2, false);
    type_keystrokes(&mut doc, "!");
    assert_eq!(doc.content, "**first**\n **half!**");

    // Caret at the end of the span: close it, keep bold armed, the next
    // letter reopens it on the new line
    let mut doc2 = DocumentTab::new("e2".into(), "test.md".into(), String::new(), None, None);
    doc2.apply_format_in_mode("bold", ViewMode::Wysiwyg);
    let range = doc2.selection_range();
    doc2.insert_text_in_mode(range, "first half", ViewMode::Wysiwyg);
    doc2.smart_newline(false);
    assert_eq!(doc2.content, "**first half**\n");
    assert!(doc2.pending_formats.bold);
    type_keystrokes(&mut doc2, "second");
    assert_eq!(doc2.content, "**first half**\n**second**");
}

/// Trailing spaces of an ATX heading stay in the WYSIWYG display and the
/// caret mapping, so typing a space at the end of a heading does not
/// vanish until the next letter.
#[test]
fn heading_trailing_space_stays_in_display_and_caret() {
    let mut doc = DocumentTab::new("h".into(), "test.md".into(), "# my".into(), None, None);
    doc.place(4, false);
    type_keystrokes(&mut doc, " ");
    assert_eq!(doc.content, "# my ");
    let parsed = doc.parsed();
    let line = parsed.line_at_offset(0).unwrap();
    assert_eq!(line.display_text, "my ", "trailing space left the display");
    assert_eq!(doc.cursor_offset(), 5, "caret fell back before the space");

    type_keystrokes(&mut doc, "note");
    assert_eq!(doc.content, "# my note");
}

/// Unformatting part of a larger span splits it cleanly instead of
/// leaving stray markers; for underline it must not nest a second <u>.
#[test]
fn partial_unformat_splits_the_span() {
    for (cmd, open, close) in [
        ("bold", "**", "**"),
        ("italic", "*", "*"),
        ("underline", "<u>", "</u>"),
        ("strike", "~~", "~~"),
        ("code", "`", "`"),
    ] {
        let wrapped = format!("{open}Hello{close}");

        // Start of the span: "He" becomes plain, "llo" stays formatted
        let mut doc =
            DocumentTab::new("p".into(), "test.md".into(), wrapped.clone(), None, None);
        doc.caret.range = open.len()..open.len() + 2;
        doc.apply_format_in_mode(cmd, ViewMode::Wysiwyg);
        assert_eq!(
            doc.content,
            format!("He{open}llo{close}"),
            "{cmd}: unformatting the start left stray markers"
        );

        // End of the span: "llo" becomes plain, "He" stays formatted
        let mut doc =
            DocumentTab::new("p".into(), "test.md".into(), wrapped.clone(), None, None);
        doc.caret.range = open.len() + 2..open.len() + 5;
        doc.apply_format_in_mode(cmd, ViewMode::Wysiwyg);
        assert_eq!(
            doc.content,
            format!("{open}He{close}llo"),
            "{cmd}: unformatting the end left stray markers"
        );

        // Middle of the span: "ell" becomes plain, both halves formatted
        let mut doc = DocumentTab::new("p".into(), "test.md".into(), wrapped, None, None);
        doc.caret.range = open.len() + 1..open.len() + 4;
        doc.apply_format_in_mode(cmd, ViewMode::Wysiwyg);
        assert_eq!(
            doc.content,
            format!("{open}H{close}ell{open}o{close}"),
            "{cmd}: unformatting the middle left stray markers"
        );
    }
}

fn flags_for_word(content: &str, word: &str) -> (bool, bool, bool, bool) {
    let parsed = parse_document(content);
    let segs: Vec<_> = parsed.lines[0]
        .styled_segments
        .iter()
        .filter(|s| {
            let text = &content[s.source.clone()];
            text == word || text.split_whitespace().any(|w| w == word)
        })
        .collect();
    assert!(
        !segs.is_empty(),
        "word {word:?} missing in {content:?} segs={:?}",
        parsed.lines[0]
            .styled_segments
            .iter()
            .map(|s| &content[s.source.clone()])
            .collect::<Vec<_>>()
    );
    let s = segs[0];
    (s.bold, s.italic, s.underline, s.strike)
}

fn assert_clean_display(content: &str, expected: &str) {
    assert!(
        !content.contains('\u{200B}'),
        "zero-width space in {content:?}"
    );
    assert!(
        !content.contains("****"),
        "adjacent bold runs in {content:?}"
    );
    let parsed = parse_document(content);
    assert_eq!(
        parsed.lines[0].display_text, expected,
        "raw markers in display of {content:?}"
    );
    for ch in ['*', '~', '_'] {
        assert!(
            !parsed.lines[0].display_text.contains(ch),
            "literal {ch:?} in display {:?} of {content:?}",
            parsed.lines[0].display_text
        );
    }
}

fn select_once(doc: &mut DocumentTab, word: &str) {
    let start = doc.content.find(word).expect(word);
    doc.caret.range = start..(start + word.len());
}

/// Unformatting a word whose selection ends on the space before the next
/// word must move that space outside the leftover markers. Re-applying
/// the format must merge back into the neighbor instead of `**aa**** bb**`.
#[test]
fn unformat_before_space_moves_space_outside_markers() {
    for (cmd, wrapped) in [
        ("bold", "**aa bb cc**"),
        ("italic", "*aa bb cc*"),
        ("strike", "~~aa bb cc~~"),
    ] {
        let mut doc = DocumentTab::new(
            "e1".into(),
            "test.md".into(),
            wrapped.to_string(),
            None,
            None,
        );
        select_once(&mut doc, "aa");
        doc.apply_format_in_mode(cmd, ViewMode::Wysiwyg);
        assert_clean_display(&doc.content, "aa bb cc");
        let aa = flags_for_word(&doc.content, "aa");
        let bb = flags_for_word(&doc.content, "bb");
        let cc = flags_for_word(&doc.content, "cc");
        assert_eq!(
            aa,
            (false, false, false, false),
            "{cmd} left aa styled in {}",
            doc.content
        );
        let expect = match cmd {
            "bold" => (true, false, false, false),
            "italic" => (false, true, false, false),
            "strike" => (false, false, false, true),
            _ => unreachable!(),
        };
        assert_eq!(bb, expect, "{cmd} dropped bb in {}", doc.content);
        assert_eq!(cc, expect, "{cmd} dropped cc in {}", doc.content);
        assert!(
            !doc.content.contains("aa** ")
                && !doc.content.contains("aa* ")
                && !doc.content.contains("aa~~ "),
            "{cmd}: space stayed inside the leftover opener: {}",
            doc.content
        );

        // Turning the format back on merges with the neighboring span.
        select_once(&mut doc, "aa");
        doc.apply_format_in_mode(cmd, ViewMode::Wysiwyg);
        assert_clean_display(&doc.content, "aa bb cc");
        assert_eq!(
            flags_for_word(&doc.content, "aa"),
            expect,
            "{cmd} re-apply: {}",
            doc.content
        );
        assert_eq!(
            flags_for_word(&doc.content, "bb"),
            expect,
            "{cmd} re-apply: {}",
            doc.content
        );
        assert!(
            !doc.content.contains("****") && !doc.content.contains("~~~~"),
            "{cmd} re-apply stacked markers: {}",
            doc.content
        );
    }

    // Same flanking split when italic is already on (bold+italic span).
    let mut doc = DocumentTab::new(
        "e1b".into(),
        "test.md".into(),
        "***aa bb cc***".into(),
        None,
        None,
    );
    select_once(&mut doc, "aa");
    doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
    assert_clean_display(&doc.content, "aa bb cc");
    assert_eq!(
        flags_for_word(&doc.content, "aa"),
        (false, true, false, false)
    );
    assert_eq!(
        flags_for_word(&doc.content, "bb"),
        (true, true, false, false)
    );
    assert_eq!(
        flags_for_word(&doc.content, "cc"),
        (true, true, false, false)
    );

    // Underline tags do not depend on spaces; the leftover words stay
    // underlined while the flanking-sensitive markers move off the gap.
    let mut doc = DocumentTab::new(
        "e1c".into(),
        "test.md".into(),
        "<u>**aa bb cc**</u>".into(),
        None,
        None,
    );
    select_once(&mut doc, "aa");
    doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
    assert_clean_display(&doc.content, "aa bb cc");
    assert_eq!(
        flags_for_word(&doc.content, "aa"),
        (false, false, true, false)
    );
    assert_eq!(
        flags_for_word(&doc.content, "bb"),
        (true, false, true, false)
    );
    assert_eq!(
        flags_for_word(&doc.content, "cc"),
        (true, false, true, false)
    );
}

/// Removing bold or italic from the middle or end of a bold+italic span
/// must not write `***aa **bb cc*` runs that merge and stop pairing.
#[test]
fn stacked_unformat_at_word_edge_does_not_merge_stars() {
    // End: drop italic from "bb cc"
    let mut doc = DocumentTab::new(
        "e2".into(),
        "test.md".into(),
        "***aa bb cc***".into(),
        None,
        None,
    );
    doc.caret.range = 6..11; // "bb cc"
    doc.apply_format_in_mode("italic", ViewMode::Wysiwyg);
    assert_clean_display(&doc.content, "aa bb cc");
    assert_eq!(
        flags_for_word(&doc.content, "aa"),
        (true, true, false, false)
    );
    assert_eq!(
        flags_for_word(&doc.content, "bb"),
        (true, false, false, false)
    );
    assert_eq!(
        flags_for_word(&doc.content, "cc"),
        (true, false, false, false)
    );
    assert!(
        !doc.content.contains("***aa **") && !doc.content.contains("**aa ***"),
        "end unformat merged delimiter runs: {}",
        doc.content
    );

    // End: drop bold from "bb cc"
    let mut doc = DocumentTab::new(
        "e2b".into(),
        "test.md".into(),
        "***aa bb cc***".into(),
        None,
        None,
    );
    doc.caret.range = 6..11;
    doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
    assert_clean_display(&doc.content, "aa bb cc");
    assert_eq!(
        flags_for_word(&doc.content, "aa"),
        (true, true, false, false)
    );
    assert_eq!(
        flags_for_word(&doc.content, "bb"),
        (false, true, false, false)
    );
    assert_eq!(
        flags_for_word(&doc.content, "cc"),
        (false, true, false, false)
    );

    // Middle: drop italic from "bb"
    let mut doc = DocumentTab::new(
        "e2c".into(),
        "test.md".into(),
        "***aa bb cc***".into(),
        None,
        None,
    );
    select_once(&mut doc, "bb");
    doc.apply_format_in_mode("italic", ViewMode::Wysiwyg);
    assert_clean_display(&doc.content, "aa bb cc");
    assert_eq!(
        flags_for_word(&doc.content, "aa"),
        (true, true, false, false)
    );
    assert_eq!(
        flags_for_word(&doc.content, "bb"),
        (true, false, false, false)
    );
    assert_eq!(
        flags_for_word(&doc.content, "cc"),
        (true, true, false, false)
    );
    assert!(
        !doc.content.contains("_**aa **_") && !doc.content.contains("**aa **_"),
        "middle italic unformat wrote colliding markers: {}",
        doc.content
    );

    // Same middle split with underline also on.
    let mut doc = DocumentTab::new(
        "e2d".into(),
        "test.md".into(),
        "<u>***aa bb cc***</u>".into(),
        None,
        None,
    );
    select_once(&mut doc, "bb");
    doc.apply_format_in_mode("italic", ViewMode::Wysiwyg);
    assert_clean_display(&doc.content, "aa bb cc");
    assert_eq!(
        flags_for_word(&doc.content, "aa"),
        (true, true, true, false)
    );
    assert_eq!(
        flags_for_word(&doc.content, "bb"),
        (true, false, true, false)
    );
    assert_eq!(
        flags_for_word(&doc.content, "cc"),
        (true, true, true, false)
    );
}

/// Stacked bold+italic typing (`***`, from Ctrl+B then Ctrl+I) must keep
/// one span across spaces and both toggle-offs, and the text typed after
/// leaving the stack must be plain. Same for bold+strike (`**~~`).
#[test]
fn stacked_typing_keeps_one_span_and_toggles_off_cleanly() {
    let mut doc = DocumentTab::new("x".into(), "test.md".into(), "Go ".into(), None, None);
    doc.place(3, false);
    doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
    doc.apply_format_in_mode("italic", ViewMode::Wysiwyg);
    type_keystrokes(&mut doc, "now");
    type_keystrokes(&mut doc, " ");
    assert_eq!(
        doc.content, "Go ***now*** ",
        "space did not move outside the stacked closing run"
    );
    let parsed = doc.parsed();
    let line = parsed.line_at_offset(0).unwrap();
    assert_eq!(
        line.display_text, "Go now ",
        "raw markers leaked into the display"
    );

    type_keystrokes(&mut doc, "please");
    assert_eq!(
        doc.content, "Go ***now please***",
        "stacked typing split the span"
    );

    doc.apply_format_in_mode("italic", ViewMode::Wysiwyg);
    assert_eq!(
        doc.content, "Go ***now please***",
        "Ctrl+I unwrapped the stacked span"
    );
    assert_eq!(
        doc.cursor_offset(),
        19,
        "Ctrl+I did not exit past the stacked closing run"
    );
    assert!(
        doc.pending_formats.bold,
        "bold did not stay armed after leaving italic"
    );

    doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
    assert_eq!(doc.content, "Go ***now please***");
    assert!(
        !doc.has_pending_inline_formats(),
        "bold is still armed after the second toggle"
    );
    type_keystrokes(&mut doc, " ok");
    assert_eq!(doc.content, "Go ***now please*** ok");

    // Strike outer, bold inner: same stacked behavior
    let mut doc = DocumentTab::new("y".into(), "test.md".into(), String::new(), None, None);
    doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
    doc.apply_format_in_mode("strike", ViewMode::Wysiwyg);
    type_keystrokes(&mut doc, "was");
    type_keystrokes(&mut doc, " ");
    assert_eq!(doc.content, "~~**was**~~ ");
    type_keystrokes(&mut doc, "here");
    assert_eq!(doc.content, "~~**was here**~~");
    doc.apply_format_in_mode("strike", ViewMode::Wysiwyg);
    assert_eq!(doc.content, "~~**was here**~~");
    assert!(doc.pending_formats.bold);
    doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
    assert!(!doc.has_pending_inline_formats());
    type_keystrokes(&mut doc, " ok");
    assert_eq!(doc.content, "~~**was here**~~ ok");
}

/// Typing into a newly created formatted span is one undo step: a single
/// undo removes the whole span (and redo brings it back), for plain and
/// stacked spans alike.
#[test]
fn undo_removes_a_newly_typed_span_in_one_step() {
    let mut doc = DocumentTab::new("u".into(), "test.md".into(), "Hello ".into(), None, None);
    doc.place(6, false);
    doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
    type_keystrokes(&mut doc, "world");
    assert_eq!(doc.content, "Hello **world**");
    doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
    assert!(doc.undo(), "undo failed");
    assert_eq!(
        doc.content, "Hello ",
        "one undo did not remove the whole span"
    );
    assert!(doc.redo(), "redo failed");
    assert_eq!(doc.content, "Hello **world**");

    // Stacked span with a space inside: still one undo step
    let mut doc = DocumentTab::new("v".into(), "test.md".into(), "Go ".into(), None, None);
    doc.place(3, false);
    doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
    doc.apply_format_in_mode("italic", ViewMode::Wysiwyg);
    type_keystrokes(&mut doc, "now please");
    doc.apply_format_in_mode("italic", ViewMode::Wysiwyg);
    doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
    assert!(doc.undo(), "undo failed");
    assert_eq!(
        doc.content, "Go ",
        "one undo did not remove the stacked span"
    );
}

/// Extra spaces typed at the end of an emphasis span keep stacking
/// outside the closer: the broken `**a  **` form never appears, the
/// display stays formatted, and the next letter is still bold.
#[test]
fn extra_spaces_at_span_end_stay_outside_the_closer() {
    for (cmd, open, close) in [
        ("bold", "**", "**"),
        ("italic", "*", "*"),
        ("strike", "~~", "~~"),
    ] {
        let mut doc = DocumentTab::new("w".into(), "test.md".into(), String::new(), None, None);
        doc.apply_format_in_mode(cmd, ViewMode::Wysiwyg);
        type_keystrokes(&mut doc, "a");
        type_keystrokes(&mut doc, " ");
        type_keystrokes(&mut doc, " ");
        assert_eq!(
            doc.content,
            format!("{open}a{close}  "),
            "{cmd}: the second space reentered the span"
        );
        let parsed = doc.parsed();
        let line = parsed.line_at_offset(0).unwrap();
        assert_eq!(
            line.display_text, "a  ",
            "{cmd}: raw markers leaked into the display"
        );
        assert!(
            pending_flag(&doc, cmd),
            "{cmd}: format is not armed after the spaces"
        );

        // the next letter is still formatted
        type_keystrokes(&mut doc, "b");
        assert_eq!(
            doc.content,
            format!("{open}a{close}  {open}b{close}"),
            "{cmd}: the letter after the spaces is not formatted"
        );
        assert!(styled_flag(&mut doc, cmd));
    }
}

/// Ctrl+B with no selection wraps the current word only when the caret
/// is strictly inside it (letters on both sides). At the end of a word
/// just typed the format is armed for the letters typed next instead, so
/// `un{Ctrl+B}believ{Ctrl+B}able` saves as `un**believ**able`.
#[test]
fn toggle_at_word_end_arms_instead_of_wrapping_the_word() {
    let mut doc = DocumentTab::new("t".into(), "test.md".into(), String::new(), None, None);
    type_keystrokes(&mut doc, "un");
    doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
    assert_eq!(doc.content, "un", "Ctrl+B wrapped the word the caret ends");
    assert!(
        doc.pending_formats.bold,
        "bold is not armed at the word end"
    );

    type_keystrokes(&mut doc, "believ");
    assert_eq!(doc.content, "un**believ**");
    doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
    assert_eq!(doc.content, "un**believ**");
    type_keystrokes(&mut doc, "able");
    assert_eq!(doc.content, "un**believ**able");

    // A caret strictly inside a word still wraps the whole word
    let mut doc = DocumentTab::new("t2".into(), "test.md".into(), "Hello".into(), None, None);
    doc.place(3, false);
    doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
    assert_eq!(doc.content, "**Hello**");

    // A caret at the start of a word arms the format without wrapping
    let mut doc = DocumentTab::new("t3".into(), "test.md".into(), " world".into(), None, None);
    doc.place(1, false);
    doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
    assert_eq!(doc.content, " world");
    assert!(doc.pending_formats.bold);
}

/// Select-all then an inline toggle formats each block inside its prefix,
/// leaves code blocks alone, and a second press removes the format from
/// everything without touching the other formats.
#[test]
fn select_all_inline_toggle_formats_each_block_and_toggles_back() {
    fn has(seg: &StyledSegment, cmd: &str) -> bool {
        match cmd {
            "bold" => seg.bold,
            "italic" => seg.italic,
            "underline" => seg.underline,
            _ => seg.strike,
        }
    }
    fn shape(content: &str) -> Vec<(String, String)> {
        crate::markdown::parse_document(content)
            .lines
            .iter()
            .map(|l| {
                (
                    format!("{:?}", std::mem::discriminant(&l.kind)),
                    l.display_text.clone(),
                )
            })
            .collect()
    }
    fn press(doc: &mut DocumentTab, cmd: &str, mode: ViewMode) {
        doc.caret.range = 0..doc.content.len();
        doc.apply_format_in_mode(cmd, mode);
    }
    let welcome = crate::services::fs_service::INITIAL_MARKDOWN;
    let plain = "# Title\n\nPlain paragraph.\n\n- item one\n- [ ] task\n\n> quote\n\n```\ncode here\n```\n";
    let code_block = "```\nfn main() {\n    println!(\"Hello from LiteMark!\");\n}\n```\n";

    for cmd in ["bold", "italic", "underline", "strike"] {
        for mode in [ViewMode::Wysiwyg, ViewMode::Raw] {
            for original in [welcome, plain] {
                let ctx = format!("{cmd} {mode:?} on {original:?}");
                let mut doc =
                    DocumentTab::new("p".into(), "p.md".into(), original.into(), None, None);
                doc.view_mode = mode;

                press(&mut doc, cmd, mode);
                let on = doc.content.clone();
                assert_eq!(
                    shape(&on),
                    shape(original),
                    "block structure changed: {ctx}\n{on}"
                );
                if original == welcome {
                    assert!(on.ends_with(code_block), "code block touched: {ctx}\n{on}");
                }
                let parsed = crate::markdown::parse_document(&on);
                for line in &parsed.lines {
                    if matches!(line.kind, LineKind::CodeLine | LineKind::CodeFence { .. }) {
                        continue;
                    }
                    for seg in &line.styled_segments {
                        let text = &on[seg.source.clone()];
                        assert!(
                            text.trim().is_empty() || has(seg, cmd),
                            "{text:?} not formatted: {ctx}\n{on}"
                        );
                    }
                }

                press(&mut doc, cmd, mode);
                let off = doc.content.clone();
                assert_eq!(shape(&off), shape(original), "second press: {ctx}\n{off}");
                let parsed = crate::markdown::parse_document(&off);
                assert!(
                    parsed
                        .lines
                        .iter()
                        .flat_map(|l| &l.styled_segments)
                        .all(|s| !has(s, cmd)),
                    "second press left {cmd}: {ctx}\n{off}"
                );
                if original == plain {
                    assert_eq!(off, plain, "second press did not restore: {ctx}");
                }

                press(&mut doc, cmd, mode);
                assert_eq!(doc.content, on, "third press differs from first: {ctx}");
            }
        }
    }
}

/// Select-all toggles leave a heading's closing hashes outside the
/// markers, and skip lines the parser would not show in the format, so a
/// second press turns the format off instead of nesting it.
#[test]
fn select_all_inline_toggle_skips_what_cannot_render() {
    // (source, command, after first press, after second press)
    let cases = [
        ("# Title #\n", "bold", "# **Title** #\n", "# Title #\n"),
        ("## A ##\n", "strike", "## ~~A~~ ##\n", "## A ##\n"),
        // Indented code and a `~~~` fence are code blocks: left alone.
        (
            "para\n\n    code line\n",
            "underline",
            "<u>para</u>\n\n    code line\n",
            "para\n\n    code line\n",
        ),
        ("~~~\ncode\n~~~\n", "strike", "~~~\ncode\n~~~\n", "~~~\ncode\n~~~\n"),
        // `~~` wrapped in `~~` is a `~~~~~~` fence line, not strikethrough:
        // the trial parse rejects it, so only `a` is struck.
        ("a\n~~\n", "strike", "~~a~~\n~~\n", "a\n~~\n"),
        (
            "text[^1]\n\n[^1]: note\n",
            "italic",
            "*text[^1]*\n\n*[^1]: note*\n",
            "text[^1]\n\n[^1]: note\n",
        ),
    ];
    for (source, cmd, first, second) in cases {
        for mode in [ViewMode::Wysiwyg, ViewMode::Raw] {
            let mut doc = DocumentTab::new("p".into(), "p.md".into(), source.into(), None, None);
            doc.view_mode = mode;
            doc.caret.range = 0..doc.content.len();
            doc.apply_format_in_mode(cmd, mode);
            assert_eq!(doc.content, first, "first {cmd} {mode:?} on {source:?}");
            doc.caret.range = 0..doc.content.len();
            doc.apply_format_in_mode(cmd, mode);
            assert_eq!(doc.content, second, "second {cmd} {mode:?} on {source:?}");
        }
    }
}

/// Inline code is always the innermost format with its backticks adjacent
/// to the text, and no other delimiters are ever emitted inside a code
/// span: when a non-code format turns off mid-typing, the code span
/// closes, the format closes, and the code span reopens
/// (`**`hi`**`yo`). Covers every pair and triple containing code.
#[test]
fn code_stays_innermost_when_a_non_code_format_toggles_off() {
    // (armed formats, format toggled off mid-typing, expected content,
    //  flags of "hi", flags of "yo" as (bold, italic, underline, strike, code))
    type CodeCombo = (
        &'static [&'static str],
        &'static str,
        &'static str,
        (bool, bool, bool, bool, bool),
        (bool, bool, bool, bool, bool),
    );
    let combos: &[CodeCombo] = &[
        (
            &["bold", "code"],
            "bold",
            "**`hi`**`yo`",
            (true, false, false, false, true),
            (false, false, false, false, true),
        ),
        (
            &["italic", "code"],
            "italic",
            "*`hi`*`yo`",
            (false, true, false, false, true),
            (false, false, false, false, true),
        ),
        (
            &["underline", "code"],
            "underline",
            "<u>`hi`</u>`yo`",
            (false, false, true, false, true),
            (false, false, false, false, true),
        ),
        (
            &["strike", "code"],
            "strike",
            "~~`hi`~~`yo`",
            (false, false, false, true, true),
            (false, false, false, false, true),
        ),
        (
            &["bold", "italic", "code"],
            "bold",
            "***`hi`**`yo`*",
            (true, true, false, false, true),
            (false, true, false, false, true),
        ),
        (
            &["bold", "underline", "code"],
            "bold",
            "<u>**`hi`**`yo`</u>",
            (true, false, true, false, true),
            (false, false, true, false, true),
        ),
        (
            &["bold", "strike", "code"],
            "bold",
            "~~**`hi`**`yo`~~",
            (true, false, false, true, true),
            (false, false, false, true, true),
        ),
        (
            &["italic", "underline", "code"],
            "italic",
            "<u>*`hi`*`yo`</u>",
            (false, true, true, false, true),
            (false, false, true, false, true),
        ),
        (
            &["italic", "strike", "code"],
            "italic",
            "~~*`hi`*`yo`~~",
            (false, true, false, true, true),
            (false, false, false, true, true),
        ),
        (
            &["underline", "strike", "code"],
            "underline",
            "~~<u>`hi`</u>`yo`~~",
            (false, false, true, true, true),
            (false, false, false, true, true),
        ),
    ];

    for (formats, toggle_off, expected, hi_flags, yo_flags) in combos {
        let mut doc = DocumentTab::new("c".into(), "test.md".into(), String::new(), None, None);
        type_keystrokes(&mut doc, "x ");
        for format in *formats {
            doc.apply_format_in_mode(format, ViewMode::Wysiwyg);
        }
        type_keystrokes(&mut doc, "hi");
        let mut flags = FormatFlags::default();
        for format in *formats {
            match *format {
                "bold" => flags.bold = true,
                "italic" => flags.italic = true,
                "underline" => flags.underline = true,
                "strike" => flags.strike = true,
                "code" => flags.code = true,
                _ => {}
            }
        }
        let (open, close) = wrap_delimiters(&flags);
        let wrapped = format!("x {}hi{}", open, close);
        assert_eq!(
            doc.content, wrapped,
            "{formats:?}: typing did not wrap with the code span innermost"
        );

        doc.apply_format_in_mode(toggle_off, ViewMode::Wysiwyg);
        type_keystrokes(&mut doc, "yo");
        assert_eq!(
            doc.content,
            format!("x {expected}"),
            "{formats:?}: toggling off {toggle_off} misplaced the delimiters"
        );

        // The saved Markdown renders as styled text, never as literal
        // markers inside a code span.
        let parsed = parse_document(&doc.content);
        let segments: Vec<(&str, bool, bool, bool, bool, bool)> = parsed.lines[0]
            .styled_segments
            .iter()
            .map(|s| {
                (
                    &doc.content[s.source.clone()],
                    s.bold,
                    s.italic,
                    s.underline,
                    s.strike,
                    s.code,
                )
            })
            .collect();
        let hi = segments
            .iter()
            .find(|(text, ..)| *text == "hi")
            .unwrap_or_else(|| panic!("{formats:?}: no segment for hi in {segments:?}"));
        let yo = segments
            .iter()
            .find(|(text, ..)| *text == "yo")
            .unwrap_or_else(|| panic!("{formats:?}: no segment for yo in {segments:?}"));
        assert_eq!(
            (hi.1, hi.2, hi.3, hi.4, hi.5),
            *hi_flags,
            "{formats:?}: wrong styles on hi"
        );
        assert_eq!(
            (yo.1, yo.2, yo.3, yo.4, yo.5),
            *yo_flags,
            "{formats:?}: wrong styles on yo"
        );
    }
}

/// No styled segment may render a delimiter run as literal text, and no
/// edit may write a zero-width space into the content.
fn assert_no_delimiter_runs(doc: &DocumentTab) {
    assert!(
        !doc.content.contains('\u{200B}'),
        "zero-width space written into {:?}",
        doc.content
    );
    let content = doc.content.clone();
    let parsed = parse_document(&content);
    for line in &parsed.lines {
        for seg in &line.styled_segments {
            let text = &content[seg.source.clone()];
            assert!(
                !text.contains("**") && !text.contains("~~"),
                "literal delimiter rendered in {text:?} of {content:?}"
            );
        }
    }
}

/// Delete skips hidden delimiters like Backspace does and drops a span
/// whose last visible grapheme it removes, instead of eating marker
/// characters (`x **ab**` must not become `xy*ab**`).
#[test]
fn delete_forward_skips_delimiters_and_drops_empty_spans() {
    let mut doc = DocumentTab::new("d1".into(), "test.md".into(), "x **ab**".into(), None, None);
    doc.place(1, false);
    doc.delete_forward_in_mode(ViewMode::Wysiwyg);
    assert_eq!(doc.content, "x**ab**", "Delete ate a visible character");
    doc.delete_forward_in_mode(ViewMode::Wysiwyg);
    assert_eq!(doc.content, "x**b**", "Delete ate an opener character instead of skipping it");
    doc.delete_forward_in_mode(ViewMode::Wysiwyg);
    assert_eq!(
        doc.content, "x",
        "Delete did not drop the span once it was empty"
    );
    let parsed = parse_document(&doc.content);
    assert!(parsed.lines[0].styled_segments.iter().all(|s| !s.bold));
}

/// Backspace through a stacked span never splits a delimiter: the empty
/// span is dropped whole, and text typed afterwards stays outside the
/// tags (`<u>z/u>` must be impossible).
#[test]
fn backspace_through_stacked_span_drops_it_whole() {
    let mut doc = DocumentTab::new("d2".into(), "test.md".into(), String::new(), None, None);
    for f in ["bold", "italic", "underline"] {
        doc.apply_format_in_mode(f, ViewMode::Wysiwyg);
    }
    type_keystrokes(&mut doc, "ab");
    assert_eq!(doc.content, "<u>***ab***</u>");
    for f in ["bold", "italic", "underline"] {
        doc.apply_format_in_mode(f, ViewMode::Wysiwyg);
    }
    for _ in 0..3 {
        doc.smart_backspace_in_mode(ViewMode::Wysiwyg);
    }
    assert_eq!(doc.content, "", "backspace split a delimiter");
    type_keystrokes(&mut doc, "z");
    assert_eq!(doc.content, "z");
    assert_no_delimiter_runs(&doc);
}

/// Overlapping toggles close and reopen their markers so the spans nest
/// properly and round-trip through the parser: each variant below must
/// display without raw markers and parse back to the typed spans, with
/// no zero-width spaces and no literal delimiter runs.
#[test]
fn overlapping_toggles_nest_properly() {
    // (first format, second format, typed chars, expected spans as
    // (text, bold, italic, underline, strike))
    type OverlapVariant = (
        &'static str,
        &'static str,
        Vec<&'static str>,
        Vec<(&'static str, bool, bool, bool, bool)>,
    );
    let variants: Vec<OverlapVariant> = vec![
        (
            // 1-char: a italic, b bold+italic, c bold, d plain
            "italic",
            "bold",
            vec!["a", "b", "c", "d"],
            vec![
                ("a", false, true, false, false),
                ("b", true, true, false, false),
                ("c", true, false, false, false),
                ("d", false, false, false, false),
            ],
        ),
        (
            // 2-char variant of the same overlap
            "italic",
            "bold",
            vec!["ab", "cd", "ef", "gh"],
            vec![
                ("ab", false, true, false, false),
                ("cd", true, true, false, false),
                ("ef", true, false, false, false),
                ("gh", false, false, false, false),
            ],
        ),
        (
            // mirror order: bold first, then italic
            "bold",
            "italic",
            vec!["a", "b", "c", "d"],
            vec![
                ("a", true, false, false, false),
                ("b", true, true, false, false),
                ("c", false, true, false, false),
                ("d", false, false, false, false),
            ],
        ),
        (
            // strike first, then underline
            "strike",
            "underline",
            vec!["a", "b", "c", "d"],
            vec![
                ("a", false, false, false, true),
                ("b", false, false, true, true),
                ("c", false, false, true, false),
                ("d", false, false, false, false),
            ],
        ),
        (
            // underline first, then strike
            "underline",
            "strike",
            vec!["a", "b", "c", "d"],
            vec![
                ("a", false, false, true, false),
                ("b", false, false, true, true),
                ("c", false, false, false, true),
                ("d", false, false, false, false),
            ],
        ),
    ];
    for (first, second, chars, expected) in variants {
        let mut doc = DocumentTab::new("d3".into(), "test.md".into(), String::new(), None, None);
        doc.apply_format_in_mode(first, ViewMode::Wysiwyg);
        type_keystrokes(&mut doc, chars[0]);
        doc.apply_format_in_mode(second, ViewMode::Wysiwyg);
        type_keystrokes(&mut doc, chars[1]);
        doc.apply_format_in_mode(first, ViewMode::Wysiwyg);
        type_keystrokes(&mut doc, chars[2]);
        doc.apply_format_in_mode(second, ViewMode::Wysiwyg);
        type_keystrokes(&mut doc, chars[3]);

        assert!(
            !doc.content.contains('\u{200B}'),
            "{first}/{second}: zero-width space written"
        );
        let content = doc.content.clone();
        let parsed = parse_document(&content);
        assert_eq!(
            parsed.lines[0].display_text,
            chars.concat(),
            "{first}/{second}: raw markers visible in the display"
        );
        let segments: Vec<(&str, bool, bool, bool, bool)> = parsed.lines[0]
            .styled_segments
            .iter()
            .map(|s| {
                (
                    content[s.source.clone()].as_ref(),
                    s.bold,
                    s.italic,
                    s.underline,
                    s.strike,
                )
            })
            .collect();
        assert_eq!(segments, expected, "{first}/{second}: wrong spans");
        assert_no_delimiter_runs(&doc);
    }
}

/// Toggling a non-code format while code is armed splits the code span
/// around the change instead of nesting delimiters inside the backticks;
/// the code span stays innermost and adjacent to the text.
#[test]
fn toggles_split_code_span_keeping_code_innermost() {
    for (format, open, close) in [
        ("bold", "**", "**"),
        ("italic", "*", "*"),
        ("underline", "<u>", "</u>"),
        ("strike", "~~", "~~"),
    ] {
        let mut doc = DocumentTab::new("d4".into(), "test.md".into(), String::new(), None, None);
        doc.apply_format_in_mode("code", ViewMode::Wysiwyg);
        type_keystrokes(&mut doc, "a");
        doc.apply_format_in_mode(format, ViewMode::Wysiwyg);
        type_keystrokes(&mut doc, "b");
        doc.apply_format_in_mode(format, ViewMode::Wysiwyg);
        type_keystrokes(&mut doc, "c");
        assert_eq!(
            doc.content,
            format!("`a`{open}`b`{close}`c`"),
            "{format}: delimiters nested inside the code span"
        );

        let content = doc.content.clone();
        let parsed = parse_document(&content);
        let segments: Vec<(&str, bool, bool, bool, bool, bool)> = parsed.lines[0]
            .styled_segments
            .iter()
            .map(|s| {
                (
                    content[s.source.clone()].trim_matches('\u{200B}'),
                    s.bold,
                    s.italic,
                    s.underline,
                    s.strike,
                    s.code,
                )
            })
            .collect();
        let want = match format {
            "bold" => vec![("a", false, false, false, false, true), ("b", true, false, false, false, true), ("c", false, false, false, false, true)],
            "italic" => vec![("a", false, false, false, false, true), ("b", false, true, false, false, true), ("c", false, false, false, false, true)],
            "underline" => vec![("a", false, false, false, false, true), ("b", false, false, true, false, true), ("c", false, false, false, false, true)],
            _ => vec![("a", false, false, false, false, true), ("b", false, false, false, true, true), ("c", false, false, false, false, true)],
        };
        assert_eq!(segments, want, "{format}: wrong styles");
        assert_no_delimiter_runs(&doc);
    }
}

/// A delimiter after an emoji or symbol and before a letter loses its
/// flanking (CommonMark counts symbols as punctuation). The document
/// text stays exactly as typed - no zero-width separators - and the
/// parser synthesizes the pair for the display, on the closer and the
/// opener side alike.
#[test]
fn symbol_adjacent_delimiters_keep_flanking() {
    // Closer side: bold emoji, toggle off, type a letter.
    let mut doc = DocumentTab::new("d5".into(), "test.md".into(), String::new(), None, None);
    doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
    type_keystrokes(&mut doc, "\u{1F600}\u{1F600}");
    doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
    assert_eq!(doc.content, "**\u{1F600}\u{1F600}**");
    type_keystrokes(&mut doc, "x");
    assert_eq!(
        doc.content, "**\u{1F600}\u{1F600}**x",
        "the typed text was altered"
    );
    assert!(
        !doc.content.contains('\u{200B}'),
        "zero-width space written"
    );
    let content = doc.content.clone();
    let parsed = parse_document(&content);
    let segments: Vec<(&str, bool)> = parsed.lines[0]
        .styled_segments
        .iter()
        .map(|s| (content[s.source.clone()].as_ref(), s.bold))
        .collect();
    assert_eq!(segments, vec![("\u{1F600}\u{1F600}", true), ("x", false)]);

    // Opener side: a letter right before a fresh span holding a symbol.
    let mut doc = DocumentTab::new("d6".into(), "test.md".into(), "x".into(), None, None);
    doc.place(1, false);
    doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
    type_keystrokes(&mut doc, "\u{1F600}");
    assert_eq!(doc.content, "x**\u{1F600}**");
    assert!(
        !doc.content.contains('\u{200B}'),
        "zero-width space written"
    );
    let content = doc.content.clone();
    let parsed = parse_document(&content);
    let segments: Vec<(&str, bool)> = parsed.lines[0]
        .styled_segments
        .iter()
        .map(|s| (content[s.source.clone()].as_ref(), s.bold))
        .collect();
    assert_eq!(segments, vec![("x", false), ("\u{1F600}", true)]);
}

/// Typing a span's own delimiter character inside it escapes it (or
/// re-fences a code span) so the span stays intact and renders styled.
#[test]
fn typing_span_delimiters_inside_keeps_the_span() {
    // Strike: `~` inside, then a space and more text.
    let mut doc = DocumentTab::new("d7".into(), "test.md".into(), String::new(), None, None);
    doc.apply_format_in_mode("strike", ViewMode::Wysiwyg);
    type_keystrokes(&mut doc, "a");
    type_keystrokes(&mut doc, "~");
    assert_eq!(doc.content, "~~a\\~~~", "the tilde was not escaped");
    type_keystrokes(&mut doc, " ");
    type_keystrokes(&mut doc, "b");
    doc.apply_format_in_mode("strike", ViewMode::Wysiwyg);
    assert_eq!(doc.content, "~~a\\~ b~~");
    let content = doc.content.clone();
    let parsed = parse_document(&content);
    let segments: Vec<(&str, bool)> = parsed.lines[0]
        .styled_segments
        .iter()
        .map(|s| (content[s.source.clone()].trim_matches('\\'), s.strike))
        .collect();
    assert_eq!(segments, vec![("a", true), ("~ b", true)]);

    // Italic: `*` inside.
    let mut doc = DocumentTab::new("d8".into(), "test.md".into(), String::new(), None, None);
    doc.apply_format_in_mode("italic", ViewMode::Wysiwyg);
    type_keystrokes(&mut doc, "a");
    type_keystrokes(&mut doc, "*");
    type_keystrokes(&mut doc, "b");
    assert_eq!(doc.content, "*a\\*b*");
    let content = doc.content.clone();
    let parsed = parse_document(&content);
    let segments: Vec<(&str, bool)> = parsed.lines[0]
        .styled_segments
        .iter()
        .map(|s| (content[s.source.clone()].trim_matches('\\'), s.italic))
        .collect();
    assert_eq!(segments, vec![("a", true), ("*b", true)]);

    // Code: a backtick typed inside the content re-fences the span with
    // a longer run; a backtick typed at the content's end acts as the
    // closing fence and moves the caret past it.
    let mut doc = DocumentTab::new("d9".into(), "test.md".into(), String::new(), None, None);
    doc.apply_format_in_mode("code", ViewMode::Wysiwyg);
    type_keystrokes(&mut doc, "ab");
    doc.place(2, false);
    type_keystrokes(&mut doc, "`");
    assert_eq!(
        doc.content, "``a`b``",
        "the backtick inside the code span did not re-fence"
    );
    let content = doc.content.clone();
    let parsed = parse_document(&content);
    let segments: Vec<(&str, bool)> = parsed.lines[0]
        .styled_segments
        .iter()
        .map(|s| (content[s.source.clone()].as_ref(), s.code))
        .collect();
    assert_eq!(segments, vec![("a`b", true)]);

    let mut doc = DocumentTab::new("d10".into(), "test.md".into(), String::new(), None, None);
    doc.apply_format_in_mode("code", ViewMode::Wysiwyg);
    type_keystrokes(&mut doc, "a");
    type_keystrokes(&mut doc, "`");
    assert_eq!(
        doc.content, "``a```",
        "the backtick did not join the content"
    );
    let parsed = doc.parsed();
    assert!(
        parsed.lines[0]
            .styled_segments
            .iter()
            .any(|s| s.code && doc.content[s.source.clone()] == *"a`"),
        "the re-fenced span is not displayed as code"
    );
    doc.apply_format_in_mode("code", ViewMode::Wysiwyg);
    assert_eq!(doc.cursor_offset(), 6, "the shortcut did not exit the span");
    type_keystrokes(&mut doc, "b");
    doc.apply_format_in_mode("code", ViewMode::Wysiwyg);
    type_keystrokes(&mut doc, "c");
    assert_eq!(doc.content, "``a```b`c`");
    let content = doc.content.clone();
    let parsed = parse_document(&content);
    let segments: Vec<(&str, bool)> = parsed.lines[0]
        .styled_segments
        .iter()
        .map(|s| (content[s.source.clone()].as_ref(), s.code))
        .collect();
    assert_eq!(
        segments,
        vec![("a`", true), ("b", false), ("c", true)],
        "the literal backtick and the code spans rendered wrong"
    );
    assert_no_delimiter_runs(&doc);
}

/// The wrap path escapes the armed span's own delimiters in typed text
/// (strike: `\~`), a literally typed `<` that would start an HTML tag is
/// escaped as `\<`, and escape backslashes are hidden in the display
/// with the caret mapping intact. No zero-width space is ever written.
#[test]
fn wrap_escapes_delimiters_lt_and_hides_escape_backslashes() {
    // Strike armed, text containing tildes (the wrap path).
    let mut doc = DocumentTab::new("w1".into(), "test.md".into(), String::new(), None, None);
    doc.apply_format_in_mode("strike", ViewMode::Wysiwyg);
    type_keystrokes(&mut doc, "~a~");
    assert_eq!(doc.content, "~~\\~a\\~~~", "the tildes were not escaped");
    doc.apply_format_in_mode("strike", ViewMode::Wysiwyg);
    type_keystrokes(&mut doc, " b");
    assert_eq!(doc.content, "~~\\~a\\~~~ b");
    assert!(!doc.content.contains('\u{200B}'));
    let content = doc.content.clone();
    let parsed = parse_document(&content);
    let segments: Vec<(&str, bool)> = parsed.lines[0]
        .styled_segments
        .iter()
        .map(|s| (content[s.source.clone()].trim_matches('\\'), s.strike))
        .collect();
    assert_eq!(segments, vec![("~a", true), ("~", true), (" b", false)]);
    assert_no_delimiter_runs(&doc);

    // `<` typed before a letter, and a letter typed after a `<`.
    let mut doc = DocumentTab::new("w2".into(), "test.md".into(), String::new(), None, None);
    type_keystrokes(&mut doc, "a");
    type_keystrokes(&mut doc, "<");
    type_keystrokes(&mut doc, "b");
    assert_eq!(doc.content, "a\\<b", "the `<` was not escaped");
    assert_eq!(parse_document(&doc.content).lines[0].display_text, "a<b");

    let mut doc = DocumentTab::new("w3".into(), "test.md".into(), String::new(), None, None);
    type_keystrokes(&mut doc, "<");
    type_keystrokes(&mut doc, "b");
    assert_eq!(doc.content, "\\<b", "the `<` was not escaped");
    assert_eq!(parse_document(&doc.content).lines[0].display_text, "<b");

    // Escape backslashes are hidden in the display and the offsets map.
    let mut doc = DocumentTab::new(
        "w4".into(),
        "test.md".into(),
        "**a\\*b**".into(),
        None,
        None,
    );
    doc.place(3, false);
    let parsed = doc.parsed();
    let line = &parsed.lines[0];
    assert_eq!(line.display_text, "a*b");
    for (idx, ch) in line.display_text.char_indices() {
        let src = line.offset_map.display_to_source(idx);
        assert!(
            doc.content.is_char_boundary(src) && doc.content[src..].starts_with(ch),
            "display {ch:?} at {idx} maps to {src}"
        );
    }
    assert!(!doc.content.contains('\u{200B}'));
}

#[test]
fn per_line_hidden_skips_match_a_document_wide_scan() {
    let source = "# **Head** ##\n> *quote* and `code`\n\n- [link](http://x) **b**\n\
                      | **a** | *b* |\n| --- | --- |\n| `c` | [d](e) |\n---\n```\nfn x\n```\nend ~~s~~";
    let parsed = parse_document(source);
    let all: Vec<Range<usize>> = parsed
        .lines
        .iter()
        .flat_map(|l| l.hidden_ranges.iter().cloned())
        .collect();
    let skip = |mut pos: usize, hit: &dyn Fn(usize, &Range<usize>) -> bool, left: bool| {
        while let Some(r) = all.iter().find(|r| hit(pos, r)) {
            pos = if left { r.start } else { r.end };
        }
        pos
    };
    for pos in 0..=source.len() {
        assert_eq!(
            skip_hidden_left(&parsed, pos),
            skip(pos, &|p, r| p > r.start && p <= r.end, true),
            "left at {pos}"
        );
        assert_eq!(
            skip_hidden_interior_left(&parsed, pos),
            skip(pos, &|p, r| p > r.start && p < r.end, true),
            "interior left at {pos}"
        );
        assert_eq!(
            next_visible_pos(&parsed, pos),
            skip(pos, &|p, r| p >= r.start && p < r.end, false),
            "next visible at {pos}"
        );
    }
}

#[test]
fn test_wysiwyg_delimiter_skipping_navigation_and_atomic_backspace() {
    let mut doc =
        DocumentTab::new("n".into(), "test.md".into(), "a **b** c".into(), None, None);
    doc.place(0, false);

    // Visible text is "a b c"
    // Step right across visible characters: 'a' -> ' ' -> 'b' -> ' ' -> 'c'
    doc.move_right_in_mode(false, ViewMode::Wysiwyg);
    assert_eq!(doc.cursor_offset(), 1); // after 'a'
    doc.move_right_in_mode(false, ViewMode::Wysiwyg);
    assert_eq!(doc.cursor_offset(), 4); // skipped space and opening "**" to land before 'b'
    doc.move_right_in_mode(false, ViewMode::Wysiwyg);
    assert_eq!(doc.cursor_offset(), 7); // stepped over 'b' and skipped closing "**"

    // Step left back onto 'b'
    doc.move_left_in_mode(false, ViewMode::Wysiwyg);
    assert_eq!(doc.cursor_offset(), 4); // before 'b'

    // Place right after 'b' (offset 5 or 7) and atomic backspace the single-char bold span
    doc.place(5, false);
    doc.smart_backspace_in_mode(ViewMode::Wysiwyg);
    assert_eq!(doc.content, "a  c");
}

#[test]
fn test_table_cell_tab_navigation() {
    let table = "| Name | Role |\n| --- | --- |\n| Alice | Admin |";
    let mut doc = DocumentTab::new("tbl".into(), "test.md".into(), table.into(), None, None);
    doc.place(2, false); // inside "Name"

    assert!(doc.tab_table_cell(false));
    assert_eq!(doc.selected_text(), "Role");

    assert!(doc.tab_table_cell(false));
    assert_eq!(doc.selected_text(), "Alice");

    assert!(doc.tab_table_cell(false));
    assert_eq!(doc.selected_text(), "Admin");

    // Shift+Tab goes backwards
    assert!(doc.tab_table_cell(true));
    assert_eq!(doc.selected_text(), "Alice");

    // Forward Tab to Admin, then forward Tab on last cell appends a new row
    assert!(doc.tab_table_cell(false));
    assert_eq!(doc.selected_text(), "Admin");
    assert!(doc.tab_table_cell(false));
    assert!(doc.content.ends_with("\n|  |  |"));
}

#[test]
fn test_table_insertion_and_row_addition() {
    let mut doc = DocumentTab::new("t".into(), "test.md".into(), String::new(), None, None);
    doc.apply_format_in_mode("table", ViewMode::Wysiwyg);
    assert_eq!(
        doc.content,
        "| Column 1 | Column 2 |\n| :--- | :--- |\n| Item 1 | Item 2 |"
    );
    assert_eq!(doc.selected_text(), "Column 1");

    // Calling "table" format while inside a table appends a new row
    doc.apply_format_in_mode("table", ViewMode::Wysiwyg);
    assert!(doc.content.ends_with("\n|  |  |"));
}

#[test]
fn test_table_smart_newline_continuation_and_exit() {
    let table = "| Col 1 | Col 2 |\n| :--- | :--- |\n| Item 1 | Item 2 |";
    let mut doc = DocumentTab::new("t".into(), "test.md".into(), table.into(), None, None);
    doc.place(table.len(), false); // at end of "Item 2 |"

    // Press Enter on non-empty row -> adds a new row
    doc.smart_newline(false);
    assert!(doc.content.ends_with("\n|  |  |"));

    // Press Enter on the new empty row -> removes it and exits table to a newline after table
    doc.smart_newline(false);
    assert_eq!(doc.content, format!("{}\n\n", table));
    assert_eq!(doc.cursor_offset(), table.len() + 2);

    // Pressing Enter again outside table does not add a row to the table
    doc.smart_newline(false);
    assert_eq!(doc.content, format!("{}\n\n\n", table));
}

#[test]
fn test_table_single_empty_row_exit_keeps_header_and_removes_empty_row() {
    // Table with only a single empty body row
    let table = "| Col 1 | Col 2 |\n| :--- | :--- |\n|  |  |";
    let mut doc = DocumentTab::new("t".into(), "test.md".into(), table.into(), None, None);
    doc.place(table.len(), false); // inside empty row

    doc.smart_newline(false);
    // Exiting the only empty row keeps the header and only removes the empty row!
    assert_eq!(doc.content, "| Col 1 | Col 2 |\n| :--- | :--- |\n\n");
    assert_eq!(doc.cursor_offset(), doc.content.len());
}

#[test]
fn test_table_enter_splits_cell() {
    let table = "| Col 1 | Col 2 |\n| :--- | :--- |\n| First item | Second item |";
    let mut doc = DocumentTab::new("t".into(), "test.md".into(), table.into(), None, None);
    // Place cursor between "First " and "item" in Col 1
    let pos = table.find("First ").unwrap() + "First ".len();
    doc.place(pos, false);

    doc.smart_newline(false);
    let expected = "| Col 1 | Col 2 |\n| :--- | :--- |\n| First | Second item |\n| item |  |";
    assert_eq!(doc.content, expected);
    assert_eq!(doc.cursor_offset(), expected.find("item |  |").unwrap());
}

#[test]
fn test_insert_table_with_size_and_undo() {
    let mut doc = DocumentTab::new("t".into(), "test.md".into(), String::new(), None, None);
    doc.insert_table_with_size(3, 4);
    assert_eq!(
        doc.content,
        "|  |  |  |\n| :--- | :--- | :--- |\n|  |  |  |\n|  |  |  |\n|  |  |  |"
    );
    assert_eq!(doc.selected_text(), "");
    assert_eq!(doc.cursor_offset(), 2);

    // Undo in a single step restores initial state
    assert!(doc.undo());
    assert_eq!(doc.content, "");
}

#[test]
fn test_table_row_and_column_manipulations_and_undo() {
    let initial_table = "| Column 1 | Column 2 |\n| :--- | :--- |\n| Item 1 | Item 2 |";
    let mut doc = DocumentTab::new(
        "t".into(),
        "test.md".into(),
        initial_table.into(),
        None,
        None,
    );

    // Place cursor in Item 1
    let pos = doc.content.find("Item 1").unwrap() + 2;
    doc.place(pos, false);

    // Add row above
    doc.table_add_row_above();
    assert!(doc
        .content
        .contains("| Column 1 | Column 2 |\n| :--- | :--- |\n|  |  |\n| Item 1 | Item 2 |"));
    assert!(doc.undo());
    assert_eq!(doc.content, initial_table);

    // Add row below
    let pos = doc.content.find("Item 1").unwrap() + 2;
    doc.place(pos, false);
    doc.table_add_row_below();
    assert!(doc.content.ends_with("\n|  |  |"));
    assert!(doc.undo());
    assert_eq!(doc.content, initial_table);

    // Add column right
    let pos = doc.content.find("Item 1").unwrap() + 2;
    doc.place(pos, false);
    doc.table_add_col_right();
    assert!(doc.content.contains("| Item 1 |  | Item 2 |"));
    assert!(doc.undo());
    assert_eq!(doc.content, initial_table);

    // Add column left
    let pos = doc.content.find("Item 1").unwrap() + 2;
    doc.place(pos, false);
    doc.table_add_col_left();
    assert!(doc.content.contains("|  | Item 1 | Item 2 |"));
    assert!(doc.undo());
    assert_eq!(doc.content, initial_table);

    // Align column right
    let pos = doc.content.find("Item 1").unwrap() + 2;
    doc.place(pos, false);
    doc.table_align_column(TableColumnAlign::Right);
    assert!(doc.content.contains("| ---: | :--- |"));
    assert!(doc.undo());
    assert!(doc.content.contains("| :--- | :--- |"));

    // Delete row
    let pos = doc.content.find("Item 1").unwrap() + 2;
    doc.place(pos, false);
    doc.table_delete_row();
    assert_eq!(doc.content, "| Column 1 | Column 2 |\n| :--- | :--- |");
    assert!(doc.undo());
    assert_eq!(doc.content, initial_table);

    // Delete table
    let pos = doc.content.find("Item 1").unwrap() + 2;
    doc.place(pos, false);
    doc.table_delete_table();
    assert_eq!(doc.content, "");
    assert!(doc.undo());
    assert_eq!(doc.content, initial_table);
}

#[test]
fn test_table_edit_keeps_escaped_pipe() {
    let table = "| a \\| b | c |\n| --- | --- |\n| d | e |";
    let mut doc = DocumentTab::new("t".into(), "test.md".into(), table.into(), None, None);
    doc.place(doc.content.find('a').unwrap(), false);
    doc.table_add_col_right();
    assert_eq!(
        doc.content,
        "| a \\| b |  | c |\n| --- | --- | --- |\n| d |  | e |"
    );
    assert!(!doc.content.contains("| a \\ |"));
}

#[test]
fn test_table_edit_keeps_unset_alignment() {
    let table = "| a | b |\n| --- | --- |\n| 1 | 2 |";
    let mut doc = DocumentTab::new("t".into(), "test.md".into(), table.into(), None, None);
    doc.place(doc.content.find('a').unwrap(), false);
    doc.table_add_col_right();
    assert!(doc.content.contains("| --- | --- | --- |"));
    assert!(!doc.content.contains(":---"));

    let mixed = "| a | b |\n| :--- | --- |\n| 1 | 2 |";
    let mut doc = DocumentTab::new("t".into(), "test.md".into(), mixed.into(), None, None);
    doc.place(doc.content.find('1').unwrap(), false);
    doc.table_add_row_below();
    assert!(doc.content.contains("| :--- | --- |"));
    assert!(!doc.content.contains("| :--- | :--- |"));
    assert_eq!(doc.content, "| a | b |\n| :--- | --- |\n| 1 | 2 |\n|  |  |");
}

#[test]
fn test_wysiwyg_table_boundary_guards() {
    let table = "| Col 1 | Col 2 |\n| :--- | :--- |\n| Item 1 | Item 2 |";
    let mut doc = DocumentTab::new("t".into(), "test.md".into(), table.into(), None, None);

    // Position at start of "Item 1" (right after leading pipe/space)
    let pos = table.find("Item 1").unwrap();
    doc.place(pos, false);

    // smart_backspace in WYSIWYG mode should NOT delete the delimiter or merge with previous cell
    doc.smart_backspace_in_mode(ViewMode::Wysiwyg);
    assert_eq!(doc.content, table);
    assert_eq!(doc.cursor_offset(), pos);

    // Position at end of "Item 1"
    let pos = table.find("Item 1").unwrap() + "Item 1".len();
    doc.place(pos, false);

    // delete_forward in WYSIWYG mode should NOT delete the delimiter
    doc.delete_forward_in_mode(ViewMode::Wysiwyg);
    assert_eq!(doc.content, table);
    assert_eq!(doc.cursor_offset(), pos);
}

#[test]
fn test_enter_on_indented_empty_quote_leaves_quote() {
    let mut doc = DocumentTab::new("t".into(), "test.md".into(), "  > ".into(), None, None);
    doc.place("  > ".len(), false);

    // Enter on indented empty quote removes the quote line, leaving the quote completely
    doc.smart_newline(false);
    assert_eq!(doc.content, "");
}

#[test]
fn test_enter_on_empty_list_item_with_selection() {
    let mut doc = DocumentTab::new("t".into(), "test.md".into(), "- ".into(), None, None);
    // Select the whole "- "
    doc.caret.range = 0..2;

    // Enter on empty list item with selection must replace selection with newline
    doc.smart_newline(false);
    assert_eq!(doc.content, "\n");
}

#[test]
fn test_task_list_asterisk_continuation() {
    let mut doc = DocumentTab::new(
        "t".into(),
        "test.md".into(),
        "* [x] done".into(),
        None,
        None,
    );
    doc.place("* [x] done".len(), false);

    // Enter on "* [x] done" must continue with "* [ ] ", not "- [ ] "
    doc.smart_newline(false);
    assert_eq!(doc.content, "* [x] done\n* [ ] ");
}

#[test]
fn test_emoji_formatting_no_crash() {
    let content = "🦀hello world";
    let mut doc = DocumentTab::new("t".into(), "test.md".into(), content.into(), None, None);
    // Place caret right after emoji (at byte 4)
    doc.place(4, false);

    // Press Ctrl+U (underline) in Wysiwyg mode next to emoji -> must not crash!
    doc.apply_format_in_mode("underline", ViewMode::Wysiwyg);

    // Press Ctrl+I (italic) in Wysiwyg mode next to emoji -> must not crash!
    doc.apply_format_in_mode("italic", ViewMode::Wysiwyg);

    let content2 = "hello🦀";
    let mut doc2 = DocumentTab::new("t".into(), "test.md".into(), content2.into(), None, None);
    doc2.place(content2.len(), false);
    doc2.apply_format_in_mode("underline", ViewMode::Wysiwyg);
    doc2.apply_format_in_mode("italic", ViewMode::Wysiwyg);
}

#[test]
fn test_backspace_on_text_ending_with_dash_space() {
    let mut doc = DocumentTab::new("t".into(), "test.md".into(), "foo - ".into(), None, None);
    doc.place("foo - ".len(), false);

    // Backspacing on "foo - " must delete the space, NOT wipe out "foo - " to empty!
    doc.smart_backspace();
    assert_eq!(doc.content, "foo -");

    doc.smart_backspace();
    assert_eq!(doc.content, "foo ");
}

#[test]
fn test_enter_on_indented_bullet_tab() {
    let mut doc = DocumentTab::new("t".into(), "test.md".into(), "\t- ".into(), None, None);
    doc.place("\t- ".len(), false);

    // Enter on "\t- " must outdent to "- ", NOT wipe to empty string!
    doc.smart_newline(false);
    assert_eq!(doc.content, "- ");

    // Enter on root "- " exits the list, removing the marker
    doc.smart_newline(false);
    assert_eq!(doc.content, "");
}

fn edit_probe(source: &str) -> (usize, usize, String, String, String, usize, String) {
    let prefix_len = block_prefix_len(source);
    let marker_len = crate::model::text_ops::list_marker_len(source);
    let mut newline_doc =
        DocumentTab::new("n".into(), "t.md".into(), source.into(), None, None);
    newline_doc.place(source.len(), false);
    newline_doc.smart_newline(false);
    let mut backspace_doc =
        DocumentTab::new("b".into(), "t.md".into(), source.into(), None, None);
    backspace_doc.place(source.len(), false);
    backspace_doc.smart_backspace();
    let mut toggle_doc = DocumentTab::new("o".into(), "t.md".into(), source.into(), None, None);
    toggle_doc.place(source.len(), false);
    toggle_doc.apply_format_in_mode("ol", ViewMode::Wysiwyg);
    let mut home_doc = DocumentTab::new("h".into(), "t.md".into(), source.into(), None, None);
    home_doc.place(0, false);
    home_doc.move_home_in_mode(false, ViewMode::Wysiwyg);
    let mut typed = source.to_string();
    let at = if prefix_len > 0 { prefix_len } else { 0 };
    typed.insert(at, 'Z');
    (
        prefix_len,
        marker_len,
        newline_doc.content,
        backspace_doc.content,
        toggle_doc.content,
        home_doc.cursor_offset(),
        typed,
    )
}

#[test]
fn editing_helpers_accept_dot_and_paren() {
    let (prefix, marker, newline, _backspace, toggle, home, typed) = edit_probe("1. foo");
    assert_eq!(prefix, 3, "ed-dot prefix_len");
    assert_eq!(marker, 3, "ed-dot list_marker_len");
    assert_eq!(newline, "1. foo\n2. ", "ed-dot newline");
    assert_eq!(toggle, "foo", "ed-dot ol_toggle");
    assert_eq!(home, 3, "ed-dot home_from_0");
    assert_eq!(typed, "1. Zfoo", "ed-dot type_at_0");

    let (prefix, marker, newline, _backspace, toggle, home, typed) = edit_probe("1) foo");
    assert_eq!(prefix, 3, "ed-paren prefix_len");
    assert_eq!(marker, 3, "ed-paren list_marker_len");
    assert_eq!(newline, "1) foo\n2) ", "ed-paren newline");
    assert_eq!(toggle, "foo", "ed-paren ol_toggle");
    assert_eq!(home, 3, "ed-paren home_from_0");
    assert_eq!(typed, "1) Zfoo", "ed-paren type_at_0");

    let (_prefix, _marker, newline, backspace, _toggle, _home, _typed) = edit_probe("1. ");
    assert_eq!(newline, "", "ed-marker-dot newline");
    assert_eq!(backspace, "", "ed-marker-dot backspace");

    let (_prefix, _marker, newline, backspace, toggle, _home, typed) = edit_probe("1) ");
    assert_eq!(newline, "", "ed-marker-paren newline");
    assert_eq!(backspace, "", "ed-marker-paren backspace");
    assert_eq!(toggle, "", "ed-marker-paren ol_toggle");
    assert_eq!(typed, "1) Z", "ed-marker-paren type_at_0");

    let (prefix, marker, newline, _backspace, toggle, home, typed) = edit_probe("1. [ ] foo");
    assert_eq!(prefix, 7, "ed-task prefix_len");
    assert_eq!(marker, 7, "ed-task list_marker_len");
    assert_eq!(newline, "1. [ ] foo\n2. [ ] ", "ed-task newline");
    // The ordered-list button is off on an ordered task, so the click keeps the line.
    assert_eq!(toggle, "1. [ ] foo", "ed-task ol_toggle");
    assert_eq!(home, 7, "ed-task home_from_0");
    assert_eq!(typed, "1. [ ] Zfoo", "ed-task type_at_0");

    let (prefix, marker, newline, _backspace, toggle, home, typed) = edit_probe("1) [ ] foo");
    assert_eq!(prefix, 7, "ed-task-paren prefix_len");
    assert_eq!(marker, 7, "ed-task-paren list_marker_len");
    assert_eq!(newline, "1) [ ] foo\n2) [ ] ", "ed-task-paren newline");
    assert_eq!(toggle, "1) [ ] foo", "ed-task-paren ol_toggle");
    assert_eq!(home, 7, "ed-task-paren home_from_0");
    assert_eq!(typed, "1) [ ] Zfoo", "ed-task-paren type_at_0");

    let (prefix, marker, newline, _backspace, _toggle, home, typed) = edit_probe("- [ ] foo");
    assert_eq!(prefix, 6, "ed-bullet-task prefix_len");
    assert_eq!(marker, 6, "ed-bullet-task list_marker_len");
    assert_eq!(newline, "- [ ] foo\n- [ ] ", "ed-bullet-task newline");
    assert_eq!(home, 6, "ed-bullet-task home_from_0");
    assert_eq!(typed, "- [ ] Zfoo", "ed-bullet-task type_at_0");

    let (prefix, marker, newline, _backspace, toggle, home, typed) = edit_probe("10) foo");
    assert_eq!(prefix, 4, "ed-ten prefix_len");
    assert_eq!(marker, 4, "ed-ten list_marker_len");
    assert_eq!(newline, "10) foo\n11) ", "ed-ten newline");
    assert_eq!(toggle, "foo", "ed-ten ol_toggle");
    assert_eq!(home, 4, "ed-ten home_from_0");
    assert_eq!(typed, "10) Zfoo", "ed-ten type_at_0");

    let (prefix, _marker, newline, backspace, toggle, home, typed) = edit_probe("1)");
    assert_eq!(prefix, 2, "ed-empty-paren prefix_len");
    assert_eq!(newline, "", "ed-empty-paren newline");
    assert_eq!(backspace, "", "ed-empty-paren backspace");
    assert_eq!(toggle, "", "ed-empty-paren ol_toggle");
    assert_eq!(home, 2, "ed-empty-paren home_from_0");
    assert_eq!(typed, "1)Z", "ed-empty-paren type_at_0");

    let (prefix, _marker, newline, backspace, _toggle, home, typed) = edit_probe("1.");
    assert_eq!(prefix, 2, "ed-empty-dot prefix_len");
    assert_eq!(newline, "", "ed-empty-dot newline");
    assert_eq!(backspace, "", "ed-empty-dot backspace");
    assert_eq!(home, 2, "ed-empty-dot home_from_0");
    assert_eq!(typed, "1.Z", "ed-empty-dot type_at_0");
}

#[test]
fn test_enter_with_selection_is_single_undo_step() {
    let mut doc = DocumentTab::new(
        "t".into(),
        "test.md".into(),
        "hello world".into(),
        None,
        None,
    );
    doc.caret.range = 6..11;
    doc.smart_newline(false);
    assert_eq!(doc.content, "hello \n");
    assert!(doc.undo());
    assert_eq!(doc.content, "hello world");
    assert!(!doc.undo());
}

#[test]
fn unclosed_fence_toggle_unwraps_to_eof() {
    let mut doc = DocumentTab::new(
        "c".into(),
        "code.md".into(),
        "```\naaa\n".into(),
        None,
        None,
    );
    doc.place(5, false);
    doc.apply_format_in_mode("codeblock", ViewMode::Wysiwyg);
    assert_eq!(doc.content, "aaa\n");
}

#[test]
fn crlf_and_bare_cr_carets_stay_out_of_the_line_ending() {
    let crlf = "Foo\r\nbar\r\nbaz";
    let mut doc = DocumentTab::new("s".into(), "carets.md".into(), crlf.into(), None, None);
    doc.place(0, false);
    doc.move_end(false);
    assert_eq!(doc.cursor_offset(), 3, "End stops before CRLF");
    doc.insert_text_in_mode(
        doc.cursor_offset()..doc.cursor_offset(),
        "X",
        ViewMode::Wysiwyg,
    );
    assert_eq!(doc.content, "FooX\r\nbar\r\nbaz");

    let mut doc = DocumentTab::new("s".into(), "carets.md".into(), crlf.into(), None, None);
    doc.place(5, false);
    doc.move_end(false);
    assert_eq!(doc.cursor_offset(), 8, "End on bar stops before its CRLF");

    let bare = "Foo\rbar\rbaz";
    let mut doc = DocumentTab::new("s".into(), "carets.md".into(), bare.into(), None, None);
    doc.place(0, false);
    doc.move_end(false);
    assert_eq!(doc.cursor_offset(), 3);
    doc.insert_text_in_mode(3..3, "X", ViewMode::Wysiwyg);
    assert_eq!(doc.content, "FooX\rbar\rbaz");

    let mut doc = DocumentTab::new("s".into(), "carets.md".into(), bare.into(), None, None);
    doc.place(1, false);
    doc.delete_to_line_end();
    assert_eq!(doc.content, "F\rbar\rbaz");

    let mut doc = DocumentTab::new("s".into(), "carets.md".into(), bare.into(), None, None);
    doc.place(0, false);
    doc.move_down(false);
    assert_eq!(doc.cursor_offset(), 4, "Down crosses a bare CR");
    doc.move_end(false);
    assert_eq!(doc.cursor_offset(), 7, "End stops before the next bare CR");
    doc.move_up(false);
    assert_eq!(
        doc.cursor_offset(),
        3,
        "Up crosses a bare CR and keeps the end column"
    );

    let mut doc = DocumentTab::new("s".into(), "carets.md".into(), crlf.into(), None, None);
    doc.place(doc.content.len(), false);
    for _ in 0..4 {
        doc.move_left_in_mode(false, ViewMode::Wysiwyg);
    }
    assert_ne!(
        doc.cursor_offset(),
        9,
        "Left must not sit between CR and LF"
    );
    let at = doc.cursor_offset();
    doc.insert_text_in_mode(at..at, "X", ViewMode::Wysiwyg);
    assert!(
        !doc.content.contains("\rX\n"),
        "typed between CR and LF: {:?}",
        doc.content
    );
}

#[test]
fn move_line_keeps_crlf_and_bare_cr_endings() {
    let crlf = "Aaa\r\nBbb\r\nCcc";
    let mut doc = DocumentTab::new("s".into(), "lines.md".into(), crlf.into(), None, None);
    doc.place(5, false);
    doc.move_line_up();
    assert_eq!(doc.content, "Bbb\r\nAaa\r\nCcc");

    let mut doc = DocumentTab::new("s".into(), "lines.md".into(), crlf.into(), None, None);
    doc.place(5, false);
    doc.move_line_down();
    assert_eq!(doc.content, "Aaa\r\nCcc\r\nBbb");

    let bare = "Aaa\rBbb\rCcc";
    let mut doc = DocumentTab::new("s".into(), "lines.md".into(), bare.into(), None, None);
    doc.place(4, false);
    doc.move_line_up();
    assert_eq!(doc.content, "Bbb\rAaa\rCcc");

    let mut doc = DocumentTab::new("s".into(), "lines.md".into(), bare.into(), None, None);
    doc.place(4, false);
    doc.move_line_down();
    assert_eq!(doc.content, "Aaa\rCcc\rBbb");

    let lf = "Aaa\nBbb\nCcc";
    let mut doc = DocumentTab::new("s".into(), "lines.md".into(), lf.into(), None, None);
    doc.place(4, false);
    doc.move_line_up();
    assert_eq!(doc.content, "Bbb\nAaa\nCcc");
}

/// End, then Home, Backspace, and typing, the way CHECKER-4-R3 drove them.
fn crlf_caret(source: &str) -> (usize, usize, String, String, String, String, String) {
    let mut ended = DocumentTab::new("e".into(), "t.md".into(), source.into(), None, None);
    ended.move_end(false);
    let end_at = ended.cursor_offset();
    let before_disp = ended.parsed().lines[0].display_text.clone();
    ended.move_home_in_mode(false, ViewMode::Wysiwyg);
    let home = ended.cursor_offset();

    let mut typed = DocumentTab::new("z".into(), "t.md".into(), source.into(), None, None);
    typed.move_end(false);
    let at = typed.cursor_offset();
    typed.edit(at..at, "Z");
    let typed_content = typed.content.clone();
    let typed_disp = typed.parsed().lines[0].display_text.clone();

    let mut backspaced = DocumentTab::new("b".into(), "t.md".into(), source.into(), None, None);
    backspaced.move_end(false);
    backspaced.smart_backspace_in_mode(ViewMode::Wysiwyg);
    let bs_content = backspaced.content.clone();
    let bs_disp = backspaced.parsed().lines[0].display_text.clone();

    (
        end_at,
        home,
        before_disp,
        typed_content,
        typed_disp,
        bs_content,
        bs_disp,
    )
}

#[test]
fn crlf_home_after_end_on_1_dot_foo() {
    let (end_at, home, before_disp, _, _, _, _) = crlf_caret("1. foo\r\n");
    assert_eq!(end_at, 6, "end_at");
    assert_eq!(home, 3, "home after end");
    assert_eq!(before_disp, "foo", "before_disp");
}

#[test]
fn crlf_first_backspace_on_1_dot_foo() {
    let (_, _, before_disp, _, _, bs_content, bs_disp) = crlf_caret("1. foo\r\n");
    assert_eq!(before_disp, "foo", "before_disp");
    assert_eq!(bs_content, "1. fo\r\n");
    assert_eq!(bs_disp, "fo");
}

#[test]
fn crlf_type_z_at_end_on_1_dot_foo() {
    let (_, _, before_disp, typed_content, typed_disp, _, _) = crlf_caret("1. foo\r\n");
    assert_eq!(before_disp, "foo", "before_disp");
    assert_eq!(typed_content, "1. fooZ\r\n");
    assert_eq!(typed_disp, "fooZ");
    assert_eq!(typed_disp, format!("{before_disp}Z"));
}

#[test]
fn bare_cr_after_marker_1_dot_crfoo_is_not_a_list_item() {
    let source = "1.\rfoo";
    let (end_at, home, before_disp, typed_content, typed_disp, bs_content, bs_disp) =
        crlf_caret(source);
    let doc = parse_document(source);
    assert!(
        matches!(
            doc.lines[0].kind,
            LineKind::ListItem {
                ordered: true,
                number: 1,
                ..
            }
        ),
        "1. before a bare CR is an empty item, display {before_disp:?}"
    );
    assert!(
        matches!(doc.lines[1].kind, LineKind::Paragraph),
        "foo after the bare CR is its own line"
    );
    assert_eq!(doc.lines[1].display_text, "foo");
    assert_eq!(end_at, 2, "end_at");
    assert_eq!(home, 2, "home");
    assert_eq!(before_disp, "");
    assert_eq!(typed_content, "1.Z\rfoo");
    assert_eq!(typed_disp, "1.Z");
    assert_eq!(bs_content, "\rfoo");
    assert_eq!(bs_disp, "");
}

fn place_end(content: &str) -> DocumentTab {
    let mut doc = DocumentTab::new("p".into(), "t.md".into(), content.to_string(), None, None);
    doc.move_end(false);
    doc
}

#[test]
fn checklist_toggle_keeps_ordered_marker() {
    for (source, off, on) in [
        ("1. [x] a", "1. a", "- [ ] a"),
        ("1) [x] a", "1) a", "- [ ] a"),
        ("12. [X] a", "12. a", "- [ ] a"),
    ] {
        let mut doc =
            DocumentTab::new("c".into(), "t.md".into(), source.to_string(), None, None);
        assert!(doc.active_formats.checklist, "{source} checklist");
        assert!(!doc.active_formats.ol, "{source} ol");
        doc.apply_format_in_mode("checklist", ViewMode::Wysiwyg);
        assert_eq!(doc.content, off, "{source} off");
        assert!(!doc.active_formats.checklist, "{source} checklist off");
        assert!(doc.active_formats.ol, "{source} ol on");
        doc.apply_format_in_mode("checklist", ViewMode::Wysiwyg);
        assert_eq!(doc.content, on, "{source} on");
        assert!(doc.active_formats.checklist, "{source} checklist on");
        assert!(!doc.active_formats.ol, "{source} ol off");
    }
}

#[test]
fn edit_probes_for_interrupted_and_indented_markers() {
    // Empty `*` after a paragraph stays text. End lands on that line; Enter inserts a newline.
    let mut entered = DocumentTab::new("e".into(), "t.md".into(), "foo\n*".into(), None, None);
    entered.place(4, false);
    entered.move_end(false);
    assert_eq!(entered.cursor_offset(), 5);
    entered.smart_newline(false);
    assert_eq!(entered.content, "foo\n*\n");
    let mut typed = DocumentTab::new("t".into(), "t.md".into(), "foo\n*".into(), None, None);
    typed.place(5, false);
    typed.move_home_in_mode(false, ViewMode::Wysiwyg);
    assert_eq!(typed.cursor_offset(), 4);
    typed.edit(4..4, "Z");
    assert_eq!(typed.content, "foo\nZ*");
    let mut deleted = DocumentTab::new("d".into(), "t.md".into(), "foo\n*".into(), None, None);
    deleted.edit(4..5, "");
    assert_eq!(deleted.content, "foo\n");
    let mut backspaced =
        DocumentTab::new("b".into(), "t.md".into(), "foo\n*".into(), None, None);
    backspaced.place(5, false);
    backspaced.smart_backspace_in_mode(ViewMode::Wysiwyg);
    assert_eq!(backspaced.content, "foo\n");

    // `14)` after paragraph text is not a list marker.
    let paren = "The number of windows in my house is\n14)  The number of doors is 6.";
    let mut paren_enter =
        DocumentTab::new("e".into(), "t.md".into(), paren.to_string(), None, None);
    paren_enter.place(paren.find('\n').unwrap() + 1, false);
    paren_enter.move_end(false);
    let paren_end = paren_enter.cursor_offset();
    assert_eq!(paren_end, paren.len());
    paren_enter.smart_newline(false);
    assert_eq!(paren_enter.content, format!("{paren}\n"));
    let marker_at = paren.find("14)").unwrap();
    let mut paren_type =
        DocumentTab::new("t".into(), "t.md".into(), paren.to_string(), None, None);
    paren_type.edit(marker_at..marker_at, "Z");
    assert_eq!(
        paren_type.content,
        "The number of windows in my house is\nZ14)  The number of doors is 6."
    );
    let mut paren_bs =
        DocumentTab::new("b".into(), "t.md".into(), paren.to_string(), None, None);
    paren_bs.place(marker_at + "14)  ".len(), false);
    paren_bs.smart_backspace_in_mode(ViewMode::Wysiwyg);
    assert_eq!(
        paren_bs.content,
        "The number of windows in my house is\n14) The number of doors is 6."
    );

    // Column 4 marker is indented code, not a list. Enter keeps the
    // indent and does not continue a list. Home in WYSIWYG lands after
    // the hidden four-column indent, so typing stays in the code block.
    let indented = " \t63.\tnot";
    let mut ind_enter = place_end(indented);
    assert_eq!(ind_enter.cursor_offset(), indented.len());
    ind_enter.smart_newline(false);
    assert_eq!(ind_enter.content, " \t63.\tnot\n \t");
    let mut ind_type =
        DocumentTab::new("t".into(), "t.md".into(), indented.to_string(), None, None);
    ind_type.move_home_in_mode(false, ViewMode::Wysiwyg);
    assert_eq!(ind_type.cursor_offset(), 2);
    ind_type.edit(2..2, "Z");
    assert_eq!(ind_type.content, " \tZ63.\tnot");
    let mut ind_bs = place_end(indented);
    ind_bs.smart_backspace_in_mode(ViewMode::Wysiwyg);
    assert_eq!(ind_bs.content, " \t63.\tno");

    // A nested marker under a parent item still continues.
    let nested_src = "10) foo\n    - bar";
    let mut nested = DocumentTab::new(
        "n".into(),
        "t.md".into(),
        nested_src.to_string(),
        None,
        None,
    );
    nested.place(nested_src.find("    - bar").unwrap(), false);
    nested.move_end(false);
    assert_eq!(nested.cursor_offset(), nested_src.len());
    nested.smart_newline(false);
    assert_eq!(nested.content, "10) foo\n    - bar\n    - ");
}

#[test]
fn enter_keeps_cr_on_ordered_task_and_paren_marker() {
    let mut task =
        DocumentTab::new("t".into(), "t.md".into(), "1. [ ] \r\n".into(), None, None);
    task.place(crate::model::text_ops::line_end(&task.content, 0), false);
    task.smart_newline(false);
    assert_eq!(task.content, "1. [ ] \n2. [ ] \r\n");
    let mut paren = DocumentTab::new("p".into(), "t.md".into(), "1) \r\n".into(), None, None);
    paren.place(crate::model::text_ops::line_end(&paren.content, 0), false);
    paren.smart_newline(false);
    assert_eq!(paren.content, "1) \n2) \r\n");
    // Plain dot and bullet markers still exit, the same as the kit.
    let mut dot = DocumentTab::new("d".into(), "t.md".into(), "1. \r\n".into(), None, None);
    dot.place(crate::model::text_ops::line_end(&dot.content, 0), false);
    dot.smart_newline(false);
    assert_eq!(dot.content, "\n");
    let mut bullet =
        DocumentTab::new("b".into(), "t.md".into(), "- [ ] \r\n".into(), None, None);
    bullet.place(crate::model::text_ops::line_end(&bullet.content, 0), false);
    bullet.smart_newline(false);
    assert_eq!(bullet.content, "\n");
}

/// Ordered-list button is off on an ordered task, and the click leaves
/// the number and the box in place.
#[test]
fn ordered_button_off_does_not_strip_task() {
    for source in ["1. [x] a", "1) [x] a", "12. [X] a"] {
        let mut doc =
            DocumentTab::new("o".into(), "t.md".into(), source.to_string(), None, None);
        assert_eq!(
            format!(
                "ol={} checklist={} content={:?}",
                doc.active_formats.ol, doc.active_formats.checklist, doc.content
            ),
            format!("ol=false checklist=true content={source:?}"),
            "{source} before"
        );
        doc.apply_format_in_mode("ol", ViewMode::Wysiwyg);
        assert_eq!(
            format!(
                "ol={} checklist={} content={:?}",
                doc.active_formats.ol, doc.active_formats.checklist, doc.content
            ),
            format!("ol=false checklist=true content={source:?}"),
            "{source} after ol click"
        );
    }
}

/// Delete, Enter, Backspace, End, and type at the markers round 6 changed.
#[test]
fn edit_probes_for_nested_lazy_and_crlf_markers() {
    let nested = "- foo\n  *";
    let mut nested_end =
        DocumentTab::new("e".into(), "t.md".into(), nested.to_string(), None, None);
    nested_end.place(6, false);
    nested_end.move_end(false);
    assert_eq!(nested_end.cursor_offset(), nested.len());
    nested_end.smart_newline(false);
    assert_eq!(nested_end.content, "- foo\n  *\n  ");
    let mut nested_type =
        DocumentTab::new("t".into(), "t.md".into(), nested.to_string(), None, None);
    nested_type.place(6, false);
    nested_type.move_home_in_mode(false, ViewMode::Wysiwyg);
    assert_eq!(nested_type.cursor_offset(), 6);
    nested_type.edit(6..6, "Z");
    assert_eq!(nested_type.content, "- foo\nZ  *");
    let mut nested_del =
        DocumentTab::new("d".into(), "t.md".into(), nested.to_string(), None, None);
    nested_del.edit(8..9, "");
    assert_eq!(nested_del.content, "- foo\n  ");
    let mut nested_bs =
        DocumentTab::new("b".into(), "t.md".into(), nested.to_string(), None, None);
    nested_bs.place(nested.len(), false);
    nested_bs.smart_backspace_in_mode(ViewMode::Wysiwyg);
    assert_eq!(nested_bs.content, "- foo\n  ");

    let lazy = "- foo\nbar\n2. baz";
    let mut lazy_end =
        DocumentTab::new("e".into(), "t.md".into(), lazy.to_string(), None, None);
    lazy_end.place(lazy.rfind('\n').unwrap() + 1, false);
    lazy_end.move_end(false);
    assert_eq!(lazy_end.cursor_offset(), lazy.len());
    lazy_end.smart_newline(false);
    assert_eq!(lazy_end.content, "- foo\nbar\n2. baz\n3. ");
    let marker = lazy.rfind("2. ").unwrap();
    let mut lazy_del =
        DocumentTab::new("d".into(), "t.md".into(), lazy.to_string(), None, None);
    lazy_del.edit(marker..marker + 3, "");
    assert_eq!(lazy_del.content, "- foo\nbar\nbaz");
    let mut lazy_bs = DocumentTab::new("b".into(), "t.md".into(), lazy.to_string(), None, None);
    lazy_bs.place(lazy.len(), false);
    lazy_bs.smart_backspace_in_mode(ViewMode::Wysiwyg);
    assert_eq!(lazy_bs.content, "- foo\nbar\n2. ba");
    let mut lazy_type =
        DocumentTab::new("t".into(), "t.md".into(), lazy.to_string(), None, None);
    lazy_type.place(marker, false);
    lazy_type.move_home_in_mode(false, ViewMode::Wysiwyg);
    lazy_type.edit(lazy_type.cursor_offset()..lazy_type.cursor_offset(), "Z");
    assert_eq!(lazy_type.content, "- foo\nbar\n2. Zbaz");

    let crlf = "1. foo\r\n2.\r\n3. bar";
    let third = crlf.rfind('\n').unwrap() + 1;
    let mut crlf_end =
        DocumentTab::new("e".into(), "t.md".into(), crlf.to_string(), None, None);
    crlf_end.place(third, false);
    crlf_end.move_end(false);
    assert_eq!(crlf_end.cursor_offset(), crlf.len());
    crlf_end.smart_newline(false);
    assert_eq!(crlf_end.content, "1. foo\r\n2.\r\n3. bar\n4. ");
    let mut crlf_del =
        DocumentTab::new("d".into(), "t.md".into(), crlf.to_string(), None, None);
    crlf_del.edit(third..third + 3, "");
    assert_eq!(crlf_del.content, "1. foo\r\n2.\r\nbar");
    let mut crlf_type =
        DocumentTab::new("t".into(), "t.md".into(), crlf.to_string(), None, None);
    crlf_type.place(third, false);
    crlf_type.move_home_in_mode(false, ViewMode::Wysiwyg);
    let home = crlf_type.cursor_offset();
    crlf_type.edit(home..home, "Z");
    assert_eq!(crlf_type.content, "1. foo\r\n2.\r\n3. Zbar");
    let mut crlf_bs = DocumentTab::new("b".into(), "t.md".into(), crlf.to_string(), None, None);
    crlf_bs.place(crlf.len(), false);
    crlf_bs.smart_backspace_in_mode(ViewMode::Wysiwyg);
    assert_eq!(crlf_bs.content, "1. foo\r\n2.\r\n3. ba");
}

/// R5-1 / R6-1. Click, End, and Right share the visible end of a closing-hash
/// heading. Delete there joins the next line the way a heading without a
/// closer does (`# foo\nnext` becomes `# foonext`). Enter there breaks
/// after the heading.
#[test]
fn closing_hash_click_end_delete_and_enter_keep_the_heading() {
    let src = "# foo #\nnext";
    let mut doc = DocumentTab::new("t".into(), "t.md".into(), src.into(), None, None);
    let click = {
        let parsed = doc.parsed();
        let line = &parsed.lines[0];
        assert_eq!(line.display_text, "foo");
        line.offset_map.display_to_source(line.display_text.len())
    };

    doc.place(0, false);
    doc.move_end(false);
    assert_eq!(doc.cursor_offset(), click, "End lands on the click");
    doc.place(4, false);
    doc.move_right_in_mode(false, ViewMode::Wysiwyg);
    assert_eq!(doc.cursor_offset(), click, "Right lands on the click");
    doc.place(7, false);
    assert_eq!(
        doc.cursor_offset(),
        click,
        "place at the source end snaps to the click"
    );

    doc.place(click, false);
    doc.delete_forward_in_mode(ViewMode::Wysiwyg);
    // BASE Delete at the source end (offset 7, closer still visible) is
    // these bytes. Display `foo #next`: the hash is no longer a closer.
    assert_eq!(doc.content, "# foo #next");
    let parsed = doc.parsed();
    assert_eq!(parsed.lines.len(), 1);
    assert_eq!(parsed.lines[0].display_text, "foo #next");
    assert!(matches!(
        parsed.lines[0].kind,
        LineKind::Heading { level: 1 }
    ));

    let mut doc = DocumentTab::new("t".into(), "t.md".into(), src.into(), None, None);
    doc.place(click, false);
    doc.smart_newline(false);
    assert_eq!(doc.content, "# foo #\n\nnext");
    let parsed = doc.parsed();
    assert_eq!(parsed.lines[0].display_text, "foo");
    assert_eq!(parsed.lines[1].display_text, "");
    assert_eq!(parsed.lines[2].display_text, "next");
    assert!(matches!(
        parsed.lines[0].kind,
        LineKind::Heading { level: 1 }
    ));
    assert!(matches!(parsed.lines[1].kind, LineKind::Empty));
    assert!(matches!(parsed.lines[2].kind, LineKind::Paragraph));

    let src = "## foo ##\nbar";
    let mut doc = DocumentTab::new("t".into(), "t.md".into(), src.into(), None, None);
    let click = {
        let parsed = doc.parsed();
        let line = &parsed.lines[0];
        assert_eq!(line.display_text, "foo");
        line.offset_map.display_to_source(line.display_text.len())
    };
    doc.place(0, false);
    doc.move_end(false);
    assert_eq!(doc.cursor_offset(), click);
    doc.place(5, false);
    doc.move_right_in_mode(false, ViewMode::Wysiwyg);
    assert_eq!(doc.cursor_offset(), click);
    doc.place(click, false);
    doc.delete_forward_in_mode(ViewMode::Wysiwyg);
    assert_eq!(doc.content, "## foo ##bar");
    let parsed = doc.parsed();
    assert_eq!(parsed.lines.len(), 1);
    assert_eq!(parsed.lines[0].display_text, "foo ##bar");
    assert!(matches!(
        parsed.lines[0].kind,
        LineKind::Heading { level: 2 }
    ));

    let mut doc = DocumentTab::new("t".into(), "t.md".into(), src.into(), None, None);
    doc.place(click, false);
    doc.smart_newline(false);
    assert_eq!(doc.content, "## foo ##\n\nbar");
    let parsed = doc.parsed();
    assert_eq!(parsed.lines[0].display_text, "foo");
    assert_eq!(parsed.lines[1].display_text, "");
    assert_eq!(parsed.lines[2].display_text, "bar");
    assert!(matches!(
        parsed.lines[0].kind,
        LineKind::Heading { level: 2 }
    ));
    assert!(matches!(parsed.lines[1].kind, LineKind::Empty));
    assert!(matches!(parsed.lines[2].kind, LineKind::Paragraph));
}

/// Enter after `---` inserts a plain break. The other rows are not rules
/// on BASE: a spaced dash or star line is a list, and the indented star
/// line is a paragraph whose indent Enter copies.
#[test]
fn enter_on_spaced_rules_inserts_a_plain_break() {
    let src = "---\nnext";
    let end = {
        let parsed = parse_document(src);
        assert!(matches!(parsed.lines[0].kind, LineKind::HorizontalRule));
        assert_eq!(parsed.lines[0].display_text, "");
        parsed.lines[0].source.end
    };
    let mut doc = DocumentTab::new("t".into(), "t.md".into(), src.into(), None, None);
    doc.place(end, false);
    doc.smart_newline(false);
    assert_eq!(doc.content, "---\n\nnext");
    let parsed = doc.parsed();
    assert!(matches!(parsed.lines[0].kind, LineKind::HorizontalRule));
    assert_eq!(parsed.lines[0].display_text, "");
    assert!(matches!(parsed.lines[1].kind, LineKind::Empty));
    assert_eq!(parsed.lines[1].display_text, "");
    assert_eq!(parsed.lines[2].display_text, "next");

    let src = "- - -\nnext";
    let end = parse_document(src).lines[0].source.end;
    let mut doc = DocumentTab::new("t".into(), "t.md".into(), src.into(), None, None);
    doc.place(end, false);
    doc.smart_newline(false);
    assert_eq!(doc.content, "- - -\n- \nnext");
    let parsed = doc.parsed();
    assert!(matches!(
        parsed.lines[0].kind,
        LineKind::ListItem {
            ordered: false,
            indent_cols: 0,
            ..
        }
    ));
    assert_eq!(parsed.lines[0].display_text, "- -");
    assert!(matches!(
        parsed.lines[1].kind,
        LineKind::ListItem { ordered: false, .. }
    ));
    assert_eq!(parsed.lines[1].display_text, "");
    assert_eq!(parsed.lines[2].display_text, "next");

    let src = "* * *\nnext";
    let end = parse_document(src).lines[0].source.end;
    let mut doc = DocumentTab::new("t".into(), "t.md".into(), src.into(), None, None);
    doc.place(end, false);
    doc.smart_newline(false);
    assert_eq!(doc.content, "* * *\n* \nnext");
    let parsed = doc.parsed();
    assert_eq!(parsed.lines[0].display_text, "* *");
    assert!(matches!(
        parsed.lines[0].kind,
        LineKind::ListItem {
            ordered: false,
            indent_cols: 0,
            ..
        }
    ));
    assert_eq!(parsed.lines[1].display_text, "");
    assert_eq!(parsed.lines[2].display_text, "next");

    let src = "-     -      -      -\nnext";
    let end = parse_document(src).lines[0].source.end;
    let mut doc = DocumentTab::new("t".into(), "t.md".into(), src.into(), None, None);
    doc.place(end, false);
    doc.smart_newline(false);
    assert_eq!(doc.content, "-     -      -      -\n- \nnext");
    let parsed = doc.parsed();
    assert_eq!(parsed.lines[0].display_text, "    -      -      -");
    assert!(matches!(
        parsed.lines[0].kind,
        LineKind::ListItem {
            ordered: false,
            indent_cols: 0,
            ..
        }
    ));
    assert_eq!(parsed.lines[1].display_text, "");
    assert_eq!(parsed.lines[2].display_text, "next");

    let src = " **  * ** * ** * **\nnext";
    let end = parse_document(src).lines[0].source.end;
    let mut doc = DocumentTab::new("t".into(), "t.md".into(), src.into(), None, None);
    doc.place(end, false);
    doc.smart_newline(false);
    assert_eq!(doc.content, " **  * ** * ** * **\n \nnext");
    let parsed = doc.parsed();
    assert!(matches!(parsed.lines[0].kind, LineKind::Paragraph));
    assert_eq!(parsed.lines[0].display_text, " **  * ** * ** * **");
    assert!(matches!(parsed.lines[1].kind, LineKind::Empty));
    assert_eq!(parsed.lines[1].display_text, " ");
    assert_eq!(parsed.lines[2].display_text, "next");
}

/// R5-3. Enter at offset 4 of `Foo\rBar` inserts a blank line between them.
#[test]
fn enter_at_start_of_bare_cr_line_inserts_a_line() {
    let mut doc = DocumentTab::new("t".into(), "t.md".into(), "Foo\rBar".into(), None, None);
    doc.place(4, false);
    assert_eq!(doc.cursor_offset(), 4);
    doc.smart_newline(false);
    assert_eq!(doc.content, "Foo\r\n\nBar");
    let parsed = doc.parsed();
    assert_eq!(parsed.lines.len(), 3);
    assert_eq!(parsed.lines[0].display_text, "Foo");
    assert_eq!(parsed.lines[1].display_text, "");
    assert_eq!(parsed.lines[2].display_text, "Bar");
    assert!(matches!(parsed.lines[1].kind, LineKind::Empty));
}

/// R6-2. Up from the line under a closing-hash heading lands on the
/// visible end, not on the hidden `#`.
#[test]
fn up_onto_closing_hash_lands_on_the_visible_end() {
    let src = "# foo #\nnext!!!!";
    let mut doc = DocumentTab::new("t".into(), "t.md".into(), src.into(), None, None);
    doc.place(src.len(), false);
    assert_eq!(doc.cursor_offset(), src.len());
    doc.move_up(false);
    assert_eq!(doc.cursor_offset(), 5);
    let at = doc.cursor_offset();
    doc.insert_text_in_mode(at..at, "X", ViewMode::Wysiwyg);
    assert_eq!(doc.content, "# fooX #\nnext!!!!");
    let parsed = doc.parsed();
    assert_eq!(parsed.lines[0].display_text, "fooX");
    assert!(matches!(
        parsed.lines[0].kind,
        LineKind::Heading { level: 1 }
    ));
}

/// R6-3. Shift+Enter uses the same heading and bare-CR breaks as Enter.
#[test]
fn shift_enter_keeps_closing_hash_and_bare_cr_line() {
    let src = "# foo #\nnext";
    let mut doc = DocumentTab::new("t".into(), "t.md".into(), src.into(), None, None);
    let click = {
        let line = &doc.parsed().lines[0];
        line.offset_map.display_to_source(line.display_text.len())
    };
    doc.place(click, false);
    doc.smart_newline(true);
    assert_eq!(doc.content, "# foo #\n\nnext");
    let parsed = doc.parsed();
    assert_eq!(parsed.lines[0].display_text, "foo");
    assert_eq!(parsed.lines[1].display_text, "");
    assert_eq!(parsed.lines[2].display_text, "next");
    assert!(matches!(
        parsed.lines[0].kind,
        LineKind::Heading { level: 1 }
    ));
    assert!(matches!(parsed.lines[1].kind, LineKind::Empty));

    let mut bare = DocumentTab::new("t".into(), "t.md".into(), "Foo\rBar".into(), None, None);
    bare.place(4, false);
    bare.smart_newline(true);
    assert_eq!(bare.content, "Foo\r\n\nBar");
    let parsed = bare.parsed();
    assert_eq!(parsed.lines.len(), 3);
    assert_eq!(parsed.lines[0].display_text, "Foo");
    assert_eq!(parsed.lines[1].display_text, "");
    assert_eq!(parsed.lines[2].display_text, "Bar");
}

/// R6-4. Toolbar toggles follow the visible heading text. A leftover
/// closer stays only when the line is still a heading, so pulldown does
/// not show the hash (`# foo ##`).
#[test]
fn toolbar_toggle_uses_visible_heading_text() {
    let src = "# foo #\nnext";
    let click_of = |text: &str| {
        let doc = parse_document(text);
        let line = &doc.lines[0];
        line.offset_map.display_to_source(line.display_text.len())
    };
    let mut doc = DocumentTab::new("t".into(), "t.md".into(), src.into(), None, None);
    doc.place(click_of(src), false);
    doc.apply_format_in_mode("h1", ViewMode::Wysiwyg);
    assert_eq!(doc.content, "foo\nnext");
    let parsed = doc.parsed();
    assert_eq!(parsed.lines[0].display_text, "foo");
    assert!(matches!(parsed.lines[0].kind, LineKind::Paragraph));
    assert_eq!(parsed.lines[1].display_text, "next");

    let mut doc = DocumentTab::new("t".into(), "t.md".into(), src.into(), None, None);
    doc.place(click_of(src), false);
    doc.apply_format_in_mode("ul", ViewMode::Wysiwyg);
    assert_eq!(doc.content, "- foo\nnext");
    assert_eq!(doc.parsed().lines[0].display_text, "foo");

    let mut doc = DocumentTab::new("t".into(), "t.md".into(), src.into(), None, None);
    doc.place(click_of(src), false);
    doc.apply_format_in_mode("ol", ViewMode::Wysiwyg);
    assert_eq!(doc.content, "1. foo\nnext");
    assert_eq!(doc.parsed().lines[0].display_text, "foo");

    let mut doc = DocumentTab::new("t".into(), "t.md".into(), src.into(), None, None);
    doc.place(click_of(src), false);
    doc.apply_format_in_mode("quote", ViewMode::Wysiwyg);
    assert_eq!(doc.content, "> foo\nnext");
    assert_eq!(doc.parsed().lines[0].display_text, "foo");

    let src = "## foo ##\nbar";
    let mut doc = DocumentTab::new("t".into(), "t.md".into(), src.into(), None, None);
    doc.place(click_of(src), false);
    doc.apply_format_in_mode("h1", ViewMode::Wysiwyg);
    assert_eq!(doc.content, "# foo ##\nbar");
    let parsed = doc.parsed();
    assert_eq!(parsed.lines[0].display_text, "foo");
    assert!(matches!(
        parsed.lines[0].kind,
        LineKind::Heading { level: 1 }
    ));
}

/// R6-5. Delete-to-end and delete-word-right at the visible end keep the
/// hidden closer. A heading without a closer is unchanged by delete-to-end
/// and loses the next word to delete-word-right.
#[test]
fn delete_to_end_and_word_right_keep_the_closing_hash() {
    let src = "# foo #\nnext";
    let click = {
        let doc = parse_document(src);
        let line = &doc.lines[0];
        line.offset_map.display_to_source(line.display_text.len())
    };
    assert_eq!(click, 5);

    let mut doc = DocumentTab::new("t".into(), "t.md".into(), src.into(), None, None);
    doc.place(click, false);
    doc.delete_to_line_end();
    assert_eq!(doc.content, "# foo #\nnext");
    let parsed = doc.parsed();
    assert_eq!(parsed.lines[0].display_text, "foo");
    assert_eq!(parsed.lines[1].display_text, "next");

    let mut doc = DocumentTab::new("t".into(), "t.md".into(), src.into(), None, None);
    doc.place(click, false);
    doc.delete_word_right();
    assert_eq!(doc.content, "# foo #\nnext");
    let parsed = doc.parsed();
    assert_eq!(parsed.lines[0].display_text, "foo");
    assert_eq!(parsed.lines[1].display_text, "next");

    let plain = "# foo\nnext";
    let mut doc = DocumentTab::new("t".into(), "t.md".into(), plain.into(), None, None);
    doc.place(5, false);
    doc.delete_to_line_end();
    assert_eq!(doc.content, "# foo\nnext");
    assert_eq!(doc.parsed().lines[0].display_text, "foo");

    let mut doc = DocumentTab::new("t".into(), "t.md".into(), plain.into(), None, None);
    doc.place(5, false);
    doc.delete_word_right();
    assert_eq!(doc.content, "# foo");
    assert_eq!(doc.parsed().lines[0].display_text, "foo");
}

/// K2. Select-line then Delete, and delete-to-line-start, at the visible
/// end of `# foo #\nnext`. BASE select-line uses `line_extent` (the whole
/// line, including the ending), so Delete leaves `next`. BASE
/// delete-to-line-start at the source end (offset 7, the closer is
/// visible there) leaves `\nnext`. The no-closer line matches both.
#[test]
fn select_line_delete_and_delete_to_line_start_match_base() {
    let src = "# foo #\nnext";
    let mut doc = DocumentTab::new("t".into(), "t.md".into(), src.into(), None, None);
    doc.place(5, false);
    assert_eq!(doc.cursor_offset(), 5);
    doc.select_line();
    doc.delete_forward_in_mode(ViewMode::Wysiwyg);
    assert_eq!(doc.content, "next");
    let parsed = doc.parsed();
    assert_eq!(parsed.lines.len(), 1);
    assert_eq!(parsed.lines[0].display_text, "next");
    assert!(matches!(parsed.lines[0].kind, LineKind::Paragraph));

    let mut doc = DocumentTab::new("t".into(), "t.md".into(), src.into(), None, None);
    doc.place(5, false);
    doc.delete_to_line_start();
    assert_eq!(doc.content, "\nnext");
    let parsed = doc.parsed();
    assert_eq!(parsed.lines[0].display_text, "");
    assert!(matches!(parsed.lines[0].kind, LineKind::Empty));
    assert_eq!(parsed.lines[1].display_text, "next");
    assert!(matches!(parsed.lines[1].kind, LineKind::Paragraph));

    let plain = "# foo\nnext";
    let mut doc = DocumentTab::new("t".into(), "t.md".into(), plain.into(), None, None);
    doc.place(5, false);
    doc.select_line();
    doc.delete_forward_in_mode(ViewMode::Wysiwyg);
    assert_eq!(doc.content, "next");

    let mut doc = DocumentTab::new("t".into(), "t.md".into(), plain.into(), None, None);
    doc.place(5, false);
    doc.delete_to_line_start();
    assert_eq!(doc.content, "\nnext");
}

/// K4. Raw paints the source, so End and place stay on the raw line end.
/// BASE `place` does not snap and BASE End on `# foo #\nnext` is offset 7.
/// Rendered mode still snaps that offset back to the visible end.
#[test]
fn raw_end_and_place_use_the_source_line_end() {
    let src = "# foo #\nnext";
    let mut rendered = DocumentTab::new("t".into(), "t.md".into(), src.into(), None, None);
    rendered.view_mode = ViewMode::Wysiwyg;
    rendered.place(0, false);
    rendered.move_end(false);
    assert_eq!(rendered.cursor_offset(), 5);
    rendered.place(7, false);
    assert_eq!(rendered.cursor_offset(), 5);

    let mut raw = DocumentTab::new("t".into(), "t.md".into(), src.into(), None, None);
    raw.view_mode = ViewMode::Raw;
    raw.place(0, false);
    raw.move_end(false);
    assert_eq!(raw.cursor_offset(), 7);
    raw.place(7, false);
    assert_eq!(raw.cursor_offset(), 7);

    let plain = "# foo\nnext";
    let mut raw_plain = DocumentTab::new("t".into(), "t.md".into(), plain.into(), None, None);
    raw_plain.view_mode = ViewMode::Raw;
    raw_plain.place(0, false);
    raw_plain.move_end(false);
    assert_eq!(raw_plain.cursor_offset(), 5);
    raw_plain.place(5, false);
    assert_eq!(raw_plain.cursor_offset(), 5);
}

/// K3. Down on the only painted row of `# foo #` must land on the visible
/// end, the same snap `move_down` applies. Typing there keeps the closer.
/// BASE types at the source end and produces `# foo #X`, because BASE does
/// not hide the closer. The rendered snap inserts at offset 5.
#[test]
fn down_on_the_last_painted_row_snaps_before_the_closing_hash() {
    let src = "# foo #";
    let mut doc = DocumentTab::new("t".into(), "t.md".into(), src.into(), None, None);
    let (byte_start, byte_end, offset_map, vend) = {
        let parsed = doc.parsed();
        let line = &parsed.lines[0];
        assert_eq!(line.display_text, "foo");
        let vend = line.offset_map.display_to_source(line.display_text.len());
        (
            line.source.start,
            line.source.end,
            line.offset_map.clone(),
            vend,
        )
    };
    assert_eq!(vend, 5);
    assert_eq!(doc.content.len(), 7);
    let paint = EditorPaint {
        wrap_cols: usize::MAX,
        line_height: px(28.0),
        total_height: px(28.0),
        rows: vec![PaintRow {
            line_index: 0,
            byte_start,
            byte_end,
            origin: point(px(0.0), px(0.0)),
            line_height: px(28.0),
            shaped: std::rc::Rc::new(ShapedLine::default()),
            offset_map,
            checkbox_hit: None,
            link_hits: Arc::default(),
            cells: Vec::new(),
        }],
        bounds: None,
        ..EditorPaint::default()
    };
    doc.caret.move_to(0);
    let hit = move_vertical_visual(
        &paint,
        &doc.content,
        &mut doc.caret,
        1,
        false,
        ViewMode::Wysiwyg,
    );
    assert!(hit);
    assert_eq!(doc.cursor_offset(), vend);
    let at = doc.cursor_offset();
    doc.insert_text_in_mode(at..at, "X", ViewMode::Wysiwyg);
    assert_eq!(doc.content, "# fooX #");
    let parsed = doc.parsed();
    assert_eq!(parsed.lines[0].display_text, "fooX");
    assert!(matches!(
        parsed.lines[0].kind,
        LineKind::Heading { level: 1 }
    ));

    let mut raw = DocumentTab::new("t".into(), "t.md".into(), src.into(), None, None);
    raw.caret.move_to(0);
    let raw_hit = move_vertical_visual(
        &paint,
        &raw.content,
        &mut raw.caret,
        1,
        false,
        ViewMode::Raw,
    );
    assert!(raw_hit);
    assert_eq!(raw.cursor_offset(), src.len());
    let at = raw.cursor_offset();
    raw.insert_text_in_mode(at..at, "X", ViewMode::Raw);
    assert_eq!(raw.content, "# foo #X");

    let mut shift = DocumentTab::new("t".into(), "t.md".into(), src.into(), None, None);
    shift.caret.move_to(0);
    let shift_hit = move_vertical_visual(
        &paint,
        &shift.content,
        &mut shift.caret,
        1,
        true,
        ViewMode::Wysiwyg,
    );
    assert!(shift_hit);
    // BASE `1d03313` Shift+Down on this last row selects `0..7`, and
    // Delete clears the line. The collapsed Down above still types at
    // the visible end.
    assert_eq!(shift.caret.range, 0..7);
    shift.delete_forward_in_mode(ViewMode::Wysiwyg);
    assert_eq!(shift.content, "");
}

fn heading_tab(text: &str) -> DocumentTab {
    DocumentTab::new("t".into(), "t.md".into(), text.into(), None, None)
}

/// `active_tab_mut` and `editor_surface` call `set_view_mode` with the
/// mode the tab already has, before a paint or an edit.
fn app_touch(doc: &mut DocumentTab) {
    let mode = doc.view_mode;
    doc.set_view_mode(mode);
}

/// K1. Ancestor 7ad1cff rendered End on `# foo #` is offset 5, and typing
/// X there yields `# fooX #` (display `fooX`).
#[test]
fn raw_end_then_rendered_type_keeps_the_closer() {
    let mut doc = heading_tab("# foo #");
    doc.set_view_mode(ViewMode::Raw);
    doc.place(0, false);
    doc.move_end(false);
    assert_eq!(doc.cursor_offset(), 7);
    doc.set_view_mode(ViewMode::Wysiwyg);
    let at = doc.cursor_offset();
    doc.insert_text_in_mode(at..at, "X", ViewMode::Wysiwyg);
    assert_eq!(doc.content, "# fooX #");
    assert_eq!(doc.parsed().lines[0].display_text, "fooX");
}

/// Ancestor rendered End then Backspace yields `# fo #` (display `fo`).
#[test]
fn raw_end_then_rendered_backspace_deletes_visible_text() {
    let mut doc = heading_tab("# foo #");
    doc.set_view_mode(ViewMode::Raw);
    doc.place(0, false);
    doc.move_end(false);
    doc.set_view_mode(ViewMode::Wysiwyg);
    doc.smart_backspace_in_mode(ViewMode::Wysiwyg);
    assert_eq!(doc.content, "# fo #");
    assert_eq!(doc.parsed().lines[0].display_text, "fo");
}

/// Ancestor rendered End then Ctrl-Delete on `# foo #\nnext` leaves both lines.
#[test]
fn raw_end_then_rendered_ctrl_delete_keeps_the_next_line() {
    let mut doc = heading_tab("# foo #\nnext");
    doc.set_view_mode(ViewMode::Raw);
    doc.place(0, false);
    doc.move_end(false);
    assert_eq!(doc.cursor_offset(), 7);
    doc.set_view_mode(ViewMode::Wysiwyg);
    doc.delete_word_right();
    assert_eq!(doc.content, "# foo #\nnext");
    assert_eq!(doc.parsed().lines[0].display_text, "foo");
    assert_eq!(doc.parsed().lines[1].display_text, "next");
}

/// Raw End, type X (`# foo #X`), switch to rendered, undo, redo, then type Y.
/// Ancestor rendered End then Y is `# fooY #`.
#[test]
fn undo_redo_after_raw_edit_then_rendered_types_at_the_visible_end() {
    let mut doc = heading_tab("# foo #");
    doc.set_view_mode(ViewMode::Raw);
    doc.place(0, false);
    doc.move_end(false);
    let at = doc.cursor_offset();
    doc.insert_text_in_mode(at..at, "X", ViewMode::Raw);
    assert_eq!(doc.content, "# foo #X");
    doc.set_view_mode(ViewMode::Wysiwyg);
    assert!(doc.undo());
    assert_eq!(doc.content, "# foo #");
    assert_eq!(doc.cursor_offset(), 5);
    assert!(doc.redo());
    assert_eq!(doc.content, "# foo #X");
    assert_eq!(doc.cursor_offset(), "# foo #X".len());
    assert!(doc.undo());
    assert_eq!(doc.cursor_offset(), 5);
    let at = doc.cursor_offset();
    doc.insert_text_in_mode(at..at, "Y", ViewMode::Wysiwyg);
    assert_eq!(doc.content, "# fooY #");
    assert_eq!(doc.parsed().lines[0].display_text, "fooY");
}

/// A caret that is valid in rendered mode stays put when the view becomes Raw.
#[test]
fn rendered_to_raw_keeps_a_valid_caret() {
    let mut doc = heading_tab("# foo #\nnext");
    doc.set_view_mode(ViewMode::Wysiwyg);
    doc.place(0, false);
    doc.move_end(false);
    assert_eq!(doc.cursor_offset(), 5);
    doc.set_view_mode(ViewMode::Raw);
    assert_eq!(doc.cursor_offset(), 5);
    assert_eq!(doc.content, "# foo #\nnext");
    doc.place(2, false);
    doc.set_view_mode(ViewMode::Wysiwyg);
    assert_eq!(doc.cursor_offset(), 2);
    doc.place(0, false);
    doc.set_view_mode(ViewMode::Raw);
    assert_eq!(doc.cursor_offset(), 0);
}

/// BASE `1d03313` deletes these raw ranges. The switch to rendered must
/// leave the range in place. `# foo #\nnext` `0..7` deletes to `\nnext`.
/// `# foo #` `2..7` deletes to `# `. `7..12` deletes to `# foo #`.
/// `6..7` deletes to `# foo \nnext`.
#[test]
fn raw_selection_then_rendered_snaps_anchor_and_caret() {
    let mut doc = heading_tab("# foo #\nnext");
    doc.set_view_mode(ViewMode::Raw);
    doc.place(0, false);
    doc.move_end(true);
    assert_eq!(doc.caret.range, 0..7);
    doc.set_view_mode(ViewMode::Wysiwyg);
    assert_eq!(doc.caret.range, 0..7);
    doc.delete_forward_in_mode(ViewMode::Wysiwyg);
    assert_eq!(doc.content, "\nnext");

    let mut doc = heading_tab("# foo #");
    doc.set_view_mode(ViewMode::Raw);
    doc.caret.range = 2..7;
    doc.caret.reversed = true;
    doc.set_view_mode(ViewMode::Wysiwyg);
    assert_eq!(doc.caret.range, 2..7);
    assert!(doc.caret.reversed);
    doc.delete_forward_in_mode(ViewMode::Wysiwyg);
    assert_eq!(doc.content, "# ");

    let mut doc = heading_tab("# foo #\nnext");
    doc.set_view_mode(ViewMode::Raw);
    doc.caret.range = 7..12;
    doc.set_view_mode(ViewMode::Wysiwyg);
    assert_eq!(doc.caret.range, 7..12);
    doc.delete_forward_in_mode(ViewMode::Wysiwyg);
    assert_eq!(doc.content, "# foo #");

    let mut doc = heading_tab("# foo #\nnext");
    doc.set_view_mode(ViewMode::Raw);
    doc.caret.range = 6..7;
    doc.set_view_mode(ViewMode::Wysiwyg);
    assert_eq!(doc.caret.range, 6..7);
    doc.delete_forward_in_mode(ViewMode::Wysiwyg);
    assert_eq!(doc.content, "# foo \nnext");
}

/// BASE `1d03313` select-all, then Delete, Backspace, type X, or cut, on a
/// document that ends on a closing-hash heading. Delete clears the file.
#[test]
fn rendered_select_all_on_a_final_heading_matches_base() {
    for src in ["# foo #", "hello\n# Title #", "## foo ##", "### ###"] {
        let mut doc = heading_tab(src);
        doc.select_all();
        app_touch(&mut doc);
        doc.delete_forward_in_mode(ViewMode::Wysiwyg);
        assert_eq!(doc.content, "", "{src:?} delete");

        let mut doc = heading_tab(src);
        doc.select_all();
        app_touch(&mut doc);
        doc.smart_backspace_in_mode(ViewMode::Wysiwyg);
        assert_eq!(doc.content, "", "{src:?} backspace");

        let mut doc = heading_tab(src);
        doc.select_all();
        app_touch(&mut doc);
        let range = doc.selection_range();
        doc.insert_text_in_mode(range, "X", ViewMode::Wysiwyg);
        assert_eq!(doc.content, "X", "{src:?} type");

        let mut doc = heading_tab(src);
        doc.select_all();
        app_touch(&mut doc);
        let clip = doc.selected_text();
        let range = doc.selection_range();
        doc.edit(range, "");
        assert_eq!(clip, src, "{src:?} clip");
        assert_eq!(doc.content, "", "{src:?} cut");
    }
}

/// BASE `1d03313` select-line on the closing-hash heading, then the same
/// four edits. A following `hello\n` line stays. A one-line heading clears.
#[test]
fn rendered_select_line_on_a_final_heading_matches_base() {
    for src in ["# foo #", "## foo ##", "### ###"] {
        let mut doc = heading_tab(src);
        doc.place(0, false);
        doc.select_line();
        app_touch(&mut doc);
        doc.delete_forward_in_mode(ViewMode::Wysiwyg);
        assert_eq!(doc.content, "", "{src:?} delete");

        let mut doc = heading_tab(src);
        doc.place(0, false);
        doc.select_line();
        app_touch(&mut doc);
        doc.smart_backspace_in_mode(ViewMode::Wysiwyg);
        assert_eq!(doc.content, "", "{src:?} backspace");

        let mut doc = heading_tab(src);
        doc.place(0, false);
        doc.select_line();
        app_touch(&mut doc);
        let range = doc.selection_range();
        doc.insert_text_in_mode(range, "X", ViewMode::Wysiwyg);
        assert_eq!(doc.content, "X", "{src:?} type");

        let mut doc = heading_tab(src);
        doc.place(0, false);
        doc.select_line();
        app_touch(&mut doc);
        let clip = doc.selected_text();
        let range = doc.selection_range();
        doc.edit(range, "");
        assert_eq!(clip, src, "{src:?} clip");
        assert_eq!(doc.content, "", "{src:?} cut");
    }

    let src = "hello\n# Title #";
    let at = src.rfind('\n').map(|index| index + 1).unwrap_or(0);
    let mut doc = heading_tab(src);
    doc.place(at, false);
    doc.select_line();
    app_touch(&mut doc);
    doc.delete_forward_in_mode(ViewMode::Wysiwyg);
    assert_eq!(doc.content, "hello\n");

    let mut doc = heading_tab(src);
    doc.place(at, false);
    doc.select_line();
    app_touch(&mut doc);
    doc.smart_backspace_in_mode(ViewMode::Wysiwyg);
    assert_eq!(doc.content, "hello\n");

    let mut doc = heading_tab(src);
    doc.place(at, false);
    doc.select_line();
    app_touch(&mut doc);
    let range = doc.selection_range();
    doc.insert_text_in_mode(range, "X", ViewMode::Wysiwyg);
    assert_eq!(doc.content, "hello\nX");

    let mut doc = heading_tab(src);
    doc.place(at, false);
    doc.select_line();
    app_touch(&mut doc);
    let clip = doc.selected_text();
    let range = doc.selection_range();
    doc.edit(range, "");
    assert_eq!(clip, "# Title #");
    assert_eq!(doc.content, "hello\n");
}

fn delete_forward(doc: &mut DocumentTab) {
    doc.delete_forward_in_mode(ViewMode::Wysiwyg);
}

/// BASE `1d03313` `move_end(true)` then Delete. The selection runs to the
/// source end, and on CRLF through the CR up to the LF.
#[test]
fn shift_end_extend_deletes_through_the_closing_hash() {
    let cases = [
        ("# foo #", ""),
        ("# foo #\nnext", "\nnext"),
        ("## foo ##", ""),
        ("## bar ##", ""),
        ("### ###", ""),
        ("# foo #   ", ""),
        ("# foo #\r\nnext", "\nnext"),
        ("# café #", ""),
        ("# 😀 #", ""),
    ];
    for (src, after) in cases {
        let mut doc = heading_tab(src);
        doc.move_end(true);
        delete_forward(&mut doc);
        assert_eq!(doc.content, after, "end {src:?}");
    }

    let homes = [
        ("# foo #", "# "),
        ("# foo #\nnext", "# \nnext"),
        ("## foo ##", "## "),
        ("### ###", "### "),
        ("# foo #   ", "# "),
        ("# café #", "# "),
        ("# 😀 #", "# "),
    ];
    for (src, after) in homes {
        let mut doc = heading_tab(src);
        doc.move_home_in_mode(false, ViewMode::Wysiwyg);
        doc.move_end(true);
        delete_forward(&mut doc);
        assert_eq!(doc.content, after, "home-end {src:?}");
    }

    let src = "hello\n# foo #";
    let mut doc = heading_tab(src);
    doc.place(6, false);
    doc.move_end(true);
    delete_forward(&mut doc);
    assert_eq!(doc.content, "hello\n");
}

/// BASE `1d03313` `place` does not pull an extend target back to the
/// visible end. Mouse drag and Ctrl+Shift+End both `place` the offset.
#[test]
fn place_extend_onto_the_closing_hash_deletes_it() {
    let mut doc = heading_tab("# foo #");
    doc.place(0, false);
    doc.place(7, true);
    assert_eq!(doc.caret.range, 0..7);
    delete_forward(&mut doc);
    assert_eq!(doc.content, "");

    let mut doc = heading_tab("# foo #\nnext");
    doc.place(0, false);
    doc.place(7, true);
    delete_forward(&mut doc);
    assert_eq!(doc.content, "\nnext");

    let mut doc = heading_tab("# foo #");
    doc.place(0, false);
    doc.place(6, true);
    delete_forward(&mut doc);
    assert_eq!(doc.content, "#");

    let mut doc = heading_tab("# foo #");
    doc.place(5, false);
    doc.place(7, true);
    delete_forward(&mut doc);
    assert_eq!(doc.content, "# foo");

    let src = "hello\n# Title #";
    let mut doc = heading_tab(src);
    doc.place(0, false);
    doc.place(src.len(), true);
    delete_forward(&mut doc);
    assert_eq!(doc.content, "");

    let mut doc = heading_tab("# foo #");
    doc.place(5, false);
    doc.move_word_right(true);
    delete_forward(&mut doc);
    assert_eq!(doc.content, "# foo");
}

/// K2. BASE `1d03313` Ctrl+Shift+Right from the visible end selects the
/// hidden closer and stops there; it does not run on into the next word.
#[test]
fn move_word_right_from_the_visible_end_stops_at_the_closer() {
    let mut doc = heading_tab("# foo #\nnext");
    doc.place(5, false);
    doc.move_word_right(true);
    assert_eq!(doc.caret.range, 5..7);
    assert_eq!(doc.selected_text(), " #");
    delete_forward(&mut doc);
    assert_eq!(doc.content, "# foo\nnext");

    let mut doc = heading_tab("# foo #\n# bar #");
    doc.place(5, false);
    for _ in 0..3 {
        doc.move_word_right(true);
    }
    assert_eq!(doc.caret.range, 5..13);
    assert_eq!(doc.selected_text(), " #\n# bar");
    delete_forward(&mut doc);
    assert_eq!(doc.content, "# foo #");

    let mut doc = heading_tab("# foo #");
    doc.place(5, false);
    doc.move_word_right(true);
    assert_eq!(doc.caret.range, 5..7);
    delete_forward(&mut doc);
    assert_eq!(doc.content, "# foo");
}

fn paint_rows(doc: &mut DocumentTab) -> EditorPaint {
    let parsed = doc.parsed();
    let rows = parsed
        .lines
        .iter()
        .enumerate()
        .map(|(ix, line)| PaintRow {
            line_index: ix,
            byte_start: line.source.start,
            byte_end: line.source.end,
            origin: point(px(0.0), px(28.0 * ix as f32)),
            line_height: px(28.0),
            shaped: std::rc::Rc::new(ShapedLine::default()),
            offset_map: line.offset_map.clone(),
            checkbox_hit: None,
            link_hits: Arc::default(),
            cells: Vec::new(),
        })
        .collect();
    let n = parsed.lines.len() as f32;
    EditorPaint {
        wrap_cols: usize::MAX,
        line_height: px(28.0),
        total_height: px(28.0 * n),
        rows,
        bounds: None,
        ..EditorPaint::default()
    }
}

/// BASE `1d03313` painted Shift+Down on the last row selects through the
/// closer. Delete then matches End.
#[test]
fn painted_shift_down_on_the_last_row_deletes_the_closing_hash() {
    let mut doc = heading_tab("## bar ##");
    let paint = paint_rows(&mut doc);
    doc.caret.move_to(0);
    assert!(move_vertical_visual(
        &paint,
        &doc.content,
        &mut doc.caret,
        1,
        true,
        ViewMode::Wysiwyg,
    ));
    assert_eq!(doc.caret.range, 0..9);
    delete_forward(&mut doc);
    assert_eq!(doc.content, "");

    let src = "hello\n# foo #";
    let mut doc = heading_tab(src);
    let paint = paint_rows(&mut doc);
    doc.place(6, false);
    assert!(move_vertical_visual(
        &paint,
        &doc.content,
        &mut doc.caret,
        1,
        true,
        ViewMode::Wysiwyg,
    ));
    delete_forward(&mut doc);
    assert_eq!(doc.content, "hello\n");
}

/// BASE `1d03313` Ctrl+Shift+Down selects the source line, closer included.
#[test]
fn select_line_down_extends_through_the_closing_hash() {
    let mut doc = heading_tab("# foo #");
    doc.select_line_down();
    assert_eq!(doc.caret.range, 0..7);
    assert_eq!(doc.selected_text(), "# foo #");
    doc.select_line_down();
    assert_eq!(doc.caret.range, 0..7);
    delete_forward(&mut doc);
    assert_eq!(doc.content, "");

    let mut doc = heading_tab("# foo #\nnext");
    doc.select_line_down();
    assert_eq!(doc.caret.range, 0..7);
    assert_eq!(doc.selected_text(), "# foo #");
    delete_forward(&mut doc);
    assert_eq!(doc.content, "\nnext");

    let mut doc = heading_tab("## bar ##");
    doc.select_line_down();
    assert_eq!(doc.selected_text(), "## bar ##");
    delete_forward(&mut doc);
    assert_eq!(doc.content, "");

    let mut doc = heading_tab("# foo #\r\nnext");
    doc.select_line_down();
    assert_eq!(doc.caret.range, 0..8);
    delete_forward(&mut doc);
    assert_eq!(doc.content, "\nnext");
}

/// K1. BASE `1d03313` Ctrl+Shift+Down on the second press includes the
/// CRLF, so Delete leaves the LF behind.
#[test]
fn second_select_line_down_includes_the_crlf() {
    let mut doc = heading_tab("alpha\r\nbeta\r\ngamma");
    doc.select_line_down();
    doc.select_line_down();
    assert_eq!(doc.caret.range, 0..12);
    assert_eq!(doc.selected_text(), "alpha\r\nbeta\r");
    delete_forward(&mut doc);
    assert_eq!(doc.content, "\ngamma");

    let mut doc = heading_tab("# foo #\r\n# bar #\r\nnext");
    doc.select_line_down();
    doc.select_line_down();
    assert_eq!(doc.caret.range, 0..17);
    delete_forward(&mut doc);
    assert_eq!(doc.content, "\nnext");

    let mut doc = heading_tab("alpha\nbeta\ngamma");
    doc.select_line_down();
    doc.select_line_down();
    assert_eq!(doc.caret.range, 0..10);
    delete_forward(&mut doc);
    assert_eq!(doc.content, "\ngamma");

    let mut doc = heading_tab("# foo #\n# bar #\nnext");
    doc.select_line_down();
    doc.select_line_down();
    assert_eq!(doc.caret.range, 0..15);
    delete_forward(&mut doc);
    assert_eq!(doc.content, "\nnext");
}

/// BASE `1d03313` Ctrl+Shift+Delete from before the visible end removes the
/// closer. A caret already on the visible end stays a no-op
/// (`delete_to_end_and_word_right_keep_the_closing_hash`).
#[test]
fn delete_to_line_end_from_before_the_closer_matches_base() {
    let mut doc = heading_tab("# foo #");
    doc.place(0, false);
    doc.delete_to_line_end();
    assert_eq!(doc.content, "");

    let mut doc = heading_tab("# foo #");
    doc.move_home_in_mode(false, ViewMode::Wysiwyg);
    doc.delete_to_line_end();
    assert_eq!(doc.content, "# ");

    let mut doc = heading_tab("# foo #\nnext");
    doc.place(0, false);
    doc.delete_to_line_end();
    assert_eq!(doc.content, "\nnext");

    let mut doc = heading_tab("# foo #\nnext");
    doc.move_home_in_mode(false, ViewMode::Wysiwyg);
    doc.delete_to_line_end();
    assert_eq!(doc.content, "# \nnext");

    let mut doc = heading_tab("# foo #\r\nnext");
    doc.place(0, false);
    doc.delete_to_line_end();
    assert_eq!(doc.content, "\nnext");

    let mut doc = heading_tab("# foo #\r\nnext");
    doc.move_home_in_mode(false, ViewMode::Wysiwyg);
    doc.delete_to_line_end();
    assert_eq!(doc.content, "# \nnext");
}

/// BASE `1d03313` Shift+Right then Shift+Left on `5..7` types `# fooX\nnext`.
/// The place extend snap is what drops the closer.
#[test]
fn shift_right_then_left_on_a_closer_selection_types_over_it() {
    let mut doc = heading_tab("# foo #\nnext");
    doc.caret.range = 5..7;
    app_touch(&mut doc);
    doc.move_right_in_mode(true, ViewMode::Wysiwyg);
    doc.move_left_in_mode(true, ViewMode::Wysiwyg);
    let range = doc.selection_range();
    doc.insert_text_in_mode(range, "X", ViewMode::Wysiwyg);
    assert_eq!(doc.content, "# fooX\nnext");
}

/// BASE `1d03313` Shift+Left then Shift+Right on `0..7` types `X\nnext`.
#[test]
fn shift_left_then_right_on_a_full_heading_selection_matches_base() {
    let mut doc = heading_tab("# foo #\nnext");
    doc.caret.range = 0..7;
    app_touch(&mut doc);
    doc.move_left_in_mode(true, ViewMode::Wysiwyg);
    doc.move_right_in_mode(true, ViewMode::Wysiwyg);
    let range = doc.selection_range();
    doc.insert_text_in_mode(range, "X", ViewMode::Wysiwyg);
    assert_eq!(doc.content, "X\nnext");
}

/// 7ad1cff paints `# foo ` as `foo` and its visible end is offset 5. The
/// trailing space is part of the visible heading now, so the snap target
/// is offset 6 and typing X there appends after it.
#[test]
fn redo_after_raw_closer_delete_types_at_the_visible_end() {
    let mut doc = heading_tab("# foo #");
    doc.set_view_mode(ViewMode::Raw);
    doc.caret.range = 6..7;
    doc.delete_forward_in_mode(ViewMode::Raw);
    assert!(doc.undo());
    doc.set_view_mode(ViewMode::Wysiwyg);
    assert!(doc.redo());
    let range = doc.selection_range();
    doc.insert_text_in_mode(range, "X", ViewMode::Wysiwyg);
    assert_eq!(doc.content, "# foo X");
    assert_eq!(doc.caret.range, 7..7);
}

#[test]
fn test_format_commands_table_select_all_and_toggle() {
    let original_table = "| Header 1 | Header 2 |\n| :--- | :--- |\n| Cell 1 | Cell 2 |";

    let inline_commands = [
        (
            "bold",
            "| **Header 1** | **Header 2** |\n| :--- | :--- |\n| **Cell 1** | **Cell 2** |",
        ),
        (
            "italic",
            "| *Header 1* | *Header 2* |\n| :--- | :--- |\n| *Cell 1* | *Cell 2* |",
        ),
        (
            "underline",
            "| <u>Header 1</u> | <u>Header 2</u> |\n| :--- | :--- |\n| <u>Cell 1</u> | <u>Cell 2</u> |",
        ),
        (
            "strike",
            "| ~~Header 1~~ | ~~Header 2~~ |\n| :--- | :--- |\n| ~~Cell 1~~ | ~~Cell 2~~ |",
        ),
        (
            "code",
            "| `Header 1` | `Header 2` |\n| :--- | :--- |\n| `Cell 1` | `Cell 2` |",
        ),
        (
            "link",
            "| [Header 1](https://) | [Header 2](https://) |\n| :--- | :--- |\n| [Cell 1](https://) | [Cell 2](https://) |",
        ),
    ];

    for (cmd, expected_formatted) in inline_commands {
        // Test Raw mode
        let mut doc = DocumentTab::new(
            "tab".into(),
            "test.md".into(),
            original_table.into(),
            None,
            None,
        );
        doc.select_all();
        doc.apply_format_in_mode(cmd, ViewMode::Raw);
        assert_eq!(
            doc.content, expected_formatted,
            "Inline format '{}' failed on table in Raw mode",
            cmd
        );

        // Toggle off by applying again
        doc.apply_format_in_mode(cmd, ViewMode::Raw);
        assert_eq!(
            doc.content, original_table,
            "Inline format '{}' toggle-off failed on table in Raw mode",
            cmd
        );

        // Verify one undo step restores original
        doc.apply_format_in_mode(cmd, ViewMode::Raw);
        assert_eq!(doc.content, expected_formatted);
        assert!(doc.undo(), "Undo failed for '{}'", cmd);
        assert_eq!(
            doc.content, original_table,
            "Single undo step failed for '{}'",
            cmd
        );

        // Test WYSIWYG mode
        let mut doc_wysiwyg = DocumentTab::new(
            "tab".into(),
            "test.md".into(),
            original_table.into(),
            None,
            None,
        );
        doc_wysiwyg.select_all();
        doc_wysiwyg.apply_format_in_mode(cmd, ViewMode::Wysiwyg);
        assert_eq!(
            doc_wysiwyg.content, expected_formatted,
            "Inline format '{}' failed on table in Wysiwyg mode",
            cmd
        );

        doc_wysiwyg.apply_format_in_mode(cmd, ViewMode::Wysiwyg);
        assert_eq!(
            doc_wysiwyg.content, original_table,
            "Inline format '{}' toggle-off failed on table in Wysiwyg mode",
            cmd
        );
    }

    let line_commands = [
        "h1",
        "h2",
        "h3",
        "ul",
        "ol",
        "checklist",
        "quote",
        "codeblock",
    ];

    for cmd in line_commands {
        let mut doc = DocumentTab::new(
            "tab".into(),
            "test.md".into(),
            original_table.into(),
            None,
            None,
        );
        doc.select_all();
        doc.apply_format_in_mode(cmd, ViewMode::Raw);
        assert_eq!(
            doc.content, original_table,
            "Line format '{}' should skip table rows and preserve table",
            cmd
        );

        doc.apply_format_in_mode(cmd, ViewMode::Raw);
        assert_eq!(
            doc.content, original_table,
            "Line format '{}' second apply should keep table intact",
            cmd
        );

        // Also in WYSIWYG mode
        let mut doc_wysiwyg = DocumentTab::new(
            "tab".into(),
            "test.md".into(),
            original_table.into(),
            None,
            None,
        );
        doc_wysiwyg.select_all();
        doc_wysiwyg.apply_format_in_mode(cmd, ViewMode::Wysiwyg);
        assert_eq!(
            doc_wysiwyg.content, original_table,
            "Line format '{}' in Wysiwyg should skip table rows",
            cmd
        );
    }
}

#[test]
fn test_format_commands_plain_lines_select_all_and_toggle() {
    let original_plain = "First line\nSecond line\nThird line";

    let inline_commands = [
        ("bold", "**First line**\n**Second line**\n**Third line**"),
        ("italic", "*First line*\n*Second line*\n*Third line*"),
        (
            "underline",
            "<u>First line</u>\n<u>Second line</u>\n<u>Third line</u>",
        ),
        ("strike", "~~First line~~\n~~Second line~~\n~~Third line~~"),
        ("code", "`First line`\n`Second line`\n`Third line`"),
        (
            "link",
            "[First line](https://)\n[Second line](https://)\n[Third line](https://)",
        ),
    ];

    for (cmd, expected_formatted) in inline_commands {
        let mut doc = DocumentTab::new(
            "tab".into(),
            "test.md".into(),
            original_plain.into(),
            None,
            None,
        );
        doc.select_all();
        doc.apply_format_in_mode(cmd, ViewMode::Raw);
        assert_eq!(
            doc.content, expected_formatted,
            "Inline format '{}' failed across plain lines",
            cmd
        );

        // Toggle off
        doc.apply_format_in_mode(cmd, ViewMode::Raw);
        assert_eq!(
            doc.content, original_plain,
            "Inline format '{}' toggle-off failed across plain lines",
            cmd
        );

        // Undo as one step
        doc.apply_format_in_mode(cmd, ViewMode::Raw);
        assert_eq!(doc.content, expected_formatted);
        assert!(doc.undo(), "Undo failed for '{}'", cmd);
        assert_eq!(
            doc.content, original_plain,
            "Single undo step failed for '{}' across plain lines",
            cmd
        );
    }

    let line_commands = [
        ("h1", "# First line\n# Second line\n# Third line"),
        ("h2", "## First line\n## Second line\n## Third line"),
        ("h3", "### First line\n### Second line\n### Third line"),
        ("ul", "- First line\n- Second line\n- Third line"),
        ("ol", "1. First line\n2. Second line\n3. Third line"),
        (
            "checklist",
            "- [ ] First line\n- [ ] Second line\n- [ ] Third line",
        ),
        ("quote", "> First line\n> Second line\n> Third line"),
        ("codeblock", "```\nFirst line\nSecond line\nThird line\n```"),
    ];

    for (cmd, expected_formatted) in line_commands {
        let mut doc = DocumentTab::new(
            "tab".into(),
            "test.md".into(),
            original_plain.into(),
            None,
            None,
        );
        doc.select_all();
        doc.apply_format_in_mode(cmd, ViewMode::Raw);
        assert_eq!(
            doc.content, expected_formatted,
            "Line format '{}' failed across plain lines",
            cmd
        );

        // Toggle off
        doc.apply_format_in_mode(cmd, ViewMode::Raw);
        assert_eq!(
            doc.content, original_plain,
            "Line format '{}' toggle-off failed across plain lines",
            cmd
        );

        // Undo
        doc.apply_format_in_mode(cmd, ViewMode::Raw);
        assert_eq!(doc.content, expected_formatted);
        assert!(doc.undo(), "Undo failed for '{}'", cmd);
        assert_eq!(
            doc.content, original_plain,
            "Undo failed for '{}' across plain lines",
            cmd
        );
    }
}

#[test]
fn test_format_commands_mixed_plain_lines_and_table() {
    let mixed =
        "Before table\n\n| Col 1 | Col 2 |\n| :--- | :--- |\n| Val 1 | Val 2 |\n\nAfter table";

    // Applying h1 affects lines before and after table, but skips the table rows
    let mut doc = DocumentTab::new("tab".into(), "test.md".into(), mixed.into(), None, None);
    doc.select_all();
    doc.apply_format("h1");
    let expected_h1 = "# Before table\n\n| Col 1 | Col 2 |\n| :--- | :--- |\n| Val 1 | Val 2 |\n\n# After table";
    assert_eq!(doc.content, expected_h1);

    // Toggle off restores
    doc.apply_format("h1");
    assert_eq!(doc.content, mixed);

    // Applying bold formats plain lines and table cells individually
    let mut doc_bold =
        DocumentTab::new("tab".into(), "test.md".into(), mixed.into(), None, None);
    doc_bold.select_all();
    doc_bold.apply_format("bold");
    let expected_bold = "**Before table**\n\n| **Col 1** | **Col 2** |\n| :--- | :--- |\n| **Val 1** | **Val 2** |\n\n**After table**";
    assert_eq!(doc_bold.content, expected_bold);

    // Toggle off restores
    doc_bold.apply_format("bold");
    assert_eq!(doc_bold.content, mixed);
}

#[test]
fn test_inline_format_partial_selection_across_cells() {
    let doc_text = "| Alpha Beta | Gamma Delta |\n| :--- | :--- |";
    let mut doc = DocumentTab::new("tab".into(), "test.md".into(), doc_text.into(), None, None);

    // Select from "Beta" to "Gamma"
    let start = doc_text.find("Beta").unwrap();
    let end = doc_text.find("Delta").unwrap();
    doc.caret.range = start..end;
    doc.apply_format("bold");
    assert_eq!(
        doc.content,
        "| Alpha **Beta** | **Gamma** Delta |\n| :--- | :--- |"
    );

    // Toggle off
    doc.apply_format("bold");
    assert_eq!(doc.content, doc_text);
}

#[test]
fn test_unwrap_outer_link_safety() {
    // Valid link
    let text = "before [inner](https://example.com) after";
    let inner_start = text.find("inner").unwrap();
    let inner_end = inner_start + "inner".len();
    assert_eq!(
        unwrap_outer_link(text, inner_start, inner_end),
        Some((
            text.find('[').unwrap()..(text.find(')').unwrap() + 1),
            "inner".to_string()
        ))
    );

    // Missing closing parenthesis must return None without panicking
    let unclosed = "before [inner](https://example.com after";
    let u_start = unclosed.find("inner").unwrap();
    let u_end = u_start + "inner".len();
    assert_eq!(unwrap_outer_link(unclosed, u_start, u_end), None);

    // Out of bounds / invalid bounds must return None without panicking
    assert_eq!(unwrap_outer_link(text, 0, text.len()), None);
    assert_eq!(unwrap_outer_link(text, 100, 200), None);
    assert_eq!(unwrap_outer_link("", 0, 0), None);
}

fn table_doc(text: &str) -> DocumentTab {
    DocumentTab::new("t".into(), "test.md".into(), text.into(), None, None)
}

const TBL: &str = "| A | B |\n| --- | --- |\n| a1 | b1 |\n| a2 | b2 |";

fn line_kind_at(doc: &DocumentTab, needle: &str) -> LineKind {
    let at = doc.content.find(needle).unwrap();
    crate::markdown::parse_document(&doc.content)
        .line_at_offset(at)
        .unwrap()
        .kind
        .clone()
}

#[test]
fn table_exit_leaves_blank_line_and_next_text_is_a_paragraph() {
    let mut doc = table_doc(TBL);
    doc.place(TBL.len(), false);
    doc.smart_newline(false); // adds a row
    doc.smart_newline(false); // exits
    assert_eq!(doc.content, format!("{TBL}\n\n"));
    assert_eq!(doc.cursor_offset(), doc.content.len());
    let at = doc.cursor_offset();
    doc.insert_text_in_mode(at..at, "hello", ViewMode::Wysiwyg);
    assert!(matches!(line_kind_at(&doc, "hello"), LineKind::Paragraph));
    assert!(matches!(
        line_kind_at(&doc, "b2"),
        LineKind::TableRow { .. }
    ));
}

#[test]
fn table_exit_reuses_an_existing_blank_line() {
    let text = format!("{TBL}\n|  |  |\n\nafter");
    let mut doc = table_doc(&text);
    doc.place(text.find("|  |").unwrap() + 1, false);
    doc.smart_newline(false);
    assert_eq!(doc.content, format!("{TBL}\n\nafter"));
    assert_eq!(doc.cursor_offset(), TBL.len() + 2);
    // A directly adjacent paragraph gets a blank line inserted
    let text = format!("{TBL}\n|  |  |\n# after");
    let mut doc = table_doc(&text);
    doc.place(text.find("|  |").unwrap() + 1, false);
    doc.smart_newline(false);
    assert_eq!(doc.content, format!("{TBL}\n\n# after"));
    assert_eq!(doc.cursor_offset(), TBL.len() + 2);
    // one undo step
    assert!(doc.undo());
    assert_eq!(doc.content, text);
}

#[test]
fn enter_on_empty_middle_row_moves_to_same_column_next_row() {
    let text = "| A | B |\n| --- | --- |\n|  |  |\n| x | y |";
    let mut doc = table_doc(text);
    // caret in the second (empty) cell of the empty row
    let row = text.find("|  |  |").unwrap();
    doc.place(row + 4, false);
    doc.smart_newline(false);
    assert_eq!(doc.content, text);
    assert_eq!(doc.cursor_offset(), text.find("y |").unwrap());
    // first column
    doc.place(row + 1, false);
    doc.smart_newline(false);
    assert_eq!(doc.content, text);
    assert_eq!(doc.cursor_offset(), text.find("x |").unwrap());
}

#[test]
fn shift_enter_leaves_the_table_without_changing_it() {
    for pos in [0usize, TBL.find("a1").unwrap() + 1, TBL.find("b2").unwrap() + 2] {
        let mut doc = table_doc(TBL);
        doc.place(pos, false);
        doc.smart_newline(true);
        assert_eq!(doc.content, format!("{TBL}\n\n"));
        assert_eq!(doc.cursor_offset(), doc.content.len());
    }
    // existing text below keeps one blank line
    let text = format!("{TBL}\n# next");
    let mut doc = table_doc(&text);
    doc.place(text.find("a1").unwrap(), false);
    doc.smart_newline(true);
    assert_eq!(doc.content, format!("{TBL}\n\n# next"));
    // outside tables Shift+Enter is a plain newline
    let mut doc = table_doc("hello world");
    doc.place(5, false);
    doc.smart_newline(true);
    assert_eq!(doc.content, "hello\n world");
}

#[test]
fn ctrl_enter_adds_a_row_below_in_the_same_column() {
    let mut doc = table_doc(TBL);
    let pos = TBL.find("b1").unwrap() + 1;
    doc.place(pos, false);
    assert!(doc.table_insert_row_below());
    let expected = "| A | B |\n| --- | --- |\n| a1 | b1 |\n|  |  |\n| a2 | b2 |";
    assert_eq!(doc.content, expected);
    assert_eq!(doc.cursor_offset(), expected.find("|  |  |").unwrap() + 5);
    // header row inserts below the delimiter
    let mut doc = table_doc(TBL);
    doc.place(2, false);
    assert!(doc.table_insert_row_below());
    assert!(doc.content.starts_with("| A | B |\n| --- | --- |\n|  |  |\n| a1"));
    assert_eq!(doc.cursor_offset(), doc.content.find("|  |  |").unwrap() + 1 + 2 - 1);
    // outside a table nothing happens
    let mut doc = table_doc("plain");
    assert!(!doc.table_insert_row_below());
    assert_eq!(doc.content, "plain");
}

#[test]
fn up_and_down_leave_the_table_from_its_edge_rows() {
    // Down from the last row of a table that ends the note
    let mut doc = table_doc(TBL);
    doc.place(TBL.len() - 2, false);
    assert!(doc.table_edge_exit(1));
    assert_eq!(doc.content, format!("{TBL}\n\n"));
    assert_eq!(doc.cursor_offset(), doc.content.len());
    // Up from the first row of a table that starts the note
    let mut doc = table_doc(TBL);
    doc.place(3, false);
    assert!(doc.table_edge_exit(-1));
    assert_eq!(doc.content, format!("\n\n{TBL}"));
    assert_eq!(doc.cursor_offset(), 0);
    // With a line above and below the caret just moves there
    let text = format!("above\n\n{TBL}\n\nbelow");
    let mut doc = table_doc(&text);
    doc.place(text.find("A |").unwrap(), false);
    assert!(doc.table_edge_exit(-1));
    assert_eq!(doc.cursor_offset(), "above\n".len());
    doc.place(text.find("b2").unwrap(), false);
    assert!(doc.table_edge_exit(1));
    assert_eq!(doc.cursor_offset(), text.find("below").unwrap() - 1);
    // Middle rows are not edges
    doc.place(text.find("a1").unwrap(), false);
    assert!(!doc.table_edge_exit(1));
    assert!(!doc.table_edge_exit(-1));
}

#[test]
fn left_and_right_jump_between_cells_not_pipes() {
    let mut doc = table_doc(TBL);
    let a1 = TBL.find("a1").unwrap();
    let b1 = TBL.find("b1").unwrap();
    // Left at the start of B1 goes to the end of A1
    doc.place(b1, false);
    doc.move_left_in_mode(false, ViewMode::Wysiwyg);
    assert_eq!(doc.cursor_offset(), a1 + 2);
    // Right at the end of A1 goes to the start of B1
    doc.move_right_in_mode(false, ViewMode::Wysiwyg);
    assert_eq!(doc.cursor_offset(), b1);
    // End of a row goes to the next row's first cell, start goes to previous row's last
    doc.place(b1 + 2, false);
    doc.move_right_in_mode(false, ViewMode::Wysiwyg);
    assert_eq!(doc.cursor_offset(), TBL.find("a2").unwrap());
    doc.move_left_in_mode(false, ViewMode::Wysiwyg);
    assert_eq!(doc.cursor_offset(), b1 + 2);
    // Walking right through the first body row never stops next to a pipe
    doc.place(a1, false);
    for _ in 0..4 {
        doc.move_right_in_mode(false, ViewMode::Wysiwyg);
        let c = doc.cursor_offset();
        assert_ne!(doc.content.as_bytes()[c], b'|', "caret before a pipe at {c}");
        assert_ne!(doc.content.as_bytes()[c - 1], b'|', "caret after a pipe at {c}");
    }
}

#[test]
fn shift_tab_in_the_first_cell_stays_put() {
    let mut doc = table_doc(TBL);
    let a = TBL.find('A').unwrap();
    doc.place(a, false);
    assert!(doc.tab_table_cell(true));
    assert_eq!(doc.cursor_offset(), a);
    assert_eq!(doc.content, TBL);
    // elsewhere it still goes back one cell
    let b = TBL.find('B').unwrap();
    doc.place(b, false);
    assert!(doc.tab_table_cell(true));
    assert_eq!(doc.selection_range().start, a);
}

#[test]
fn test_inline_format_stacked_combos_table_driven() {
    let combos: Vec<(&[&str], &str)> = vec![
        // 2-format combos (6)
        (&["bold", "italic"], "***Hello***"),
        (&["bold", "underline"], "<u>**Hello**</u>"),
        (&["bold", "strike"], "**~~Hello~~**"),
        (&["italic", "underline"], "<u>*Hello*</u>"),
        (&["italic", "strike"], "*~~Hello~~*"),
        (&["strike", "underline"], "<u>~~Hello~~</u>"),
        // 3-format combos (4)
        (&["bold", "italic", "underline"], "<u>***Hello***</u>"),
        (&["bold", "italic", "strike"], "***~~Hello~~***"),
        (&["bold", "strike", "underline"], "<u>**~~Hello~~**</u>"),
        (&["italic", "strike", "underline"], "<u>*~~Hello~~*</u>"),
    ];

    let positions = ["whole", "start", "middle", "end", "caret"];

    for (combo_formats, initial_md) in &combos {
        for &cmd in *combo_formats {
            for &pos in &positions {
                let mut doc = DocumentTab::new(
                    "tab".into(),
                    "test.md".into(),
                    initial_md.to_string(),
                    None,
                    None,
                );

                let word_start = doc.content.find("Hello").unwrap();
                match pos {
                    "whole" => {
                        doc.caret.range = word_start..(word_start + 5);
                    }
                    "start" => {
                        doc.caret.range = word_start..(word_start + 2);
                    }
                    "middle" => {
                        doc.caret.range = (word_start + 1)..(word_start + 4);
                    }
                    "end" => {
                        doc.caret.range = (word_start + 2)..(word_start + 5);
                    }
                    "caret" => {
                        doc.place(word_start + 2, false);
                    }
                    _ => unreachable!(),
                }

                doc.apply_format_in_mode(cmd, ViewMode::Wysiwyg);

                let parsed = doc.parsed();
                let line = parsed.lines.first().expect("Line not found");

                // 1. No leaked markers in display text
                assert_eq!(
                    line.display_text, "Hello",
                    "Display text leaked markers for combo {:?}, format {}, pos {}. Content was: {}",
                    combo_formats, cmd, pos, doc.content
                );

                // 2. Clean delimiters: no **** or <u><u> or ~~~~
                assert!(
                    !doc.content.contains("****"),
                    "Content contains **** for combo {:?}, format {}, pos {}: {}",
                    combo_formats, cmd, pos, doc.content
                );
                assert!(
                    !doc.content.contains("<u><u>"),
                    "Content contains <u><u> for combo {:?}, format {}, pos {}: {}",
                    combo_formats, cmd, pos, doc.content
                );
                assert!(
                    !doc.content.contains("</u></u>"),
                    "Content contains </u></u> for combo {:?}, format {}, pos {}: {}",
                    combo_formats, cmd, pos, doc.content
                );
                assert!(
                    !doc.content.contains("~~~~"),
                    "Content contains ~~~~ for combo {:?}, format {}, pos {}: {}",
                    combo_formats, cmd, pos, doc.content
                );

                // 3. Verify format state of each letter in "Hello"
                for i in 0..5 {
                    let should_have_toggled = match pos {
                        "whole" | "caret" => false,
                        "start" => i >= 2,
                        "middle" => i == 0 || i == 4,
                        "end" => i < 2,
                        _ => unreachable!(),
                    };

                    let src_offset = line.offset_map.display_to_source(i);
                    let seg = line
                        .styled_segments
                        .iter()
                        .find(|s| s.source.start <= src_offset && src_offset < s.source.end)
                        .unwrap_or_else(|| {
                            panic!(
                                "No styled segment covering char index {} at source offset {} for combo {:?}, format {}, pos {}. Content: {}",
                                i, src_offset, combo_formats, cmd, pos, doc.content
                            )
                        });

                    let has_cmd = match cmd {
                        "bold" => seg.bold,
                        "italic" => seg.italic,
                        "underline" => seg.underline,
                        "strike" => seg.strike,
                        _ => unreachable!(),
                    };
                    assert_eq!(
                        has_cmd, should_have_toggled,
                        "Format {} on char index {} expected {} but was {} for combo {:?}, pos {}. Content: {}",
                        cmd, i, should_have_toggled, has_cmd, combo_formats, pos, doc.content
                    );

                    // All other formats in combo_formats must remain true
                    for &other_cmd in *combo_formats {
                        if other_cmd == cmd {
                            continue;
                        }
                        let has_other = match other_cmd {
                            "bold" => seg.bold,
                            "italic" => seg.italic,
                            "underline" => seg.underline,
                            "strike" => seg.strike,
                            _ => unreachable!(),
                        };
                        assert!(
                            has_other,
                            "Other format {} on char index {} lost for combo {:?}, toggled {}, pos {}. Content: {}",
                            other_cmd, i, combo_formats, cmd, pos, doc.content
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn test_wysiwyg_typing_format_splice_inner_outer_plain() {
    // Arm bold + italic, type "now", turn off italic, type "ok", turn off bold, type " plain"
    let mut doc = DocumentTab::new("t1".into(), "test.md".into(), String::new(), None, None);
    doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
    doc.apply_format_in_mode("italic", ViewMode::Wysiwyg);
    type_keystrokes(&mut doc, "now");
    assert_eq!(doc.content, "***now***");
    doc.apply_format_in_mode("italic", ViewMode::Wysiwyg);
    type_keystrokes(&mut doc, "ok");
    assert_eq!(doc.content, "***now*ok**");
    doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
    type_keystrokes(&mut doc, " plain");
    assert_eq!(doc.content, "***now*ok** plain");
    let parsed = doc.parsed();
    let line = parsed.lines.first().unwrap();
    assert_eq!(line.display_text, "nowok plain");

    // Arm bold + italic, type "now", turn off bold (outer), type "ok", turn off italic, type " plain"
    let mut doc = DocumentTab::new("t2".into(), "test.md".into(), String::new(), None, None);
    doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
    doc.apply_format_in_mode("italic", ViewMode::Wysiwyg);
    type_keystrokes(&mut doc, "now");
    doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
    type_keystrokes(&mut doc, "ok");
    assert_eq!(doc.content, "***now**ok*");
    doc.apply_format_in_mode("italic", ViewMode::Wysiwyg);
    type_keystrokes(&mut doc, " plain");
    assert_eq!(doc.content, "***now**ok* plain");
    let parsed = doc.parsed();
    let line = parsed.lines.first().unwrap();
    assert_eq!(line.display_text, "nowok plain");

    // Arm bold + underline, type "now", turn off underline (inner), type "ok", turn off bold, type " plain"
    let mut doc = DocumentTab::new("t3".into(), "test.md".into(), String::new(), None, None);
    doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
    doc.apply_format_in_mode("underline", ViewMode::Wysiwyg);
    type_keystrokes(&mut doc, "now");
    assert_eq!(doc.content, "<u>**now**</u>");
    doc.apply_format_in_mode("underline", ViewMode::Wysiwyg);
    type_keystrokes(&mut doc, "ok");
    assert_eq!(doc.content, "**<u>now</u>ok**");
    doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
    type_keystrokes(&mut doc, " plain");
    assert_eq!(doc.content, "**<u>now</u>ok** plain");
    let parsed = doc.parsed();
    let line = parsed.lines.first().unwrap();
    assert_eq!(line.display_text, "nowok plain");

    // Arm bold + underline, type "now", turn off bold (outer), type "ok", turn off underline, type " plain"
    let mut doc = DocumentTab::new("t4".into(), "test.md".into(), String::new(), None, None);
    doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
    doc.apply_format_in_mode("underline", ViewMode::Wysiwyg);
    type_keystrokes(&mut doc, "now");
    doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
    type_keystrokes(&mut doc, "ok");
    assert_eq!(doc.content, "<u>**now**ok</u>");
    doc.apply_format_in_mode("underline", ViewMode::Wysiwyg);
    type_keystrokes(&mut doc, " plain");
    assert_eq!(doc.content, "<u>**now**ok</u> plain");
    let parsed = doc.parsed();
    let line = parsed.lines.first().unwrap();
    assert_eq!(line.display_text, "nowok plain");
}

#[test]
fn test_wysiwyg_typing_req5_consecutive_italic_toggle() {
    let mut doc = DocumentTab::new("t".into(), "test.md".into(), String::new(), None, None);
    type_keystrokes(&mut doc, "a ");
    doc.apply_format_in_mode("italic", ViewMode::Wysiwyg);
    type_keystrokes(&mut doc, "b");
    doc.apply_format_in_mode("italic", ViewMode::Wysiwyg);
    doc.apply_format_in_mode("italic", ViewMode::Wysiwyg);
    type_keystrokes(&mut doc, "c");
    doc.apply_format_in_mode("italic", ViewMode::Wysiwyg);
    type_keystrokes(&mut doc, " d");
    assert_eq!(doc.content, "a *bc* d");
    let line = doc.parsed().lines.first().unwrap().clone();
    assert_eq!(line.display_text, "a bc d");
}

#[test]
fn test_wysiwyg_typing_req6_underline_and_code_trailing_space() {
    let mut doc = DocumentTab::new("u".into(), "test.md".into(), String::new(), None, None);
    doc.apply_format_in_mode("underline", ViewMode::Wysiwyg);
    type_keystrokes(&mut doc, "hello ");
    doc.apply_format_in_mode("underline", ViewMode::Wysiwyg);
    type_keystrokes(&mut doc, "world");
    assert_eq!(doc.content, "<u>hello</u> world");
    let line = doc.parsed().lines.first().unwrap().clone();
    assert_eq!(line.display_text, "hello world");

    let mut doc = DocumentTab::new("c".into(), "test.md".into(), String::new(), None, None);
    doc.apply_format_in_mode("code", ViewMode::Wysiwyg);
    type_keystrokes(&mut doc, "hello ");
    doc.apply_format_in_mode("code", ViewMode::Wysiwyg);
    type_keystrokes(&mut doc, "world");
    assert_eq!(doc.content, "`hello` world");
    let line = doc.parsed().lines.first().unwrap().clone();
    assert_eq!(line.display_text, "hello world");
}

#[test]
fn test_wysiwyg_repro1_stacked_format_typing_continuation() {
    let mut doc = DocumentTab::new("r1".into(), "test.md".into(), String::new(), None, None);
    type_keystrokes(&mut doc, "This is a test for ");
    doc.apply_format_in_mode("italic", ViewMode::Wysiwyg);
    doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
    doc.apply_format_in_mode("underline", ViewMode::Wysiwyg);
    type_keystrokes(&mut doc, "combination i want ");
    doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
    type_keystrokes(&mut doc, "jlkj ");
    doc.apply_format_in_mode("underline", ViewMode::Wysiwyg);

    assert_eq!(
        doc.content,
        "This is a test for <u>***combination i want** jlkj*</u> "
    );
    let parsed = crate::markdown::parse_document(&doc.content);
    let line = parsed.lines.first().unwrap();
    assert_eq!(line.display_text, "This is a test for combination i want jlkj ");

    let s_lead = line.styled_segments.iter().find(|s| s.source == (0..19)).unwrap();
    assert!(!s_lead.bold && !s_lead.italic && !s_lead.underline && !s_lead.strike);

    let s_combo = line.styled_segments.iter().find(|s| &doc.content[s.source.clone()] == "combination i want").unwrap();
    assert!(s_combo.bold && s_combo.italic && s_combo.underline);

    let s_jlkj = line.styled_segments.iter().find(|s| &doc.content[s.source.clone()] == " jlkj").unwrap();
    assert!(!s_jlkj.bold && s_jlkj.italic && s_jlkj.underline);
}

#[test]
fn test_wysiwyg_repro2_stacked_format_typing_disarm_plain() {
    let mut doc = DocumentTab::new("r2".into(), "test.md".into(), String::new(), None, None);
    doc.apply_format_in_mode("italic", ViewMode::Wysiwyg);
    doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
    doc.apply_format_in_mode("underline", ViewMode::Wysiwyg);
    type_keystrokes(&mut doc, "combination i want");
    doc.apply_format_in_mode("italic", ViewMode::Wysiwyg);
    doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
    doc.apply_format_in_mode("underline", ViewMode::Wysiwyg);
    type_keystrokes(&mut doc, "fasjj");

    assert_eq!(doc.content, "<u>***combination i want***</u>fasjj");
    let parsed = crate::markdown::parse_document(&doc.content);
    let line = parsed.lines.first().unwrap();
    assert_eq!(line.display_text, "combination i wantfasjj");

    let s_formatted = line.styled_segments.iter().find(|s| &doc.content[s.source.clone()] == "combination i want").unwrap();
    assert!(s_formatted.bold && s_formatted.italic && s_formatted.underline && !s_formatted.strike);

    let s_plain = line.styled_segments.iter().find(|s| &doc.content[s.source.clone()] == "fasjj").unwrap();
    assert!(!s_plain.bold && !s_plain.italic && !s_plain.underline && !s_plain.strike);
}

#[test]
fn test_all_stacked_combos_followed_by_unspaced_text() {
    let combos = [
        // Pairs
        (vec!["bold", "italic"], true, true, false, false),
        (vec!["bold", "underline"], true, false, true, false),
        (vec!["italic", "underline"], false, true, true, false),
        (vec!["strike", "underline"], false, false, true, true),
        (vec!["bold", "strike"], true, false, false, true),
        (vec!["italic", "strike"], false, true, false, true),
        // Triples
        (vec!["bold", "italic", "underline"], true, true, true, false),
        (vec!["bold", "strike", "underline"], true, false, true, true),
        (vec!["italic", "strike", "underline"], false, true, true, true),
        (vec!["bold", "italic", "strike"], true, true, false, true),
    ];

    for (fmts, b, i, u, s) in combos {
        let mut doc = DocumentTab::new("c".into(), "test.md".into(), String::new(), None, None);
        for &cmd in &fmts {
            doc.apply_format_in_mode(cmd, ViewMode::Wysiwyg);
        }
        type_keystrokes(&mut doc, "word");
        for &cmd in &fmts {
            doc.apply_format_in_mode(cmd, ViewMode::Wysiwyg);
        }
        type_keystrokes(&mut doc, "after");

        // Never emit empty format spans
        assert!(
            !doc.content.contains("<u></u>"),
            "Found empty <u></u> for {:?}: {:?}",
            fmts,
            doc.content
        );
        assert!(
            !doc.content.contains("****"),
            "Found empty **** for {:?}: {:?}",
            fmts,
            doc.content
        );
        assert!(
            !doc.content.contains("~~~~"),
            "Found empty ~~~~ for {:?}: {:?}",
            fmts,
            doc.content
        );

        // Combos with underline or single delimiter run (bold+italic) parse cleanly even without space
        if u || fmts == ["bold", "italic"] {
            let parsed = crate::markdown::parse_document(&doc.content);
            let line = parsed.lines.first().unwrap();
            assert_eq!(
                line.display_text, "wordafter",
                "Leaked delimiters for combo {:?}. Content was: {:?}",
                fmts, doc.content
            );

            let seg_word = line
                .styled_segments
                .iter()
                .find(|seg| &doc.content[seg.source.clone()] == "word")
                .unwrap_or_else(|| panic!("Segment 'word' not found for {:?}, content: {:?}", fmts, doc.content));
            assert_eq!(seg_word.bold, b, "{:?}: bold mismatch", fmts);
            assert_eq!(seg_word.italic, i, "{:?}: italic mismatch", fmts);
            assert_eq!(seg_word.underline, u, "{:?}: underline mismatch", fmts);
            assert_eq!(seg_word.strike, s, "{:?}: strike mismatch", fmts);

            let seg_after = line
                .styled_segments
                .iter()
                .find(|seg| &doc.content[seg.source.clone()] == "after")
                .unwrap_or_else(|| panic!("Segment 'after' not found for {:?}, content: {:?}", fmts, doc.content));
            assert!(
                !seg_after.bold && !seg_after.italic && !seg_after.underline && !seg_after.strike,
                "{:?}: after was styled",
                fmts
            );
        }
    }
}

#[test]
fn test_arrowing_back_into_stacked_span_and_typing() {
    let mut doc = DocumentTab::new("arrow".into(), "test.md".into(), String::new(), None, None);
    doc.apply_format_in_mode("italic", ViewMode::Wysiwyg);
    doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
    doc.apply_format_in_mode("underline", ViewMode::Wysiwyg);
    type_keystrokes(&mut doc, "hello");
    doc.apply_format_in_mode("italic", ViewMode::Wysiwyg);
    doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
    doc.apply_format_in_mode("underline", ViewMode::Wysiwyg);
    type_keystrokes(&mut doc, " world");

    // Arrow left 7 times back into the stacked span
    for _ in 0..7 {
        doc.move_left_in_mode(false, ViewMode::Wysiwyg);
    }
    type_keystrokes(&mut doc, "X");

    let parsed = crate::markdown::parse_document(&doc.content);
    let line = parsed.lines.first().unwrap();
    assert_eq!(line.display_text, "hellXo world");

    let seg_mod = line
        .styled_segments
        .iter()
        .find(|seg| &doc.content[seg.source.clone()] == "hellXo")
        .expect("hellXo segment not found");
    assert!(seg_mod.bold && seg_mod.italic && seg_mod.underline && !seg_mod.strike);

    let seg_world = line
        .styled_segments
        .iter()
        .find(|seg| &doc.content[seg.source.clone()] == " world")
        .expect("world segment not found");
    assert!(!seg_world.bold && !seg_world.italic && !seg_world.underline && !seg_world.strike);
}

#[test]
fn test_b_i_s_c_pairs_and_triples_toggled_off_and_on_adjacent_to_letters() {
    let combos: Vec<(&[&str], bool, bool, bool, bool)> = vec![
        // Pairs (6)
        (&["bold", "italic"], true, true, false, false),
        (&["bold", "strike"], true, false, true, false),
        (&["bold", "code"], true, false, false, true),
        (&["italic", "strike"], false, true, true, false),
        (&["italic", "code"], false, true, false, true),
        (&["strike", "code"], false, false, true, true),
        // Triples (4)
        (&["bold", "italic", "strike"], true, true, true, false),
        (&["bold", "italic", "code"], true, true, false, true),
        (&["bold", "strike", "code"], true, false, true, true),
        (&["italic", "strike", "code"], false, true, true, true),
    ];

    for (combo, exp_b, exp_i, exp_s, exp_c) in combos {
        // 1. Toggled on immediately after a letter: type "a", arm combo, type "hi"
        {
            let mut doc = DocumentTab::new("on".into(), "test.md".into(), String::new(), None, None);
            type_keystrokes(&mut doc, "a");
            for &cmd in combo {
                doc.apply_format_in_mode(cmd, ViewMode::Wysiwyg);
            }
            type_keystrokes(&mut doc, "hi");

            let parsed = crate::markdown::parse_document(&doc.content);
            let line = parsed.lines.first().expect("line not found");
            assert_eq!(
                line.display_text, "ahi",
                "Leaked delimiters in display text when toggling on combo {:?}. Content was: {:?}",
                combo, doc.content
            );

            // 'a' must be unstyled
            let a_src = line.offset_map.display_to_source(0);
            let a_seg = line
                .styled_segments
                .iter()
                .find(|s| s.source.start <= a_src && a_src < s.source.end)
                .unwrap_or_else(|| panic!("No styled segment covering 'a' for combo {:?}", combo));
            assert!(!a_seg.bold && !a_seg.italic && !a_seg.strike && !a_seg.code,
                "Expected 'a' unstyled for combo {:?}, got B={} I={} S={} C={}",
                combo, a_seg.bold, a_seg.italic, a_seg.strike, a_seg.code);

            // 'h' and 'i' must have the combo formats
            for ch_idx in 1..3 {
                let src = line.offset_map.display_to_source(ch_idx);
                let seg = line
                    .styled_segments
                    .iter()
                    .find(|s| s.source.start <= src && src < s.source.end)
                    .unwrap_or_else(|| panic!("No styled segment covering char {} for combo {:?}", ch_idx, combo));
                assert_eq!(seg.bold, exp_b, "Bold mismatch on char {} for combo ON {:?}. Content: {:?}", ch_idx, combo, doc.content);
                assert_eq!(seg.italic, exp_i, "Italic mismatch on char {} for combo ON {:?}. Content: {:?}", ch_idx, combo, doc.content);
                assert_eq!(seg.strike, exp_s, "Strike mismatch on char {} for combo ON {:?}. Content: {:?}", ch_idx, combo, doc.content);
                assert_eq!(seg.code, exp_c, "Code mismatch on char {} for combo ON {:?}. Content: {:?}", ch_idx, combo, doc.content);
            }
        }

        // 2. Toggled off with a letter typed immediately after: arm combo, type "hi", toggle off, type "y"
        {
            let mut doc = DocumentTab::new("off".into(), "test.md".into(), String::new(), None, None);
            for &cmd in combo {
                doc.apply_format_in_mode(cmd, ViewMode::Wysiwyg);
            }
            type_keystrokes(&mut doc, "hi");
            for &cmd in combo {
                doc.apply_format_in_mode(cmd, ViewMode::Wysiwyg);
            }
            type_keystrokes(&mut doc, "y");

            let parsed = crate::markdown::parse_document(&doc.content);
            let line = parsed.lines.first().expect("line not found");
            assert_eq!(
                line.display_text, "hiy",
                "Leaked delimiters in display text when toggling off combo {:?}. Content was: {:?}",
                combo, doc.content
            );

            // 'h' and 'i' must have the combo formats
            for ch_idx in 0..2 {
                let src = line.offset_map.display_to_source(ch_idx);
                let seg = line
                    .styled_segments
                    .iter()
                    .find(|s| s.source.start <= src && src < s.source.end)
                    .unwrap_or_else(|| panic!("No styled segment covering char {} for combo {:?}", ch_idx, combo));
                assert_eq!(seg.bold, exp_b, "Bold mismatch on char {} for combo OFF {:?}. Content: {:?}", ch_idx, combo, doc.content);
                assert_eq!(seg.italic, exp_i, "Italic mismatch on char {} for combo OFF {:?}. Content: {:?}", ch_idx, combo, doc.content);
                assert_eq!(seg.strike, exp_s, "Strike mismatch on char {} for combo OFF {:?}. Content: {:?}", ch_idx, combo, doc.content);
                assert_eq!(seg.code, exp_c, "Code mismatch on char {} for combo OFF {:?}. Content: {:?}", ch_idx, combo, doc.content);
            }

            // 'y' must be unstyled
            let y_src = line.offset_map.display_to_source(2);
            let y_seg = line
                .styled_segments
                .iter()
                .find(|s| s.source.start <= y_src && y_src < s.source.end)
                .unwrap_or_else(|| panic!("No styled segment covering 'y' for combo {:?}", combo));
            assert!(!y_seg.bold && !y_seg.italic && !y_seg.strike && !y_seg.code,
                "Expected 'y' unstyled for combo {:?}, got B={} I={} S={} C={}",
                combo, y_seg.bold, y_seg.italic, y_seg.strike, y_seg.code);
        }

        // 3. With prefix "x ": "x ", arm combo, type "hi", toggle off, type "y"
        {
            let mut doc = DocumentTab::new("prefix_off".into(), "test.md".into(), String::new(), None, None);
            type_keystrokes(&mut doc, "x ");
            for &cmd in combo {
                doc.apply_format_in_mode(cmd, ViewMode::Wysiwyg);
            }
            type_keystrokes(&mut doc, "hi");
            for &cmd in combo {
                doc.apply_format_in_mode(cmd, ViewMode::Wysiwyg);
            }
            type_keystrokes(&mut doc, "y");

            let parsed = crate::markdown::parse_document(&doc.content);
            let line = parsed.lines.first().expect("line not found");
            assert_eq!(
                line.display_text, "x hiy",
                "Leaked delimiters in display text when toggling off with prefix combo {:?}. Content was: {:?}",
                combo, doc.content
            );

            // 'x' and ' ' must be unstyled
            for ch_idx in 0..2 {
                let src = line.offset_map.display_to_source(ch_idx);
                let seg = line
                    .styled_segments
                    .iter()
                    .find(|s| s.source.start <= src && src < s.source.end)
                    .unwrap_or_else(|| panic!("No styled segment covering char {} for combo {:?}", ch_idx, combo));
                assert!(!seg.bold && !seg.italic && !seg.strike && !seg.code);
            }

            // 'h' and 'i' must have the combo formats
            for ch_idx in 2..4 {
                let src = line.offset_map.display_to_source(ch_idx);
                let seg = line
                    .styled_segments
                    .iter()
                    .find(|s| s.source.start <= src && src < s.source.end)
                    .unwrap_or_else(|| panic!("No styled segment covering char {} for combo {:?}", ch_idx, combo));
                assert_eq!(seg.bold, exp_b, "Bold mismatch on char {} for combo OFF {:?}. Content: {:?}", ch_idx, combo, doc.content);
                assert_eq!(seg.italic, exp_i, "Italic mismatch on char {} for combo OFF {:?}. Content: {:?}", ch_idx, combo, doc.content);
                assert_eq!(seg.strike, exp_s, "Strike mismatch on char {} for combo OFF {:?}. Content: {:?}", ch_idx, combo, doc.content);
                assert_eq!(seg.code, exp_c, "Code mismatch on char {} for combo OFF {:?}. Content: {:?}", ch_idx, combo, doc.content);
            }

            // 'y' must be unstyled
            let y_src = line.offset_map.display_to_source(4);
            let y_seg = line
                .styled_segments
                .iter()
                .find(|s| s.source.start <= y_src && y_src < s.source.end)
                .unwrap_or_else(|| panic!("No styled segment covering 'y' for combo {:?}", combo));
            assert!(!y_seg.bold && !y_seg.italic && !y_seg.strike && !y_seg.code,
                "Expected 'y' unstyled for combo {:?}, got B={} I={} S={} C={}",
                combo, y_seg.bold, y_seg.italic, y_seg.strike, y_seg.code);
        }
    }
}

#[test]
fn test_whitespace_only_inserts_with_each_format_armed() {
    let whitespaces = [" ", "\t", "   ", "  \t  "];
    let formats = [
        ("bold", true, false, false, false, false, "**word**"),
        ("italic", false, true, false, false, false, "*word*"),
        ("underline", false, false, true, false, false, "<u>word</u>"),
        ("strike", false, false, false, true, false, "~~word~~"),
        ("code", false, false, false, false, true, "`word`"),
    ];

    for ws in whitespaces {
        for (fmt, b, i, u, s, c, expected_span) in formats {
            // 1. In empty doc: verify plain insertion & pending formats preserved
            let mut doc =
                DocumentTab::new("ws".into(), "test.md".into(), String::new(), None, None);
            doc.apply_format_in_mode(fmt, ViewMode::Wysiwyg);
            assert!(
                doc.has_pending_inline_formats(),
                "Format {} should be armed before inserting {:?}",
                fmt,
                ws
            );

            doc.insert_text_in_mode(0..0, ws, ViewMode::Wysiwyg);
            assert_eq!(
                doc.content, ws,
                "Whitespace {:?} with format {} should be inserted plainly",
                ws, fmt
            );
            assert!(
                doc.has_pending_inline_formats(),
                "Format {} should remain armed after inserting whitespace {:?}",
                fmt,
                ws
            );

            type_keystrokes(&mut doc, "word");
            assert_eq!(
                doc.content,
                format!("{}{}", ws, expected_span),
                "Typed text should wrap with armed format delimiters"
            );

            // 2. In paragraph (with prefix text): verify parse_document styled segments
            let mut doc2 =
                DocumentTab::new("ws2".into(), "test.md".into(), "intro".into(), None, None);
            doc2.caret.move_to(5);
            doc2.apply_format_in_mode(fmt, ViewMode::Wysiwyg);
            doc2.insert_text_in_mode(5..5, ws, ViewMode::Wysiwyg);
            assert!(doc2.has_pending_inline_formats());
            type_keystrokes(&mut doc2, "word");

            let parsed = crate::markdown::parse_document(&doc2.content);
            let line = parsed.lines.first().unwrap();
            let seg = line
                .styled_segments
                .iter()
                .find(|seg| &doc2.content[seg.source.clone()] == "word")
                .unwrap_or_else(|| {
                    panic!(
                        "Segment 'word' not found for format {} with whitespace {:?}, content: {:?}",
                        fmt, ws, doc2.content
                    )
                });
            assert_eq!(seg.bold, b, "{}: bold mismatch", fmt);
            assert_eq!(seg.italic, i, "{}: italic mismatch", fmt);
            assert_eq!(seg.underline, u, "{}: underline mismatch", fmt);
            assert_eq!(seg.strike, s, "{}: strike mismatch", fmt);
            assert_eq!(seg.code, c, "{}: code mismatch", fmt);
        }
    }
}

#[test]
fn test_whitespace_only_inserts_in_table_cell_with_each_format_armed() {
    let whitespaces = [" ", "\t", "   ", "  \t  "];
    let formats = ["bold", "italic", "underline", "strike", "code"];

    for ws in whitespaces {
        for fmt in formats {
            let initial = "| x |\n| --- |\n";
            let mut doc = DocumentTab::new(
                "table_ws".into(),
                "test.md".into(),
                initial.to_string(),
                None,
                None,
            );
            // Position caret right after 'x' at index 3 in "| x |"
            doc.caret.move_to(3);
            doc.apply_format_in_mode(fmt, ViewMode::Wysiwyg);
            assert!(
                doc.has_pending_inline_formats(),
                "Format {} should be armed in table cell",
                fmt
            );

            // Inserting whitespace-only text must not panic
            doc.insert_text_in_mode(3..3, ws, ViewMode::Wysiwyg);
            assert!(
                doc.has_pending_inline_formats(),
                "Format {} should remain armed after whitespace insert in table cell",
                fmt
            );
            assert_eq!(
                &doc.content[..3 + ws.len()],
                format!("| x{}", ws),
                "Whitespace must be inserted plainly inside table cell"
            );

            // Now type text and ensure it receives the armed format
            type_keystrokes(&mut doc, "word");
            let parsed = crate::markdown::parse_document(&doc.content);
            let line = parsed.lines.first().unwrap();
            let seg = line
                .styled_segments
                .iter()
                .find(|seg| &doc.content[seg.source.clone()] == "word")
                .unwrap_or_else(|| {
                    panic!(
                        "Segment 'word' not found in table for format {} with ws {:?}, content: {:?}",
                        fmt, ws, doc.content
                    )
                });
            match fmt {
                "bold" => assert!(seg.bold),
                "italic" => assert!(seg.italic),
                "underline" => assert!(seg.underline),
                "strike" => assert!(seg.strike),
                "code" => assert!(seg.code),
                _ => unreachable!(),
            }
        }
    }
}

#[test]
fn test_no_empty_format_spans_after_toggling() {
    // Repro: x *~~hi~~*y
    let mut doc = DocumentTab::new("repro".into(), "test.md".into(), String::new(), None, None);
    type_keystrokes(&mut doc, "x ");
    doc.apply_format_in_mode("italic", ViewMode::Wysiwyg);
    doc.apply_format_in_mode("strike", ViewMode::Wysiwyg);
    type_keystrokes(&mut doc, "hi");
    doc.apply_format_in_mode("italic", ViewMode::Wysiwyg);
    doc.apply_format_in_mode("strike", ViewMode::Wysiwyg);
    type_keystrokes(&mut doc, "y");

    assert_eq!(doc.content, "x ~~*hi*~~y");
    assert!(!doc.content.contains("<u></u>"));
    assert!(!doc.content.contains("****"));
    assert!(!doc.content.contains("~~~~"));

    // Toggle on and immediately toggle off for each format
    let formats = ["bold", "italic", "underline", "strike", "code"];
    for fmt in formats {
        let mut doc = DocumentTab::new("empty_toggle".into(), "test.md".into(), String::new(), None, None);
        doc.apply_format_in_mode(fmt, ViewMode::Wysiwyg);
        doc.apply_format_in_mode(fmt, ViewMode::Wysiwyg);
        assert_eq!(doc.content, "");
        assert!(!doc.content.contains("<u></u>"));
        assert!(!doc.content.contains("****"));
        assert!(!doc.content.contains("~~~~"));
        assert!(!doc.has_pending_inline_formats());
    }

    // Type word, toggle off, type word for each format
    for fmt in formats {
        let mut doc = DocumentTab::new("toggle_seq".into(), "test.md".into(), String::new(), None, None);
        doc.apply_format_in_mode(fmt, ViewMode::Wysiwyg);
        type_keystrokes(&mut doc, "abc");
        doc.apply_format_in_mode(fmt, ViewMode::Wysiwyg);
        type_keystrokes(&mut doc, "def");

        assert!(!doc.content.contains("<u></u>"), "Format {} left <u></u>: {:?}", fmt, doc.content);
        assert!(!doc.content.contains("****"), "Format {} left ****: {:?}", fmt, doc.content);
        assert!(!doc.content.contains("~~~~"), "Format {} left ~~~~: {:?}", fmt, doc.content);
    }
}

const PERF_LINE: &str =
    "Some words **bold text** and *it* plus <u>under</u> ~~st~~ `c` [link](http://x) 😀 tiếng\n";

fn naive_span_close_end(parsed: &ParsedDocument, pair: &InlinePair) -> usize {
    parsed
        .all_inline_pairs()
        .filter(|p| p.kind != InlineTagKind::Link && p.inner == pair.inner)
        .map(|p| p.close_delim.end)
        .max()
        .unwrap_or(pair.close_delim.end)
}

fn naive_span_open_start(parsed: &ParsedDocument, pair: &InlinePair) -> usize {
    parsed
        .all_inline_pairs()
        .filter(|p| p.kind != InlineTagKind::Link && p.inner == pair.inner)
        .map(|p| p.open_delim.start)
        .min()
        .unwrap_or(pair.open_delim.start)
}

fn naive_collect_group_pairs(
    parsed: &ParsedDocument,
    range: Range<usize>,
) -> Vec<&InlinePair> {
    let mut group_pairs: Vec<&InlinePair> = parsed
        .all_inline_pairs()
        .filter(|p| {
            p.kind != InlineTagKind::Link
                && p.open_delim.start < range.end
                && naive_span_close_end(parsed, p) > range.start
        })
        .collect();
    let mut expanded = true;
    while expanded {
        expanded = false;
        for p in parsed.all_inline_pairs() {
            if p.kind == InlineTagKind::Link
                || group_pairs.iter().any(|g| std::ptr::eq(*g, p))
            {
                continue;
            }
            let touches = group_pairs.iter().any(|g| {
                p.open_delim.start < naive_span_close_end(parsed, g)
                    && naive_span_close_end(parsed, p) > g.open_delim.start
            });
            if touches {
                group_pairs.push(p);
                expanded = true;
            }
        }
    }
    group_pairs
}

fn pair_key(
    p: &InlinePair,
) -> (
    InlineTagKind,
    usize,
    usize,
    usize,
    usize,
    usize,
    usize,
) {
    (
        p.kind,
        p.open_delim.start,
        p.open_delim.end,
        p.inner.start,
        p.inner.end,
        p.close_delim.start,
        p.close_delim.end,
    )
}

fn assert_helper_equivalence(parsed: &ParsedDocument) {
    for pair in parsed.all_inline_pairs() {
        assert_eq!(
            span_close_end(parsed, pair),
            naive_span_close_end(parsed, pair),
            "group_close_end mismatch for {pair:?}"
        );
        assert_eq!(
            span_open_start(parsed, pair),
            naive_span_open_start(parsed, pair),
            "group_open_start mismatch for {pair:?}"
        );
        assert_eq!(pair.group_close_end, naive_span_close_end(parsed, pair));
        assert_eq!(pair.group_open_start, naive_span_open_start(parsed, pair));
    }
    let n = parsed.all_inline_pairs().count().max(1);
    let step = (parsed.lines.last().map(|l| l.source.end).unwrap_or(0) / n.max(1)).max(1);
    for start in (0..=parsed.lines.last().map(|l| l.source.end).unwrap_or(0)).step_by(step.max(7)) {
        let end = (start + 11).min(parsed.lines.last().map(|l| l.source.end).unwrap_or(start));
        let mut naive: Vec<_> = naive_collect_group_pairs(parsed, start..end)
            .into_iter()
            .map(pair_key)
            .collect();
        let mut fast: Vec<_> = collect_group_pairs(parsed, start..end)
            .into_iter()
            .map(pair_key)
            .collect();
        naive.sort();
        fast.sort();
        assert_eq!(naive, fast, "group_pairs mismatch at {start}..{end}");
    }
}

fn mixed_perf_doc(n: usize) -> String {
    let mut s = String::with_capacity(n * PERF_LINE.len() + 200);
    s.push_str("# Title\n\n");
    s.push_str("- list **item**\n\n");
    s.push_str("```\ncode\n```\n\n");
    s.push_str("| a | b |\n| --- | --- |\n| **c** | d |\n\n");
    for _ in 0..n {
        s.push_str(PERF_LINE);
    }
    s
}

#[test]
fn precomputed_span_bounds_match_naive_scan() {
    let content = mixed_perf_doc(8);
    let parsed = parse_document(&content);
    assert_helper_equivalence(&parsed);
    let mid = content.len() / 2;
    assert_eq!(
        parsed.line_at_offset(mid).map(|l| l.line_index),
        parsed
            .lines
            .iter()
            .find(|l| mid >= l.source.start && mid <= l.source.end)
            .map(|l| l.line_index)
    );
}

fn snapshot(doc: &DocumentTab) -> (String, Range<usize>, ActiveFormats, ActiveFormats) {
    (
        doc.content.clone(),
        doc.caret.range.clone(),
        doc.active_formats,
        doc.pending_formats,
    )
}

fn run_scripted_format_sequence(n: usize) -> Vec<(String, Range<usize>, ActiveFormats, ActiveFormats)> {
    let mut doc = DocumentTab::new(
        "perf".into(),
        "perf.md".into(),
        mixed_perf_doc(n),
        None,
        None,
    );
    let mut out = vec![snapshot(&doc)];
    let mid = doc.content.len() / 2;
    doc.place(mid, false);
    out.push(snapshot(&doc));
    assert_helper_equivalence(&doc.parsed());

    type_keystrokes(&mut doc, "Z");
    out.push(snapshot(&doc));
    assert_helper_equivalence(&doc.parsed());

    for cmd in ["bold", "italic", "underline", "strike", "code", "link"] {
        doc.apply_format_in_mode(cmd, ViewMode::Wysiwyg);
        out.push(snapshot(&doc));
        type_keystrokes(&mut doc, "a");
        out.push(snapshot(&doc));
        doc.apply_format_in_mode(cmd, ViewMode::Wysiwyg);
        out.push(snapshot(&doc));
        assert_helper_equivalence(&doc.parsed());
    }

    let start = doc.cursor_offset().saturating_sub(8);
    let end = (doc.cursor_offset() + 8).min(doc.content.len());
    doc.caret.range = start..end;
    doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
    out.push(snapshot(&doc));
    doc.apply_format_in_mode("italic", ViewMode::Wysiwyg);
    out.push(snapshot(&doc));

    doc.smart_newline(false);
    out.push(snapshot(&doc));
    doc.smart_newline(true);
    out.push(snapshot(&doc));
    doc.smart_backspace_in_mode(ViewMode::Wysiwyg);
    out.push(snapshot(&doc));
    doc.delete_forward_in_mode(ViewMode::Wysiwyg);
    out.push(snapshot(&doc));
    assert!(doc.undo());
    out.push(snapshot(&doc));
    assert!(doc.redo());
    out.push(snapshot(&doc));
    assert!(doc.undo());
    out.push(snapshot(&doc));
    assert_helper_equivalence(&doc.parsed());
    out
}

#[test]
fn format_toggle_helpers_match_naive_through_scripted_edits() {
    let a = run_scripted_format_sequence(12);
    let b = run_scripted_format_sequence(12);
    assert_eq!(a, b);
    assert!(a.len() > 10);
}

#[test]
fn format_toggle_perf_bound() {
    let n = 80;
    let content = mixed_perf_doc(n);
    let mut doc = DocumentTab::new("p".into(), "p.md".into(), content, None, None);
    let mid = doc.content.len() / 2;
    doc.place(mid, false);
    let start = std::time::Instant::now();
    doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
    let range = doc.selection_range();
    doc.insert_text_in_mode(range, "x", ViewMode::Wysiwyg);
    let elapsed = start.elapsed();
    assert!(
        elapsed.as_millis() < 1500,
        "Ctrl+B + type took {:?} on n={n} (generous debug bound 1500ms)",
        elapsed
    );
}

#[test]
fn format_toggle_selection_is_deterministic() {
    let content = mixed_perf_doc(24);
    let mut end = content.len() / 3;
    while end > 0 && !content.is_char_boundary(end) {
        end -= 1;
    }
    let apply = || {
        let mut doc = DocumentTab::new("p".into(), "p.md".into(), content.clone(), None, None);
        doc.caret.range = 0..end;
        doc.apply_format_in_mode("italic", ViewMode::Wysiwyg);
        snapshot(&doc)
    };
    assert_eq!(apply(), apply());
}

#[test]
#[ignore]
fn format_toggle_perf_bench() {
    let builder = std::thread::Builder::new().stack_size(32 * 1024 * 1024);
    let handle = builder
        .spawn(|| {
            fn median_ms(mut v: Vec<u128>) -> f64 {
                v.sort();
                let n = v.len();
                if n % 2 == 1 {
                    v[n / 2] as f64
                } else {
                    (v[n / 2 - 1] + v[n / 2]) as f64 / 2.0
                }
            }
            for n in [100usize, 1000, 5000] {
                let content = PERF_LINE.repeat(n);
                let parse_ms = {
                    let mut times = Vec::new();
                    for _ in 0..5 {
                        let t = std::time::Instant::now();
                        let _ = parse_document(&content);
                        times.push(t.elapsed().as_micros());
                    }
                    median_ms(times) / 1000.0
                };
                let mut type_times = Vec::new();
                let mut toggle_times = Vec::new();
                let mut undo_times = Vec::new();
                let mut sel_times = Vec::new();
                for _ in 0..5 {
                    let mut doc = DocumentTab::new(
                        "b".into(),
                        "b.md".into(),
                        content.clone(),
                        None,
                        None,
                    );
                    let mid = doc.content.len() / 2;
                    doc.place(mid, false);
                    let _ = doc.parsed();
                    let t = std::time::Instant::now();
                    let range = doc.selection_range();
                    doc.insert_text_in_mode(range, "x", ViewMode::Wysiwyg);
                    type_times.push(t.elapsed().as_micros());

                    let mut doc = DocumentTab::new(
                        "b".into(),
                        "b.md".into(),
                        content.clone(),
                        None,
                        None,
                    );
                    let mid = doc.content.len() / 2;
                    doc.place(mid, false);
                    let _ = doc.parsed();
                    let t = std::time::Instant::now();
                    doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
                    let range = doc.selection_range();
                    doc.insert_text_in_mode(range, "x", ViewMode::Wysiwyg);
                    toggle_times.push(t.elapsed().as_micros());
                    let t = std::time::Instant::now();
                    doc.undo();
                    undo_times.push(t.elapsed().as_micros());

                    let mut doc = DocumentTab::new(
                        "b".into(),
                        "b.md".into(),
                        content.clone(),
                        None,
                        None,
                    );
                    let mut end = content.len() / 3;
                    while end > 0 && !content.is_char_boundary(end) {
                        end -= 1;
                    }
                    doc.caret.range = 0..end;
                    let _ = doc.parsed();
                    let t = std::time::Instant::now();
                    doc.apply_format_in_mode("italic", ViewMode::Wysiwyg);
                    sel_times.push(t.elapsed().as_micros());
                }
                eprintln!(
                    "n={n} parse={:.2}ms type={:.2}ms ctrlb_type={:.2}ms undo={:.2}ms sel_italic={:.2}ms",
                    parse_ms,
                    median_ms(type_times) / 1000.0,
                    median_ms(toggle_times) / 1000.0,
                    median_ms(undo_times) / 1000.0,
                    median_ms(sel_times) / 1000.0
                );
            }
        })
        .unwrap();
    handle.join().unwrap();
}

#[test]
fn compute_stats_is_lazy_until_ensure_stats() {
    let mut doc = DocumentTab::new(
        "s".into(),
        "s.md".into(),
        "one two".into(),
        None,
        None,
    );
    assert_eq!(doc.word_count, 2);
    doc.insert_text_in_mode(doc.content.len()..doc.content.len(), " three four", ViewMode::Raw);
    let before = compute_stats(&doc.content);
    assert_eq!(
        doc.word_count, 2,
        "stats must stay stale until the status bar reads them"
    );
    doc.ensure_stats();
    assert_eq!(doc.word_count, before.words);
    assert_eq!(doc.char_count, before.chars);
    assert_eq!(doc.line_count, before.lines);
    assert_eq!(doc.reading_time_min, before.reading_time_min);
    doc.ensure_stats();
    assert_eq!(doc.word_count, before.words);
}
