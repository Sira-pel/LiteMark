use super::*;

const M: ViewMode = ViewMode::Wysiwyg;

fn clear(src: &str, sel: Range<usize>) -> (String, DocumentTab) {
    let mut d = DocumentTab::new("t".into(), "t.md".into(), src.to_string(), None, None);
    d.set_view_mode(M);
    d.caret.range = sel;
    d.apply_format_in_mode("clear", M);
    (d.content.clone(), d)
}

const S: &str = "Hello **bold** and *it* and <u>under</u> ~~st~~ `c` end";

#[test]
fn select_all_removes_every_format() {
    assert_eq!(clear(S, 0..S.len()).0, "Hello bold and it and under st c end");
}

#[test]
fn selecting_only_the_visible_word_clears_just_that_word() {
    let a = S.find("bold").unwrap();
    assert_eq!(
        clear(S, a..a + 4).0,
        "Hello bold and *it* and <u>under</u> ~~st~~ `c` end"
    );
}

#[test]
fn caret_inside_a_formatted_word_clears_that_word() {
    let a = S.find("bold").unwrap();
    assert_eq!(
        clear(S, a + 2..a + 2).0,
        "Hello bold and *it* and <u>under</u> ~~st~~ `c` end"
    );
}

#[test]
fn selection_ending_inside_a_span_splits_it_without_stray_markers() {
    let a = S.find("bold").unwrap();
    assert_eq!(
        clear(S, 0..a + 2).0,
        "Hello bo**ld** and *it* and <u>under</u> ~~st~~ `c` end"
    );
}

#[test]
fn stacked_formats_on_one_word_are_all_removed() {
    let src = "a <u>***word***</u> b";
    let at = src.find("word").unwrap();
    assert_eq!(clear(src, at..at + 4).0, "a word b");
}

#[test]
fn works_inside_headings_and_lists_and_keeps_the_block_markers() {
    let h = "# **Welcome** to <u>LiteMark</u>";
    assert_eq!(clear(h, 4..11).0, "# Welcome to <u>LiteMark</u>");
    assert_eq!(clear(h, 0..h.len()).0, "# Welcome to LiteMark");
    let l = "- **item** one\\n- *two*";
    assert_eq!(clear(l, 0..l.len()).0, "- item one\\n- two");
}

#[test]
fn plain_text_and_empty_spots_are_left_alone() {
    assert_eq!(clear("plain text", 0..5).0, "plain text");
    assert_eq!(clear("a  b", 2..2).0, "a  b");
}

#[test]
fn clearing_is_one_undo_step() {
    let (cleared, mut d) = clear(S, 0..S.len());
    assert_ne!(cleared, S);
    assert!(d.undo());
    assert_eq!(d.content, S);
}

#[test]
fn code_files_are_detected_by_extension_and_name() {
    for name in [
        "main.rs",
        "a.PY",
        "data.json",
        "Cargo.toml",
        "x.tsx",
        "build.mk",
    ] {
        assert_eq!(FileType::from_filename(name), FileType::Code, "{name}");
    }
    for name in [
        "Makefile",
        "dockerfile",
        "GNUmakefile",
        ".env",
        ".gitignore",
    ] {
        assert_eq!(FileType::from_filename(name), FileType::Code, "{name}");
    }
    assert_eq!(FileType::from_filename("notes.md"), FileType::Markdown);
    assert_eq!(FileType::from_filename("scan.pdf"), FileType::Pdf);
    assert_eq!(FileType::from_filename("notes.txt"), FileType::PlainText);
    assert_eq!(FileType::from_filename("photo.xyz"), FileType::PlainText);
    assert_eq!(FileType::from_filename("rs"), FileType::PlainText);
    assert_eq!(FileType::from_filename(".rs"), FileType::PlainText);
}

#[test]
fn zero_byte_means_binary() {
    assert!(looks_binary(b"ab\0cd"));
    assert!(!looks_binary(b"fn main() {}\n"));
    assert!(!looks_binary("héllo".as_bytes()));
}

#[test]
fn code_and_plain_text_are_always_raw() {
    for global in [ViewMode::Raw, ViewMode::Wysiwyg] {
        assert_eq!(FileType::Code.effective_view_mode(global), ViewMode::Raw);
        assert_eq!(
            FileType::PlainText.effective_view_mode(global),
            ViewMode::Raw
        );
        assert_eq!(FileType::Markdown.effective_view_mode(global), global);
    }
}

/// Ctrl+Shift+C on indented code makes one backtick fence from the whole
/// block (blank lines between chunks included) and drops the four-column
/// indent; deeper indent and the file's line endings are kept.
#[test]
fn code_block_toggle_turns_indented_code_into_a_fence() {
    let cases = [
        (
            "para\n\n    a\n\n    b\n",
            13,
            "para\n\n```\na\n\nb\n```\n",
            "a\n\nb",
        ),
        (
            "    foo\n        bar",
            2,
            "```\nfoo\n    bar\n```",
            "foo\n    bar",
        ),
        ("    a\r\n    b", 5, "```\r\na\r\nb\r\n```", "a\r\nb"),
        ("\tx\n", 1, "```\nx\n```\n", "x"),
        // A ``` line in the code needs a longer fence.
        (
            "    a\n    ```\n    b",
            5,
            "````\na\n```\nb\n````",
            "a\n```\nb",
        ),
        // Two blank lines between chunks are still one block.
        ("    a\n\n\n    b\n", 5, "```\na\n\n\nb\n```\n", "a\n\n\nb"),
    ];
    for (source, caret, expected, selected) in cases {
        let mut doc = DocumentTab::new("c".into(), "c.md".into(), source.into(), None, None);
        doc.place(caret, false);
        doc.apply_format("codeblock");
        assert_eq!(doc.content, expected, "{source:?}");
        assert_eq!(
            &doc.content[doc.caret.range.clone()],
            selected,
            "{source:?}"
        );
    }
}

/// Enter on the whitespace-only line a previous Enter left under indented
/// code clears it and ends the block. Elsewhere it is a plain newline.
#[test]
fn second_enter_ends_an_indented_code_block() {
    let mut doc = DocumentTab::new(
        "c".into(),
        "c.md".into(),
        "para\n\n    code".into(),
        None,
        None,
    );
    doc.place(doc.content.len(), false);
    doc.smart_newline(false);
    assert_eq!(doc.content, "para\n\n    code\n    ");
    doc.smart_newline(false);
    assert_eq!(doc.content, "para\n\n    code\n");
    assert_eq!(doc.cursor_offset(), doc.content.len());

    let mut text = DocumentTab::new("t".into(), "t.md".into(), "foo\n    ".into(), None, None);
    text.place(text.content.len(), false);
    text.smart_newline(false);
    assert_eq!(text.content, "foo\n    \n");
}

#[test]
fn enter_in_code_keeps_indent_and_never_continues_a_list() {
    let mut doc = DocumentTab::new("s".into(), "a.rs".into(), "    - item".into(), None, None);
    doc.caret.move_to(doc.content.len());
    doc.refresh_cursor();
    doc.smart_newline(false);
    assert_eq!(doc.content, "    - item\n    ");
}

#[test]
fn backspace_in_code_skips_markdown_list_logic() {
    let mut doc = DocumentTab::new("s".into(), "a.rs".into(), "- ".into(), None, None);
    doc.caret.move_to(2);
    doc.refresh_cursor();
    doc.smart_backspace();
    assert_eq!(doc.content, "-");
}

#[test]
fn format_commands_do_nothing_in_code() {
    let mut doc = DocumentTab::new("s".into(), "a.py".into(), "word".into(), None, None);
    doc.caret.range = 0..4;
    doc.apply_format_in_mode("bold", ViewMode::Raw);
    assert_eq!(doc.content, "word");
}

#[test]
fn set_file_type_clears_the_parse_cache() {
    let mut doc = DocumentTab::new("s".into(), "a.md".into(), "# Hi".into(), None, None);
    doc.parsed();
    assert!(doc.cached_parse.is_some());
    assert!(!doc.set_file_type(FileType::Markdown));
    assert!(doc.cached_parse.is_some());
    assert!(doc.set_file_type(FileType::Code));
    assert!(doc.cached_parse.is_none());
    assert_eq!(doc.file_type, FileType::Code);
}

// --- Markdown indentation behaviour that code-file support must not change ---

#[test]
fn markdown_indent_adds_two_spaces_at_line_start_wherever_the_caret_is() {
    let mut doc = DocumentTab::new("m".into(), "a.md".into(), "hello world".into(), None, None);
    doc.caret.move_to(5);
    doc.indent();
    assert_eq!(doc.content, "  hello world");
    assert_eq!(doc.caret.cursor(), 7);
}

#[test]
fn markdown_indent_touches_blank_lines_in_a_selection() {
    let mut doc = DocumentTab::new("m".into(), "a.md".into(), "a\n\nb".into(), None, None);
    doc.caret.range = 0..doc.content.len();
    doc.indent();
    assert_eq!(doc.content, "  a\n  \n  b");
}

#[test]
fn markdown_outdent_strips_two_spaces_one_space_or_a_tab() {
    let mut doc = DocumentTab::new(
        "m".into(),
        "a.md".into(),
        "    a\n b\n\tc".into(),
        None,
        None,
    );
    doc.caret.range = 0..doc.content.len();
    doc.outdent();
    assert_eq!(doc.content, "  a\nb\nc");
}

#[test]
fn markdown_backspace_in_indentation_deletes_two_spaces() {
    let mut doc = DocumentTab::new("m".into(), "a.md".into(), "      text".into(), None, None);
    doc.caret.move_to(6);
    doc.smart_backspace();
    assert_eq!(doc.content, "    text");
    // An odd run still removes two, as before.
    let mut odd = DocumentTab::new("m".into(), "a.md".into(), "   text".into(), None, None);
    odd.caret.move_to(3);
    odd.smart_backspace();
    assert_eq!(odd.content, " text");
}

// --- Code-file indentation and editing ---

fn code_doc(name: &str, text: &str) -> DocumentTab {
    DocumentTab::new("c".into(), name.into(), text.to_string(), None, None)
}

#[test]
fn table_commands_do_nothing_in_code_tabs() {
    let text = "| a | b |\n|---|---|\n| 1 | 2 |\n";
    let mut doc = code_doc("a.rs", text);
    doc.caret.move_to(2);
    assert!(!doc.tab_table_cell(false));
    assert!(!doc.table_insert_row_below());
    assert!(!doc.apply_table_op_at(2, TableOp::InsertColRight));
    doc.insert_table_with_size(2, 2);
    assert_eq!(doc.content, text);
}

#[test]
fn code_tabs_track_the_cursor_without_markdown_state() {
    let mut doc = code_doc("a.py", "x = 1\n# hi\n");
    doc.caret.move_to(8);
    doc.refresh_cursor();
    assert_eq!((doc.cursor_line, doc.cursor_col), (2, 3));
    assert!(!doc.active_formats.h1);
    assert!(doc.cached_parse.is_none());
}

#[test]
fn undo_in_a_code_tab_does_not_parse_markdown() {
    let mut doc = code_doc("a.py", "x\n");
    doc.caret.move_to(1);
    doc.edit(1..1, "y");
    doc.release_parse_cache();
    assert!(doc.undo());
    assert!(doc.cached_parse.is_none());
}

#[test]
fn caret_tab_after_a_real_tab_uses_display_columns() {
    let mut doc = code_doc("a.c", "\tx");
    doc.indent_style = IndentStyle::Spaces(3);
    doc.caret.move_to(2);
    doc.indent();
    // The tab shows as 4 columns and x as 1, so one space reaches the stop at 6.
    assert_eq!(doc.content, "\tx ");
}

#[test]
fn plain_text_indents_four_spaces() {
    assert_eq!(code_doc("n.txt", "").indent_style, IndentStyle::Spaces(4));
}

#[test]
fn set_file_type_clears_pending_formats() {
    let mut doc = code_doc("a.md", "");
    doc.pending_formats.bold = true;
    assert!(doc.set_file_type(FileType::Code));
    assert!(!doc.pending_formats.bold);
}

#[test]
fn crlf_content_round_trips_unchanged() {
    let text = "fn main() {\r\n    a();\r\n}\r\n";
    let doc = code_doc("a.rs", text);
    assert_eq!(doc.content, text);
    assert_eq!(doc.content.as_bytes(), text.as_bytes());
}

#[test]
fn indent_style_defaults_by_file() {
    assert_eq!(code_doc("Makefile", "").indent_style, IndentStyle::Tabs);
    assert_eq!(code_doc("GNUmakefile", "").indent_style, IndentStyle::Tabs);
    assert_eq!(code_doc("rules.mk", "").indent_style, IndentStyle::Tabs);
    assert_eq!(code_doc("main.go", "").indent_style, IndentStyle::Tabs);
    assert_eq!(code_doc("main.rs", "").indent_style, IndentStyle::Spaces(4));
    assert_eq!(
        code_doc("notes.md", "").indent_style,
        IndentStyle::Spaces(2)
    );
    assert_eq!(
        code_doc("notes.txt", "").indent_style,
        IndentStyle::Spaces(4)
    );
}

#[test]
fn indent_style_is_detected_from_existing_lines() {
    let two = "function f() {\n  if (a) {\n    b();\n  }\n}\n";
    assert_eq!(code_doc("a.js", two).indent_style, IndentStyle::Spaces(2));
    let tabs = "a {\n\tb;\n\tc;\n}\n";
    assert_eq!(code_doc("a.c", tabs).indent_style, IndentStyle::Tabs);
    let mixed = "a {\n\tb;\n    c;\n}\n";
    assert_eq!(code_doc("a.js", mixed).indent_style, IndentStyle::Spaces(4));
    assert_eq!(code_doc("a.go", mixed).indent_style, IndentStyle::Tabs);
    // Markdown keeps its default whatever the content looks like.
    assert_eq!(code_doc("a.md", tabs).indent_style, IndentStyle::Spaces(2));
}

#[test]
fn tab_inserts_up_to_the_next_stop_in_code() {
    let mut doc = code_doc("a.rs", "ab");
    doc.caret.move_to(2);
    doc.indent();
    assert_eq!(doc.content, "ab  ");
    assert_eq!(doc.caret.cursor(), 4);
    doc.indent();
    assert_eq!(doc.content, "ab      ");
    let mut go = code_doc("a.go", "x");
    go.caret.move_to(1);
    go.indent();
    assert_eq!(go.content, "x\t");
}

#[test]
fn multi_line_indent_skips_blank_lines_and_keeps_crlf() {
    let mut doc = code_doc("a.rs", "a\r\n\r\nb\r\nc");
    doc.caret.range = 0..doc.content.len();
    doc.indent();
    assert_eq!(doc.content, "    a\r\n\r\n    b\r\n    c");
    doc.outdent();
    assert_eq!(doc.content, "a\r\n\r\nb\r\nc");
}

#[test]
fn selection_ending_at_a_line_start_leaves_that_line_alone() {
    let mut doc = code_doc("a.rs", "a\nb\nc");
    doc.caret.range = 0..4; // "a\nb\n", ends at the start of "c"
    doc.indent();
    assert_eq!(doc.content, "    a\n    b\nc");
    doc.outdent();
    assert_eq!(doc.content, "a\nb\nc");
}

#[test]
fn tabs_style_indents_and_outdents_with_real_tabs() {
    let mut doc = code_doc("Makefile", "a\nb");
    doc.caret.range = 0..doc.content.len();
    doc.indent();
    assert_eq!(doc.content, "\ta\n\tb");
    doc.outdent();
    assert_eq!(doc.content, "a\nb");
}

#[test]
fn outdent_strips_at_most_one_level_of_spaces() {
    let mut doc = code_doc("a.rs", "        a\n  b");
    doc.indent_style = IndentStyle::Spaces(4);
    doc.caret.range = 0..doc.content.len();
    doc.outdent();
    assert_eq!(doc.content, "    a\nb");
}

#[test]
fn multi_line_indent_is_one_undo_step() {
    let mut doc = code_doc("a.rs", "a\nb\nc");
    doc.caret.range = 0..doc.content.len();
    doc.indent();
    assert!(doc.undo());
    assert_eq!(doc.content, "a\nb\nc");
    assert!(!doc.undo());
}

#[test]
fn code_newline_keeps_indent_and_adds_a_level_after_openers() {
    let mut doc = code_doc("a.rs", "    foo(");
    doc.caret.move_to(8);
    doc.smart_newline(false);
    assert_eq!(doc.content, "    foo(\n        ");
    let mut plain = code_doc("a.rs", "  x = 1");
    plain.caret.move_to(7);
    plain.smart_newline(false);
    assert_eq!(plain.content, "  x = 1\n  ");
    let mut tabs = code_doc("a.go", "\tif x {");
    tabs.caret.move_to(7);
    tabs.smart_newline(false);
    assert_eq!(tabs.content, "\tif x {\n\t\t");
}

#[test]
fn code_newline_does_not_continue_markdown_lists() {
    let mut doc = code_doc("a.rs", "- item");
    doc.caret.move_to(6);
    doc.smart_newline(false);
    assert_eq!(doc.content, "- item\n");
}

#[test]
fn colon_indents_only_in_python() {
    let mut py = code_doc("a.py", "if x:");
    py.caret.move_to(5);
    py.smart_newline(false);
    assert_eq!(py.content, "if x:\n    ");
    let mut rs = code_doc("a.rs", "x:");
    rs.caret.move_to(2);
    rs.smart_newline(false);
    assert_eq!(rs.content, "x:\n");
}

#[test]
fn enter_between_a_pair_opens_a_block_in_one_undo_step() {
    let mut doc = code_doc("a.rs", "fn f() {}");
    doc.caret.move_to(8);
    doc.smart_newline(false);
    assert_eq!(doc.content, "fn f() {\n    \n}");
    assert_eq!(doc.caret.cursor(), 13);
    assert!(doc.undo());
    assert_eq!(doc.content, "fn f() {}");
    assert!(doc.redo());
    assert_eq!(doc.caret.cursor(), 13);
    let mut arr = code_doc("a.js", "  x = []");
    arr.caret.move_to(7);
    arr.smart_newline(false);
    assert_eq!(arr.content, "  x = [\n    \n  ]");
}

#[test]
fn code_newline_uses_crlf_in_crlf_files() {
    let mut doc = code_doc("a.rs", "fn f() {}\r\nx");
    doc.caret.move_to(8);
    doc.smart_newline(false);
    assert_eq!(doc.content, "fn f() {\r\n    \r\n}\r\nx");
    let mut plain = code_doc("a.rs", "a\r\nb");
    plain.caret.move_to(1);
    plain.smart_newline(false);
    assert_eq!(plain.content, "a\r\n\r\nb");
}

#[test]
fn backspace_in_leading_spaces_goes_to_the_previous_stop() {
    let mut doc = code_doc("a.rs", "      x");
    doc.caret.move_to(6);
    doc.smart_backspace();
    assert_eq!(doc.content, "    x");
    doc.smart_backspace();
    assert_eq!(doc.content, "x");
    let mut tabs = code_doc("a.go", "\t\tx");
    tabs.caret.move_to(2);
    tabs.smart_backspace();
    assert_eq!(tabs.content, "\tx");
    // Not in leading whitespace: one character.
    let mut mid = code_doc("a.rs", "a    b");
    mid.caret.move_to(5);
    mid.smart_backspace();
    assert_eq!(mid.content, "a   b");
}

#[test]
fn backspace_deletes_only_code_pairs() {
    let mut doc = code_doc("a.rs", "{}");
    doc.caret.move_to(1);
    doc.smart_backspace();
    assert_eq!(doc.content, "");
    let mut quote = code_doc("a.rs", "''");
    quote.caret.move_to(1);
    quote.smart_backspace();
    assert_eq!(quote.content, "'");
}

#[test]
fn one_odd_indented_line_does_not_hide_the_detected_step() {
    let text = "a {\n  b;\n    c;\n   d;\n  e;\n    f;\n}\n";
    assert_eq!(code_doc("a.js", text).indent_style, IndentStyle::Spaces(2));
    let four = "a {\n    b;\n        c;\n   d;\n    e;\n}\n";
    assert_eq!(code_doc("a.js", four).indent_style, IndentStyle::Spaces(4));
}

#[test]
fn code_newline_follows_the_line_ending_where_it_splits() {
    // First line LF, later lines CRLF.
    let mut doc = code_doc("a.rs", "a\nb\r\nc\r\n");
    doc.caret.move_to(3);
    doc.smart_newline(false);
    assert_eq!(doc.content, "a\nb\r\n\r\nc\r\n");
    // Last line without an ending takes the previous line's.
    let mut tail = code_doc("a.rs", "a\nb\r\nc");
    tail.caret.move_to(tail.content.len());
    tail.smart_newline(false);
    assert_eq!(tail.content, "a\nb\r\nc\r\n");
}

/// Enter with the caret before a list or quote marker opens a line above
/// and leaves the marker alone.
#[test]
fn enter_at_line_start_of_list_or_quote_does_not_copy_the_marker() {
    for (text, want) in [
        ("- item", "\n- item"),
        ("1. one", "\n1. one"),
        ("- [ ] t", "\n- [ ] t"),
        ("> quote", "\n> quote"),
    ] {
        let mut doc = DocumentTab::new("t".into(), "t.md".into(), text.into(), None, None);
        doc.place(0, false);
        doc.smart_newline(false);
        assert_eq!(doc.content, want);
        assert_eq!(doc.cursor_offset(), 1);
    }
}

#[test]
fn delete_at_line_end_in_wysiwyg_drops_the_next_lines_marker() {
    for (text, want) in [
        ("- a\n- b", "- ab"),
        ("1. a\n2. b", "1. ab"),
        ("> a\n> b", "> ab"),
        ("a\n- b", "ab"),
        ("a\n\n- b", "a\n- b"),
    ] {
        let mut doc = DocumentTab::new("t".into(), "t.md".into(), text.into(), None, None);
        doc.place(text.find('\n').unwrap(), false);
        doc.delete_forward_in_mode(ViewMode::Wysiwyg);
        assert_eq!(doc.content, want, "{text:?}");
    }
    // Raw view edits the text literally.
    let mut doc = DocumentTab::new("t".into(), "t.md".into(), "- a\n- b".into(), None, None);
    doc.place(3, false);
    doc.delete_forward_in_mode(ViewMode::Raw);
    assert_eq!(doc.content, "- a- b");
}

#[test]
fn enter_before_heading_text_pushes_the_heading_down() {
    for (text, at) in [("# Head", 2), ("## Sub", 3), ("# Head", 0)] {
        let mut doc = DocumentTab::new("t".into(), "t.md".into(), text.into(), None, None);
        doc.place(at, false);
        doc.smart_newline(false);
        assert_eq!(doc.content, format!("\n{text}"));
        assert_eq!(doc.cursor_offset(), at + 1);
    }
    // Mid-heading and end-of-heading still split / continue as before.
    let mut doc = DocumentTab::new("t".into(), "t.md".into(), "# Head".into(), None, None);
    doc.place(4, false);
    doc.smart_newline(false);
    assert_eq!(doc.content, "# He\nad");
    let mut doc = DocumentTab::new("t".into(), "t.md".into(), "# Head".into(), None, None);
    doc.place(6, false);
    doc.smart_newline(false);
    assert_eq!(doc.content, "# Head\n");
}
