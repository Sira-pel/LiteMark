use super::*;

#[test]
fn plain_inline_shortcut_matches_pulldown() {
    let named = [
        "Plain prose, with commas; colons: and (parens).",
        "Smoke  test with  double spaces",
        "a 1. b",
        "word #tag and x = y",
        "a -- b - c + d",
        "see http://example.com/path?q=1 or www.example.com",
        "e.g. this \"quoted\" and 'single' text",
        "Ünïcödé text with ß and 😀 emoji",
        "tail ends with a period.",
        "a",
    ];
    let mut checked = 0usize;
    for text in named {
        assert!(is_plain_inline(text), "{text:?} should take the shortcut");
        for base in [0, 7] {
            assert_eq!(parse_inline_syntax(text, base), parse_inline_with_pulldown(text, base), "{text:?}");
        }
        checked += 1;
    }

    let alphabet: Vec<char> =
        "aZ 09.,;:-+#=()/?'\"$%^{}@é😀ßhtp:w1)\u{a0}\u{3000}\u{2003}\u{c}\u{b}".chars().collect();
    let mut seed = 0x2545_f491_4f6c_dd1du64;
    for _ in 0..200_000 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let len = 1 + (seed % 24) as usize;
        let mut text = String::new();
        let mut s = seed;
        for _ in 0..len {
            s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            text.push(alphabet[(s >> 33) as usize % alphabet.len()]);
        }
        if is_plain_inline(&text) {
            assert_eq!(parse_inline_syntax(&text, 3), parse_inline_with_pulldown(&text, 3), "{text:?}");
            checked += 1;
        }
    }
    assert!(checked > 10_000, "only {checked} inputs exercised the shortcut");

    for text in ["1. item", " lead", "trail ", "# h", "*a*", "a_b", "x\\y", "Q&A", "<b>", "[l](u)", "- x"] {
        assert!(!is_plain_inline(text), "{text:?} must use pulldown");
    }
}

#[test]
fn line_at_offset_matches_linear_scan() {
    let md = "aa\n\nbb **c**\n# h\n";
    let parsed = parse_document(md);
    for off in 0..=md.len() {
        let binary = parsed.line_at_offset(off).map(|l| l.line_index);
        let linear = parsed
            .lines
            .iter()
            .find(|l| off >= l.source.start && off <= l.source.end)
            .map(|l| l.line_index);
        assert_eq!(binary, linear, "offset {off}");
    }
    for pair in parsed.all_inline_pairs() {
        if pair.kind == InlineTagKind::Link {
            continue;
        }
        let naive_close = parsed
            .all_inline_pairs()
            .filter(|p| p.kind != InlineTagKind::Link && p.inner == pair.inner)
            .map(|p| p.close_delim.end)
            .max()
            .unwrap_or(pair.close_delim.end);
        let naive_open = parsed
            .all_inline_pairs()
            .filter(|p| p.kind != InlineTagKind::Link && p.inner == pair.inner)
            .map(|p| p.open_delim.start)
            .min()
            .unwrap_or(pair.open_delim.start);
        assert_eq!(pair.group_close_end, naive_close);
        assert_eq!(pair.group_open_start, naive_open);
    }
}

#[test]
fn test_in_list_or_quote_tracker_exhaustive() {
    let sample_kinds = [
        LineKind::Paragraph,
        LineKind::Empty,
        LineKind::ListItem {
            ordered: false,
            number: 1,
            task: None,
            indent_cols: 0,
            checkbox_source: None,
        },
        LineKind::Blockquote,
        LineKind::TableRow {
            is_header: false,
            body: Box::default(),
        },
    ];

    fn check_all(
        kinds: &[LineKind],
        seq: &mut Vec<ParsedLine>,
        tracker: InListOrQuoteTracker,
        depth: usize,
    ) {
        assert_eq!(
            tracker.is_in_list_or_quote(),
            in_list_or_quote(seq),
            "Mismatch for sequence: {:?}",
            seq.iter().map(|l| match &l.kind {
                LineKind::Empty => "E",
                LineKind::ListItem { .. } => "L",
                LineKind::Blockquote => "B",
                _ => "O",
            }).collect::<Vec<_>>()
        );
        if depth >= 8 {
            return;
        }
        for kind in kinds {
            let mut next_tracker = tracker;
            next_tracker.push(kind);
            seq.push(ParsedLine {
                line_index: depth,
                source: 0..0,
                kind: kind.clone(),
                prefix_range: 0..0,
                hidden_ranges: Vec::new(),
                inline_pairs: Vec::new(),
                display_text: String::new(),
                offset_map: Arc::new(RowOffsetMap::identity(0, 0)),
                link_hits: no_links(),
                styled_segments: Vec::new(),
            });
            check_all(kinds, seq, next_tracker, depth + 1);
            seq.pop();
        }
    }

    let mut seq = Vec::new();
    check_all(&sample_kinds, &mut seq, InListOrQuoteTracker::default(), 0);
}

#[test]
fn test_20000_row_table_parse_fast() {
    use std::fmt::Write;
    let mut md = String::with_capacity(20002 * 20);
    md.push_str("| col1 | col2 |\n| --- | --- |\n");
    for i in 0..20000 {
        let _ = writeln!(md, "| cell_{i}_1 | cell_{i}_2 |");
    }

    let start = std::time::Instant::now();
    let doc = parse_document(&md);
    let elapsed = start.elapsed();

    assert_eq!(doc.lines.len(), 20003);
    let max_allowed_ms = if cfg!(debug_assertions) { 1000 } else { 100 };
    assert!(
        elapsed.as_millis() < max_allowed_ms,
        "Parsing 20,000 table rows took too long: {elapsed:?}"
    );
}

#[test]
fn test_headings_and_hr() {
    let md = "# Title\n## Subtitle\n### Section\n---";
    let blocks = parse_markdown(md);
    assert_eq!(blocks.len(), 4);
    match &blocks[0] {
        Block::Heading { level, .. } => assert_eq!(*level, 1),
        _ => panic!("Expected H1"),
    }
    match &blocks[1] {
        Block::Heading { level, .. } => assert_eq!(*level, 2),
        _ => panic!("Expected H2"),
    }
    match &blocks[2] {
        Block::Heading { level, .. } => assert_eq!(*level, 3),
        _ => panic!("Expected H3"),
    }
    assert!(matches!(blocks[3], Block::HorizontalRule { .. }));
}

#[test]
fn test_spans_formatting() {
    let input = "Normal **bold** *italic* <u>underlined</u> ~~strike~~ `code`";
    let spans = parse_spans(input);
    assert!(!spans.is_empty());
    let has_bold = spans.iter().any(|s| s.bold && s.text.contains("bold"));
    let has_italic = spans.iter().any(|s| s.italic && s.text.contains("italic"));
    let has_underline = spans
        .iter()
        .any(|s| s.underline && s.text.contains("underlined"));
    let has_strike = spans.iter().any(|s| s.strike && s.text.contains("strike"));
    let has_code = spans.iter().any(|s| s.code && s.text.contains("code"));

    assert!(has_bold, "Expected bold span");
    assert!(has_italic, "Expected italic span");
    assert!(has_underline, "Expected underline span");
    assert!(has_strike, "Expected strike span");
    assert!(has_code, "Expected code span");
}

#[test]
fn test_link_hits_precomputed_per_line() {
    let doc = parse_document("see [site](https://a.example) now\nplain");
    assert_eq!(doc.lines[0].link_hits.len(), 1);
    assert_eq!(doc.lines[0].link_hits[0].1, "https://a.example");
    assert!(doc.lines[1].link_hits.is_empty());
}

#[test]
fn test_styled_segments_bold() {
    let input = "Normal **bold** text";
    let doc = parse_document(input);
    assert_eq!(doc.lines.len(), 1);
    let line = &doc.lines[0];
    println!("display: {:?}", line.display_text);
    println!("segments: {:?}", line.styled_segments);
    let bold_seg = line.styled_segments.iter().find(|s| s.bold);
    assert!(bold_seg.is_some(), "Expected bold styled segment");
}

#[test]
fn test_task_list() {
    let md = "- [ ] Task to do\n- [x] Completed task";
    let blocks = parse_markdown(md);
    assert_eq!(blocks.len(), 2);
    match &blocks[0] {
        Block::ListItem {
            is_task, checked, ..
        } => {
            assert!(is_task);
            assert!(!checked);
        }
        _ => panic!("Expected task item"),
    }
    match &blocks[1] {
        Block::ListItem {
            is_task, checked, ..
        } => {
            assert!(is_task);
            assert!(checked);
        }
        _ => panic!("Expected completed task item"),
    }
}

#[test]
fn escaped_pipe_is_not_a_column_break() {
    assert_eq!(
        split_table_cells(r"| a \| b | c |"),
        vec!["a \\| b".to_string(), "c".to_string()]
    );
    assert_eq!(table_column_align("---"), TableColumnAlign::None);
    assert_eq!(table_column_align(":---"), TableColumnAlign::Left);
    assert_eq!(table_column_align(":---:"), TableColumnAlign::Center);
    assert_eq!(table_column_align("---:"), TableColumnAlign::Right);

    let md = "| a \\| b | c |\n| --- | --- |\n| d | e |";
    let doc = parse_document(md);
    match &doc.lines[0].kind {
        LineKind::TableRow { ref body, .. } => {
            let TableRowBody {
                cells, alignments, ..
            } = &**body;
            assert_eq!(cells.len(), 2);
            assert_eq!(&md[cells[0].clone()], "a \\| b");
            assert_eq!(&md[cells[1].clone()], "c");
            assert_eq!(
                &alignments[..],
                &[TableColumnAlign::None, TableColumnAlign::None]
            );
        }
        other => panic!("expected a table row, got {other:?}"),
    }
}

#[test]
fn test_table_parsing() {
    let md = "| Col 1 | Col 2 |\n| :--- | :--- |\n| Val A | Val B |";
    let blocks = parse_markdown(md);
    assert_eq!(blocks.len(), 1);
    match &blocks[0] {
        Block::Table { headers, rows, .. } => {
            assert_eq!(headers.len(), 2);
            assert_eq!(headers[0], "Col 1");
            assert_eq!(headers[1], "Col 2");
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0][0], "Val A");
            assert_eq!(rows[0][1], "Val B");
        }
        _ => panic!("Expected table block"),
    }
}

#[test]
fn test_parse_document_table() {
    let md = "| **Col 1** | Col 2 |\n| :--- | :--- |\n| Val A | ~~Val B~~ |";
    let doc = parse_document(md);
    assert_eq!(doc.lines.len(), 3);
    assert!(matches!(doc.lines[0].kind, LineKind::TableRow { is_header: true, .. }));
    assert!(matches!(doc.lines[1].kind, LineKind::TableDelimiter));
    assert!(matches!(doc.lines[2].kind, LineKind::TableRow { is_header: false, .. }));
    assert_eq!(doc.lines[0].display_text, "| **Col 1** | Col 2 |");
    // Verify Col 1 is bold (via **), but Col 2 is not forced bold
    let bold_segs: Vec<_> = doc.lines[0].styled_segments.iter().filter(|s| s.bold).collect();
    assert!(!bold_segs.is_empty());
    let normal_segs: Vec<_> = doc.lines[0].styled_segments.iter().filter(|s| !s.bold).collect();
    assert!(!normal_segs.is_empty());
}

#[test]
fn rendered_click_maps_to_source_and_strike_is_marked() {
    let md = "# Hi\n\n~~gone~~";
    let blocks = parse_markdown(md);
    let offset = source_offset_for_click(md, 0, 1).unwrap();
    assert!(md.is_char_boundary(offset));
    assert_eq!(&md[offset..offset + 1], "i");
    let strike = blocks.iter().find_map(|block| match block {
        Block::Paragraph { spans, .. } => spans.iter().find(|span| span.text.contains("gone")),
        _ => None,
    });
    assert!(strike.unwrap().strike);

    let bullet = source_offset_for_click("- alpha", 0, 0).unwrap();
    assert_eq!(&"- alpha"[bullet..bullet + 1], "a");
}

#[test]
fn test_row_offset_map_roundtrip_and_multibyte() {
    let md = "# Hello **world** and [LiteMark](https://example.com) 😀កខ";
    let parsed = parse_document(md);
    assert_eq!(parsed.lines.len(), 1);
    let line = &parsed.lines[0];
    assert_eq!(line.display_text, "Hello world and LiteMark 😀កខ");

    // Display offset 0 -> 'H' at source byte 2 (after "# ")
    assert_eq!(line.offset_map.display_to_source(0), 2);
    assert_eq!(line.offset_map.source_to_display(2), 0);

    // Display offset 6 -> 'w' in "**world**" (source byte 10)
    assert_eq!(line.offset_map.display_to_source(6), 10);
    assert_eq!(line.offset_map.source_to_display(10), 6);

    // Display offset 16 -> 'L' in "[LiteMark]" (source byte 23)
    assert_eq!(line.offset_map.display_to_source(16), 23);
    assert_eq!(line.offset_map.source_to_display(23), 16);

    // Check every char boundary in display_text maps to a valid char boundary in md
    for (disp_idx, ch) in line.display_text.char_indices() {
        let src_idx = line.offset_map.display_to_source(disp_idx);
        assert!(md.is_char_boundary(src_idx));
        assert_eq!(md[src_idx..].chars().next(), Some(ch));
        assert_eq!(line.offset_map.source_to_display(src_idx), disp_idx);
    }
}

struct LeafExpect {
    kind: &'static str,
    display: &'static str,
    source: &'static str,
    marks: &'static str,
    links: &'static str,
}

fn leaf(kind: &'static str, display: &'static str, source: &'static str) -> LeafExpect {
    LeafExpect {
        kind,
        display,
        source,
        marks: "-",
        links: "-",
    }
}

fn leaf_marks(
    kind: &'static str,
    display: &'static str,
    source: &'static str,
    marks: &'static str,
) -> LeafExpect {
    LeafExpect {
        kind,
        display,
        source,
        marks,
        links: "-",
    }
}

fn leaf_link(
    kind: &'static str,
    display: &'static str,
    source: &'static str,
    links: &'static str,
) -> LeafExpect {
    LeafExpect {
        kind,
        display,
        source,
        marks: "-",
        links,
    }
}

fn leaf_kind_token(kind: &LineKind) -> String {
    match kind {
        LineKind::Empty => "empty".to_string(),
        LineKind::Paragraph => "p".to_string(),
        LineKind::Heading { level } => format!("h{level}"),
        LineKind::Blockquote => "quote".to_string(),
        LineKind::ListItem {
            ordered,
            number,
            task,
            indent_cols,
            ..
        } => match (ordered, task) {
            (false, None) => format!("ul:{indent_cols}"),
            (true, None) => format!("ol:{number}:{indent_cols}"),
            (false, Some(false)) => format!("task:{indent_cols}"),
            (false, Some(true)) => format!("task-x:{indent_cols}"),
            (true, Some(_)) => format!("ol-task:{number}:{indent_cols}"),
        },
        LineKind::CodeFence {
            language: Some(language),
            opening: true,
        } => format!("fence-open:{language}"),
        LineKind::CodeFence {
            language: None,
            opening: true,
        } => "fence-open".to_string(),
        LineKind::CodeFence { opening: false, .. } => "fence-close".to_string(),
        LineKind::CodeLine => "code".to_string(),
        LineKind::HorizontalRule => "hr".to_string(),
        LineKind::TableDelimiter => "tdelim".to_string(),
        LineKind::TableRow { .. } => "row".to_string(),
    }
}

fn leaf_dump(md: &str) -> Result<String, String> {
    let doc = parse_document(md);
    let mut out = String::new();
    for line in &doc.lines {
        for (idx, ch) in line.display_text.char_indices() {
            let src = line.offset_map.display_to_source(idx);
            if !md.is_char_boundary(src) || !md[src..].starts_with(ch) {
                return Err(format!(
                    "map display {ch:?} at {idx} -> source {src} in {:?}",
                    line.display_text
                ));
            }
        }
        let mut marks = Vec::new();
        let mut links = Vec::new();
        for segment in &line.styled_segments {
            let mut flags = String::new();
            if segment.bold {
                flags.push('B');
            }
            if segment.italic {
                flags.push('I');
            }
            if segment.code {
                flags.push('C');
            }
            if segment.strike {
                flags.push('S');
            }
            let text = md.get(segment.source.clone()).unwrap_or("");
            if !flags.is_empty() && !text.is_empty() {
                marks.push(format!("{flags}:{text}"));
            }
            if let Some(url) = &segment.link_url {
                links.push(url.clone());
            }
        }
        let marks = if marks.is_empty() {
            "-".to_string()
        } else {
            marks.join(",")
        };
        let links = if links.is_empty() {
            "-".to_string()
        } else {
            links.join(",")
        };
        let source = &md[line.source.clone()];
        out.push_str(&format!(
            "{}\tdisplay={:?}\tsource={:?}\tmarks={marks}\tlinks={links}\n",
            leaf_kind_token(&line.kind),
            line.display_text,
            source,
        ));
    }
    Ok(out)
}

fn expect_dump(lines: &[LeafExpect]) -> String {
    let mut out = String::new();
    for line in lines {
        out.push_str(&format!(
            "{}\tdisplay={:?}\tsource={:?}\tmarks={}\tlinks={}\n",
            line.kind, line.display, line.source, line.marks, line.links,
        ));
    }
    out
}

#[test]
fn unclosed_fence_spans_to_eof() {
    let md = "```\naaa\n";
    let doc = parse_document(md);
    assert_eq!(doc.code_blocks.len(), 1);
    let (open, close) = &doc.code_blocks[0];
    assert_eq!(open.start, 0);
    assert!(open.end > open.start);
    assert_eq!(close.start, md.len());
    assert_eq!(close.end, md.len());
}

/// CommonMark 0.31.2 leaf blocks this parser keeps: ATX headings, thematic
/// breaks, and top-level backtick fences. Each case compares the whole
/// kind, display, source, mark, and link record.
#[test]
fn leaf_blocks_match_commonmark_examples() {
    let cases: Vec<(&str, &str, Vec<LeafExpect>)> = leaf_block_cases();
    let mut failures = Vec::new();
    for (name, md, expected) in &cases {
        let want = expect_dump(expected);
        match leaf_dump(md) {
            Ok(actual) if actual == want => {}
            Ok(actual) => {
                failures.push(format!("CASE {name}\nexpected:\n{want}actual:\n{actual}"))
            }
            Err(err) => failures.push(format!("CASE {name} {err}")),
        }
    }
    assert!(
        failures.is_empty(),
        "{} leaf-block case(s) failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[allow(clippy::too_many_lines)]
fn leaf_block_cases() -> Vec<(&'static str, &'static str, Vec<LeafExpect>)> {
    vec![
        (
            "ex43-hr",
            "***\n---\n___",
            vec![
                leaf("hr", "", "***"),
                leaf("hr", "", "---"),
                leaf("hr", "", "___"),
            ],
        ),
        ("ex44-not-hr", "+++", vec![leaf("p", "+++", "+++")]),
        ("ex45-equals-not-hr", "===", vec![leaf("p", "===", "===")]),
        (
            "ex46-too-short",
            "--\n**\n__",
            vec![
                leaf("p", "--", "--"),
                leaf("p", "**", "**"),
                leaf("p", "__", "__"),
            ],
        ),
        (
            "ex47-hr-indent",
            " ***\n  ***\n   ***",
            vec![
                leaf("hr", "", " ***"),
                leaf("hr", "", "  ***"),
                leaf("hr", "", "   ***"),
            ],
        ),
        (
            "ex50-hr-long",
            "_____________________________________",
            vec![leaf(
                "p",
                "_____________________________________",
                "_____________________________________",
            )],
        ),
        (
            "ex51-hr-spaced",
            " - - -",
            vec![leaf("ul:1", "- -", " - - -")],
        ),
        (
            "ex52-hr-stars",
            " **  * ** * ** * **",
            vec![leaf("p", " **  * ** * ** * **", " **  * ** * ** * **")],
        ),
        (
            "ex53-hr-spaced-dashes",
            "-     -      -      -",
            vec![leaf("ul:0", "    -      -      -", "-     -      -      -")],
        ),
        (
            "ex54-hr-trailing-space",
            "- - - -    ",
            vec![leaf("ul:0", "- - -    ", "- - - -    ")],
        ),
        (
            "ex55-not-hr",
            "_ _ _ _ a\n\na------\n\n---a---",
            vec![
                leaf("p", "_ _ _ _ a", "_ _ _ _ a"),
                leaf("empty", "", ""),
                leaf("p", "a------", "a------"),
                leaf("empty", "", ""),
                leaf("p", "---a---", "---a---"),
            ],
        ),
        (
            "ex56-mixed-not-hr",
            " *-*",
            vec![leaf_marks("p", " -", " *-*", "I:-")],
        ),
        (
            "ex57-hr-between-lists",
            "- foo\n***\n- bar",
            vec![
                leaf("ul:0", "foo", "- foo"),
                leaf("hr", "", "***"),
                leaf("ul:0", "bar", "- bar"),
            ],
        ),
        (
            "ex58-hr-not-setext",
            "Foo\n***\nbar",
            vec![
                leaf("p", "Foo", "Foo"),
                leaf("hr", "", "***"),
                leaf("p", "bar", "bar"),
            ],
        ),
        (
            "ex60-hr-vs-list",
            "* Foo\n* * *\n* Bar",
            vec![
                leaf("ul:0", "Foo", "* Foo"),
                leaf("ul:0", "* *", "* * *"),
                leaf("ul:0", "Bar", "* Bar"),
            ],
        ),
        (
            "ex62-atx-levels",
            "# foo\n## foo\n### foo\n#### foo\n##### foo\n###### foo",
            vec![
                leaf("h1", "foo", "# foo"),
                leaf("h2", "foo", "## foo"),
                leaf("h3", "foo", "### foo"),
                leaf("h4", "foo", "#### foo"),
                leaf("h5", "foo", "##### foo"),
                leaf("h6", "foo", "###### foo"),
            ],
        ),
        (
            "ex63-seven-hashes",
            "####### foo",
            vec![leaf("p", "####### foo", "####### foo")],
        ),
        (
            "audit-seven-hashes",
            "####### no",
            vec![leaf("p", "####### no", "####### no")],
        ),
        (
            "ex64-atx-needs-space",
            "#5 bolt\n\n#hashtag",
            vec![
                leaf("p", "#5 bolt", "#5 bolt"),
                leaf("empty", "", ""),
                leaf("p", "#hashtag", "#hashtag"),
            ],
        ),
        (
            // The WYSIWYG display hides escape backslashes, so the
            // display shows the escaped characters unescaped.
            "ex65-escaped-atx",
            "\\## foo",
            vec![leaf("p", "## foo", "\\## foo")],
        ),
        (
            "ex66-atx-inline",
            "# foo *bar* \\*baz\\*",
            vec![leaf_marks(
                "h1",
                "foo bar *baz*",
                "# foo *bar* \\*baz\\*",
                "I:bar",
            )],
        ),
        (
            // CommonMark 0.31.2 ex67 trims the trailing run, but the
            // WYSIWYG display keeps it so typing spaces at the end of a
            // heading does not swallow them until the next letter.
            "ex67-atx-trim",
            "#                  foo                     ",
            vec![leaf(
                "h1",
                "foo                     ",
                "#                  foo                     ",
            )],
        ),
        (
            "ex68-atx-indent",
            " ### foo\n  ## foo\n   # foo",
            vec![
                leaf("h3", "foo", " ### foo"),
                leaf("h2", "foo", "  ## foo"),
                leaf("h1", "foo", "   # foo"),
            ],
        ),
        (
            "ex69-four-spaces-is-code",
            "    # foo",
            vec![leaf_marks("code", "# foo", "    # foo", "C:# foo")],
        ),
        (
            "ex71-closing-hash",
            "## foo ##\n  ###   bar    ###",
            vec![
                leaf("h2", "foo", "## foo ##"),
                leaf("h3", "bar", "  ###   bar    ###"),
            ],
        ),
        (
            "ex72-closing-hash-run",
            "# foo ##################################\n##### foo ##",
            vec![
                leaf("h1", "foo", "# foo ##################################"),
                leaf("h5", "foo", "##### foo ##"),
            ],
        ),
        (
            "ex73-closing-hash-spaces",
            "### foo ###     ",
            vec![leaf("h3", "foo", "### foo ###     ")],
        ),
        (
            "ex74-hash-not-closer",
            "### foo ### b",
            vec![leaf("h3", "foo ### b", "### foo ### b")],
        ),
        (
            "ex75-hash-needs-space",
            "# foo#",
            vec![leaf("h1", "foo#", "# foo#")],
        ),
        (
            "ex76-escaped-closer",
            "### foo \\###\n## foo #\\##\n# foo \\#",
            vec![
                leaf("h3", "foo ###", "### foo \\###"),
                leaf("h2", "foo ###", "## foo #\\##"),
                leaf("h1", "foo #", "# foo \\#"),
            ],
        ),
        (
            "ex77-four-stars",
            "****\n## foo\n****",
            vec![
                leaf("p", "****", "****"),
                leaf("h2", "foo", "## foo"),
                leaf("p", "****", "****"),
            ],
        ),
        (
            "ex78-atx-interrupts",
            "Foo bar\n# baz\nBar foo",
            vec![
                leaf("p", "Foo bar", "Foo bar"),
                leaf("h1", "baz", "# baz"),
                leaf("p", "Bar foo", "Bar foo"),
            ],
        ),
        (
            "ex79-empty-atx",
            "## \n#\n### ###",
            vec![
                leaf("h2", "", "## "),
                leaf("h1", "", "#"),
                leaf("h3", "", "### ###"),
            ],
        ),
        (
            "ex88-not-setext",
            "Foo\n= =\n\nFoo\n--- -",
            vec![
                leaf("p", "Foo", "Foo"),
                leaf("p", "= =", "= ="),
                leaf("empty", "", ""),
                leaf("p", "Foo", "Foo"),
                leaf("p", "--- -", "--- -"),
            ],
        ),
        (
            "ex92-quote-then-hr",
            "> Foo\n---",
            vec![leaf("quote", "Foo", "> Foo"), leaf("hr", "", "---")],
        ),
        (
            "ex94-list-then-hr",
            "- Foo\n---",
            vec![leaf("ul:0", "Foo", "- Foo"), leaf("hr", "", "---")],
        ),
        (
            "ex97-equals-after-blank",
            "\n====",
            vec![leaf("empty", "", ""), leaf("p", "====", "====")],
        ),
        (
            "ex98-two-hrs",
            "---\n---",
            vec![leaf("hr", "", "---"), leaf("hr", "", "---")],
        ),
        (
            "ex119-backtick-fence",
            "```\n<\n >\n```",
            vec![
                leaf("fence-open", "", "```"),
                leaf_marks("code", "<", "<", "C:<"),
                leaf_marks("code", " >", " >", "C: >"),
                leaf("fence-close", "", "```"),
            ],
        ),
        (
            "ex122-tilde-inside-backticks",
            "```\naaa\n~~~\n```",
            vec![
                leaf("fence-open", "", "```"),
                leaf_marks("code", "aaa", "aaa", "C:aaa"),
                leaf_marks("code", "~~~", "~~~", "C:~~~"),
                leaf("fence-close", "", "```"),
            ],
        ),
        (
            "ex124-fence-len",
            "````\naaa\n```\n``````",
            vec![
                leaf("fence-open", "", "````"),
                leaf_marks("code", "aaa", "aaa", "C:aaa"),
                leaf_marks("code", "```", "```", "C:```"),
                leaf("fence-close", "", "``````"),
            ],
        ),
        (
            "ex126-open-fence",
            "```",
            vec![leaf("fence-open", "", "```")],
        ),
        (
            "ex127-unclosed-fence",
            "`````\n\n```\naaa",
            vec![
                leaf("fence-open", "", "`````"),
                leaf("code", "", ""),
                leaf_marks("code", "```", "```", "C:```"),
                leaf_marks("code", "aaa", "aaa", "C:aaa"),
            ],
        ),
        (
            "ex129-fence-blanks",
            "```\n\n  \n```",
            vec![
                leaf("fence-open", "", "```"),
                leaf("code", "", ""),
                leaf_marks("code", "  ", "  ", "C:  "),
                leaf("fence-close", "", "```"),
            ],
        ),
        (
            "ex130-empty-fence",
            "```\n```",
            vec![
                leaf("fence-open", "", "```"),
                leaf("fence-close", "", "```"),
            ],
        ),
        (
            "ex131-fence-indent-1",
            " ```\n aaa\naaa\n```",
            vec![
                leaf("fence-open", "", " ```"),
                leaf_marks("code", "aaa", " aaa", "C:aaa"),
                leaf_marks("code", "aaa", "aaa", "C:aaa"),
                leaf("fence-close", "", "```"),
            ],
        ),
        (
            "ex132-fence-indent-2",
            "  ```\naaa\n  aaa\naaa\n  ```",
            vec![
                leaf("fence-open", "", "  ```"),
                leaf_marks("code", "aaa", "aaa", "C:aaa"),
                leaf_marks("code", "aaa", "  aaa", "C:aaa"),
                leaf_marks("code", "aaa", "aaa", "C:aaa"),
                leaf("fence-close", "", "  ```"),
            ],
        ),
        (
            "ex133-fence-indent-3",
            "   ```\n   aaa\n    aaa\n  aaa\n   ```",
            vec![
                leaf("fence-open", "", "   ```"),
                leaf_marks("code", "aaa", "   aaa", "C:aaa"),
                leaf_marks("code", " aaa", "    aaa", "C: aaa"),
                leaf_marks("code", "aaa", "  aaa", "C:aaa"),
                leaf("fence-close", "", "   ```"),
            ],
        ),
        (
            "ex135-closer-indent",
            "```\naaa\n  ```",
            vec![
                leaf("fence-open", "", "```"),
                leaf_marks("code", "aaa", "aaa", "C:aaa"),
                leaf("fence-close", "", "  ```"),
            ],
        ),
        (
            "ex136-opener-indent-3",
            "   ```\naaa\n  ```",
            vec![
                leaf("fence-open", "", "   ```"),
                leaf_marks("code", "aaa", "aaa", "C:aaa"),
                leaf("fence-close", "", "  ```"),
            ],
        ),
        (
            "ex137-closer-indent-4",
            "```\naaa\n    ```",
            vec![
                leaf("fence-open", "", "```"),
                leaf_marks("code", "aaa", "aaa", "C:aaa"),
                leaf_marks("code", "    ```", "    ```", "C:    ```"),
            ],
        ),
        (
            "ex138-backtick-in-info",
            "``` ```\naaa",
            vec![
                leaf_marks("p", " ", "``` ```", "C: "),
                leaf("p", "aaa", "aaa"),
            ],
        ),
        (
            "ex140-fence-interrupts",
            "foo\n```\nbar\n```\nbaz",
            vec![
                leaf("p", "foo", "foo"),
                leaf("fence-open", "", "```"),
                leaf_marks("code", "bar", "bar", "C:bar"),
                leaf("fence-close", "", "```"),
                leaf("p", "baz", "baz"),
            ],
        ),
        (
            "ex142-fence-info",
            "```ruby\ndef foo(x)\n  return 3\nend\n```",
            vec![
                leaf("fence-open:ruby", "", "```ruby"),
                leaf_marks("code", "def foo(x)", "def foo(x)", "C:def foo(x)"),
                leaf_marks("code", "  return 3", "  return 3", "C:  return 3"),
                leaf_marks("code", "end", "end", "C:end"),
                leaf("fence-close", "", "```"),
            ],
        ),
        (
            "ex144-info-semicolon",
            "````;\n````",
            vec![
                leaf("fence-open:;", "", "````;"),
                leaf("fence-close", "", "````"),
            ],
        ),
        (
            "ex145-backtick-info-not-fence",
            "``` aa ```\nfoo",
            vec![
                leaf_marks("p", " aa ", "``` aa ```", "C: aa "),
                leaf("p", "foo", "foo"),
            ],
        ),
        (
            "ex147-closer-info",
            "```\n``` aaa\n```",
            vec![
                leaf("fence-open", "", "```"),
                leaf_marks("code", "``` aaa", "``` aaa", "C:``` aaa"),
                leaf("fence-close", "", "```"),
            ],
        ),
        (
            "bold-closing-hash",
            "## **foo** ##",
            vec![leaf_marks("h2", "foo", "## **foo** ##", "B:foo")],
        ),
        (
            "heading-link",
            "# [Foo](/url)",
            vec![leaf_link("h1", "Foo", "# [Foo](/url)", "/url")],
        ),
        (
            "nested-list-not-code",
            "- foo\n    - bar",
            vec![
                leaf("ul:0", "foo", "- foo"),
                leaf("ul:4", "bar", "    - bar"),
            ],
        ),
        (
            "quote-indent-3",
            "   > foo",
            vec![leaf("quote", "foo", "   > foo")],
        ),
        (
            "tab-list-not-code",
            "\t- foo",
            vec![leaf("ul:1", "foo", "\t- foo")],
        ),
        (
            "double-tilde-not-fence",
            "~~Hi~~",
            vec![leaf_marks("p", "Hi", "~~Hi~~", "S:Hi")],
        ),
        (
            "blank-spaces-not-code",
            "    ",
            vec![leaf("empty", "    ", "    ")],
        ),
        (
            "spaced-dashes-after-paragraph",
            "Foo\n- - -",
            vec![leaf("p", "Foo", "Foo"), leaf("ul:0", "- -", "- - -")],
        ),
        (
            "four-dashes-after-paragraph-stay-text",
            "Foo\n ----\nFoo\n   ----      ",
            vec![
                leaf("p", "Foo", "Foo"),
                leaf("p", " ----", " ----"),
                leaf("p", "Foo", "Foo"),
                leaf("p", "   ----      ", "   ----      "),
            ],
        ),
        (
            "four-dashes-at-top-are-hr",
            "----",
            vec![leaf("p", "----", "----")],
        ),
        (
            "table-unchanged",
            "| a | b |\n| --- | --- |\n| c | d |",
            vec![
                leaf("row", "| a | b |", "| a | b |"),
                leaf("tdelim", "", "| --- | --- |"),
                leaf("row", "| c | d |", "| c | d |"),
            ],
        ),
        (
            "fence-tabs",
            "```\n\tkeep\tme\n```",
            vec![
                leaf("fence-open", "", "```"),
                leaf_marks("code", "\tkeep\tme", "\tkeep\tme", "C:\tkeep\tme"),
                leaf("fence-close", "", "```"),
            ],
        ),
        (
            "top-fence-keeps-shorter-backticks-as-code",
            "`````\naaa\n```\nbar",
            vec![
                leaf("fence-open", "", "`````"),
                leaf_marks("code", "aaa", "aaa", "C:aaa"),
                leaf_marks("code", "```", "```", "C:```"),
                leaf_marks("code", "bar", "bar", "C:bar"),
            ],
        ),
        (
            "list-fence-closes-on-three-backticks",
            "- foo\n````\ncode\n```\nbar",
            vec![
                leaf("ul:0", "foo", "- foo"),
                leaf("fence-open:`", "", "````"),
                leaf_marks("code", "code", "code", "C:code"),
                leaf("fence-close", "", "```"),
                leaf("p", "bar", "bar"),
            ],
        ),
        (
            "tilde-fence",
            "~~~\ncode\n~~~",
            vec![
                leaf("fence-open", "", "~~~"),
                leaf_marks("code", "code", "code", "C:code"),
                leaf("fence-close", "", "~~~"),
            ],
        ),
        (
            "ex119-tilde-fence",
            "~~~\n<\n >\n~~~",
            vec![
                leaf("fence-open", "", "~~~"),
                leaf_marks("code", "<", "<", "C:<"),
                leaf_marks("code", " >", " >", "C: >"),
                leaf("fence-close", "", "~~~"),
            ],
        ),
        (
            "ex122-backticks-do-not-close-tilde",
            "~~~\naaa\n```\n~~~",
            vec![
                leaf("fence-open", "", "~~~"),
                leaf_marks("code", "aaa", "aaa", "C:aaa"),
                leaf_marks("code", "```", "```", "C:```"),
                leaf("fence-close", "", "~~~"),
            ],
        ),
        (
            "ex124-shorter-tilde-run-is-code",
            "~~~~\naaa\n~~~\n~~~~",
            vec![
                leaf("fence-open", "", "~~~~"),
                leaf_marks("code", "aaa", "aaa", "C:aaa"),
                leaf_marks("code", "~~~", "~~~", "C:~~~"),
                leaf("fence-close", "", "~~~~"),
            ],
        ),
        (
            "ex146-tilde-info-may-hold-backticks",
            "~~~ aa ``` ~~~\nfoo\n~~~",
            vec![
                leaf("fence-open:aa ``` ~~~", "", "~~~ aa ``` ~~~"),
                leaf_marks("code", "foo", "foo", "C:foo"),
                leaf("fence-close", "", "~~~"),
            ],
        ),
        (
            "tilde-fence-unclosed-runs-to-end",
            "~~~\nfoo",
            vec![
                leaf("fence-open", "", "~~~"),
                leaf_marks("code", "foo", "foo", "C:foo"),
            ],
        ),
        (
            "ex107-indented-code",
            "    a simple\n      indented code block",
            vec![
                leaf_marks("code", "a simple", "    a simple", "C:a simple"),
                leaf_marks(
                    "code",
                    "  indented code block",
                    "      indented code block",
                    "C:  indented code block",
                ),
            ],
        ),
        (
            "ex110-indented-code-cannot-interrupt-paragraph",
            "Foo\n    bar",
            vec![leaf("p", "Foo", "Foo"), leaf("p", "    bar", "    bar")],
        ),
        (
            "ex111-paragraph-ends-indented-code",
            "    foo\nbar",
            vec![
                leaf_marks("code", "foo", "    foo", "C:foo"),
                leaf("p", "bar", "bar"),
            ],
        ),
        (
            "indented-code-after-heading",
            "# Heading\n    foo",
            vec![
                leaf("h1", "Heading", "# Heading"),
                leaf_marks("code", "foo", "    foo", "C:foo"),
            ],
        ),
        (
            "ex113-indent-past-four-is-kept",
            "        foo\n    bar",
            vec![
                leaf_marks("code", "    foo", "        foo", "C:    foo"),
                leaf_marks("code", "bar", "    bar", "C:bar"),
            ],
        ),
        (
            "ex117-blank-line-inside-indented-code",
            "    chunk1\n\n    chunk2",
            vec![
                leaf_marks("code", "chunk1", "    chunk1", "C:chunk1"),
                leaf("empty", "", ""),
                leaf_marks("code", "chunk2", "    chunk2", "C:chunk2"),
            ],
        ),
        (
            "backtick-line-inside-indented-code-stays-code",
            "    x\n    ```\n    y",
            vec![
                leaf_marks("code", "x", "    x", "C:x"),
                leaf_marks("code", "```", "    ```", "C:```"),
                leaf_marks("code", "y", "    y", "C:y"),
            ],
        ),
        (
            "backtick-line-after-blank-in-indented-code-stays-code",
            "    x\n\n    ```\ny",
            vec![
                leaf_marks("code", "x", "    x", "C:x"),
                leaf("empty", "", ""),
                leaf_marks("code", "```", "    ```", "C:```"),
                leaf("p", "y", "y"),
            ],
        ),
        (
            "list-content-is-not-indented-code",
            "- foo\n\n    bar",
            vec![
                leaf("ul:0", "foo", "- foo"),
                leaf("empty", "", ""),
                leaf("p", "    bar", "    bar"),
            ],
        ),
        (
            "four-space-continuation-parses-inline",
            "foo\n    **bar** `c`",
            vec![
                leaf("p", "foo", "foo"),
                leaf_marks("p", "    bar c", "    **bar** `c`", "B:bar,C:c"),
            ],
        ),
        (
            "quote-blank-then-fence-info-stays-fence",
            "> foo\n\n    ``` aa ```",
            vec![
                leaf("quote", "foo", "> foo"),
                leaf("empty", "", ""),
                leaf("fence-open:aa ```", "", "    ``` aa ```"),
            ],
        ),
        (
            "tab-fence-stays-red",
            "\t```\ncode",
            vec![
                leaf("fence-open", "", "\t```"),
                leaf_marks("code", "code", "code", "C:code"),
            ],
        ),
    ]
}

fn kind_report(md: &str, kind: &LineKind) -> String {
    match kind {
        LineKind::Empty => "Empty".to_string(),
        LineKind::Paragraph => "Paragraph".to_string(),
        LineKind::Heading { level } => format!("Heading level={level}"),
        LineKind::Blockquote => "Blockquote".to_string(),
        LineKind::ListItem {
            ordered,
            number,
            task,
            indent_cols,
            checkbox_source,
        } => {
            let task = match task {
                None => "none",
                Some(false) => "unchecked",
                Some(true) => "checked",
            };
            let checkbox = match checkbox_source {
                Some(range) => format!("{:?}", &md[range.clone()]),
                None => "-".to_string(),
            };
            format!(
                "ListItem ordered={ordered} number={number} task={task} indent={indent_cols} checkbox={checkbox}"
            )
        }
        LineKind::CodeFence { language, opening } => {
            format!("CodeFence opening={opening} language={language:?}")
        }
        LineKind::CodeLine => "CodeLine".to_string(),
        LineKind::HorizontalRule => "HorizontalRule".to_string(),
        LineKind::TableDelimiter => "TableDelimiter".to_string(),
        LineKind::TableRow {
            is_header,
            ref body,
            ..
        } => format!("TableRow header={is_header} cells={}", body.cells.len()),
    }
}

fn strike_report(md: &str, line: &ParsedLine) -> String {
    let mut parts = Vec::new();
    for segment in &line.styled_segments {
        if segment.strike {
            parts.push(format!("{:?}", &md[segment.source.clone()]));
        }
    }
    if parts.is_empty() {
        "-".to_string()
    } else {
        parts.join(",")
    }
}

fn link_report(line: &ParsedLine) -> String {
    let mut urls = Vec::new();
    for segment in &line.styled_segments {
        if let Some(url) = &segment.link_url {
            urls.push(format!("{url:?}"));
        }
    }
    if urls.is_empty() {
        "-".to_string()
    } else {
        urls.join(",")
    }
}

fn map_status(md: &str, line: &ParsedLine) -> String {
    for (idx, ch) in line.display_text.char_indices() {
        let src = line.offset_map.display_to_source(idx);
        if !md.is_char_boundary(src) {
            return format!("fail boundary {src}");
        }
        match md[src..].chars().next() {
            Some(got) if got == ch => {}
            other => {
                return format!("fail display {ch:?}@{idx} source {other:?}@{src}");
            }
        }
    }
    "ok".to_string()
}

fn observe_line(md: &str, index: usize) -> String {
    let doc = parse_document(md);
    let line = &doc.lines[index];
    format!(
        "{} display={:?} strike={} link={} map={}",
        kind_report(md, &line.kind),
        line.display_text,
        strike_report(md, line),
        link_report(line),
        map_status(md, line),
    )
}

fn observe_doc(label: &str, md: &str) -> String {
    let doc = parse_document(md);
    let mut out = String::new();
    for index in 0..doc.lines.len() {
        if index > 0 {
            out.push('\n');
        }
        out.push_str(&format!("{label}[{index}] {}", observe_line(md, index)));
    }
    out
}

/// CommonMark examples 265, 266, 283, 296, and 302, plus GFM ordered tasks.
#[test]
fn markers_match_gfm() {
    let observed = format!(
        "{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
        observe_doc("cm302", "1. foo\n2. bar\n3) baz"),
        observe_doc("cm296", "10) foo\n    - bar"),
        observe_doc("cm266", "1234567890. not ok"),
        observe_doc("cm265", "123456789. ok"),
        observe_doc("task", "1. [ ] first\n2. [x] second"),
        observe_doc("ex283", "1. foo\n2.\n3. bar"),
        observe_doc("eol_dot", "1."),
        observe_doc("eol_paren", "1)"),
        observe_doc("tab_dot", "1.\tfoo"),
        observe_doc("tab_paren", "1)\tfoo"),
        observe_doc("task_tab", "1. [ ]\tfoo"),
        observe_doc("task_eol", "1. [ ]"),
        observe_doc("task_x_eol", "1. [x]"),
        observe_doc("task_sp_eol", "1. [ ] "),
        observe_doc("task_tight", "1. [ ]foo"),
        observe_doc("task_gap", "1.  [ ] foo"),
        observe_doc("bullet_gap", "-  [ ] foo"),
        observe_doc("task_spaces", "1. [ ]  foo"),
        observe_doc("task_tabs", "1. [ ]\t\tbar"),
    );
    let expected = "\
cm302[0] ListItem ordered=true number=1 task=none indent=0 checkbox=- display=\"foo\" strike=- link=- map=ok
cm302[1] ListItem ordered=true number=2 task=none indent=0 checkbox=- display=\"bar\" strike=- link=- map=ok
cm302[2] ListItem ordered=true number=3 task=none indent=0 checkbox=- display=\"baz\" strike=- link=- map=ok
cm296[0] ListItem ordered=true number=10 task=none indent=0 checkbox=- display=\"foo\" strike=- link=- map=ok
cm296[1] ListItem ordered=false number=0 task=none indent=4 checkbox=- display=\"bar\" strike=- link=- map=ok
cm266[0] Paragraph display=\"1234567890. not ok\" strike=- link=- map=ok
cm265[0] ListItem ordered=true number=123456789 task=none indent=0 checkbox=- display=\"ok\" strike=- link=- map=ok
task[0] ListItem ordered=true number=1 task=unchecked indent=0 checkbox=\"[ ]\" display=\"first\" strike=- link=- map=ok
task[1] ListItem ordered=true number=2 task=checked indent=0 checkbox=\"[x]\" display=\"second\" strike=- link=- map=ok
ex283[0] ListItem ordered=true number=1 task=none indent=0 checkbox=- display=\"foo\" strike=- link=- map=ok
ex283[1] ListItem ordered=true number=2 task=none indent=0 checkbox=- display=\"\" strike=- link=- map=ok
ex283[2] ListItem ordered=true number=3 task=none indent=0 checkbox=- display=\"bar\" strike=- link=- map=ok
eol_dot[0] ListItem ordered=true number=1 task=none indent=0 checkbox=- display=\"\" strike=- link=- map=ok
eol_paren[0] ListItem ordered=true number=1 task=none indent=0 checkbox=- display=\"\" strike=- link=- map=ok
tab_dot[0] ListItem ordered=true number=1 task=none indent=0 checkbox=- display=\"foo\" strike=- link=- map=ok
tab_paren[0] ListItem ordered=true number=1 task=none indent=0 checkbox=- display=\"foo\" strike=- link=- map=ok
task_tab[0] ListItem ordered=true number=1 task=unchecked indent=0 checkbox=\"[ ]\" display=\"foo\" strike=- link=- map=ok
task_eol[0] ListItem ordered=true number=1 task=none indent=0 checkbox=- display=\"[ ]\" strike=- link=- map=ok
task_x_eol[0] ListItem ordered=true number=1 task=none indent=0 checkbox=- display=\"[x]\" strike=- link=- map=ok
task_sp_eol[0] ListItem ordered=true number=1 task=unchecked indent=0 checkbox=\"[ ]\" display=\"\" strike=- link=- map=ok
task_tight[0] ListItem ordered=true number=1 task=none indent=0 checkbox=- display=\"[ ]foo\" strike=- link=- map=ok
task_gap[0] ListItem ordered=true number=1 task=unchecked indent=0 checkbox=\"[ ]\" display=\"foo\" strike=- link=- map=ok
bullet_gap[0] ListItem ordered=false number=0 task=unchecked indent=0 checkbox=\"[ ]\" display=\"foo\" strike=- link=- map=ok
task_spaces[0] ListItem ordered=true number=1 task=unchecked indent=0 checkbox=\"[ ]\" display=\"foo\" strike=- link=- map=ok
task_tabs[0] ListItem ordered=true number=1 task=unchecked indent=0 checkbox=\"[ ]\" display=\"bar\" strike=- link=- map=ok";
    assert_eq!(observed, expected);
}

#[test]
fn task_eol_is_not_a_task() {
    for (source, line) in [
        (
            "1. [ ]",
            "ListItem ordered=true number=1 task=none indent=0 checkbox=- display=\"[ ]\" strike=- link=- map=ok",
        ),
        (
            "1. [x]",
            "ListItem ordered=true number=1 task=none indent=0 checkbox=- display=\"[x]\" strike=- link=- map=ok",
        ),
        (
            "1. [X]",
            "ListItem ordered=true number=1 task=none indent=0 checkbox=- display=\"[X]\" strike=- link=- map=ok",
        ),
        (
            "1) [ ]",
            "ListItem ordered=true number=1 task=none indent=0 checkbox=- display=\"[ ]\" strike=- link=- map=ok",
        ),
        (
            "- [ ]",
            "ListItem ordered=false number=0 task=none indent=0 checkbox=- display=\"[ ]\" strike=- link=- map=ok",
        ),
        (
            "1. [ ] ",
            "ListItem ordered=true number=1 task=unchecked indent=0 checkbox=\"[ ]\" display=\"\" strike=- link=- map=ok",
        ),
        (
            "1. [ ]\t",
            "ListItem ordered=true number=1 task=unchecked indent=0 checkbox=\"[ ]\" display=\"\" strike=- link=- map=ok",
        ),
    ] {
        assert_eq!(observe_line(source, 0), line, "{source:?}");
    }
}

#[test]
fn list_padding_is_one_to_four_columns() {
    for (source, ordered, task) in [
        ("1.  foo", true, "none"),
        ("1.   foo", true, "none"),
        ("1.    foo", true, "none"),
        ("1)  foo", true, "none"),
        ("1. \tfoo", true, "none"),
        ("1.  [ ] foo", true, "unchecked"),
        ("1.   [ ] foo", true, "unchecked"),
        ("1.    [ ] foo", true, "unchecked"),
        ("-  [ ] foo", false, "unchecked"),
    ] {
        let checkbox = if task == "none" {
            "-".to_string()
        } else {
            "\"[ ]\"".to_string()
        };
        let number = usize::from(ordered);
        assert_eq!(
            observe_line(source, 0),
            format!(
                "ListItem ordered={ordered} number={number} task={task} indent=0 checkbox={checkbox} display=\"foo\" strike=- link=- map=ok"
            ),
            "{source:?}"
        );
    }
    assert_eq!(
        observe_line("1.     foo", 0),
        "ListItem ordered=true number=1 task=none indent=0 checkbox=- display=\"    foo\" strike=- link=- map=ok"
    );
    assert_eq!(
        observe_line("1.     [ ] foo", 0),
        "ListItem ordered=true number=1 task=none indent=0 checkbox=- display=\"    [ ] foo\" strike=- link=- map=ok"
    );
}

/// A CR LF ending is not part of the line text. The marker scan still
/// treats that ending as the end of the marker, so these stay list items
/// and the display is the text before the CR.
#[test]
fn trailing_cr_stays_in_the_editor_line() {
    assert_eq!(
        observe_line("1.\r\n", 0),
        "ListItem ordered=true number=1 task=none indent=0 checkbox=- display=\"\" strike=- link=- map=ok"
    );
    assert_eq!(
        observe_line("1)\r\n", 0),
        "ListItem ordered=true number=1 task=none indent=0 checkbox=- display=\"\" strike=- link=- map=ok"
    );
    assert_eq!(
        observe_line("1. foo\r\n", 0),
        "ListItem ordered=true number=1 task=none indent=0 checkbox=- display=\"foo\" strike=- link=- map=ok"
    );
    assert_eq!(
        observe_line("1. [ ]\r\n", 0),
        "ListItem ordered=true number=1 task=none indent=0 checkbox=- display=\"[ ]\" strike=- link=- map=ok"
    );
    assert_eq!(
        observe_line("1. [ ] \r\n", 0),
        "ListItem ordered=true number=1 task=unchecked indent=0 checkbox=\"[ ]\" display=\"\" strike=- link=- map=ok"
    );
}

#[test]
fn unordered_task_uses_task_marker() {
    assert_eq!(
        observe_line("- [ ]\tfoo", 0),
        "ListItem ordered=false number=0 task=unchecked indent=0 checkbox=\"[ ]\" display=\"foo\" strike=- link=- map=ok"
    );
    assert_eq!(
        observe_line("- [ ]  foo", 0),
        "ListItem ordered=false number=0 task=unchecked indent=0 checkbox=\"[ ]\" display=\"foo\" strike=- link=- map=ok"
    );
    assert_eq!(
        observe_line("* [x] bar", 0),
        "ListItem ordered=false number=0 task=checked indent=0 checkbox=\"[x]\" display=\"bar\" strike=- link=- map=ok"
    );
    assert_eq!(
        observe_line("+ [ ] baz", 0),
        "ListItem ordered=false number=0 task=unchecked indent=0 checkbox=\"[ ]\" display=\"baz\" strike=- link=- map=ok"
    );
}

#[test]
fn task_bracket_accepts_tab_vt_ff() {
    for (source, checkbox) in [
        ("1. [\t] foo", "\"[\\t]\""),
        ("1. [\u{b}] foo", "\"[\\u{b}]\""),
        ("1. [\u{c}] foo", "\"[\\u{c}]\""),
    ] {
        assert_eq!(
            observe_line(source, 0),
            format!(
                "ListItem ordered=true number=1 task=unchecked indent=0 checkbox={checkbox} display=\"foo\" strike=- link=- map=ok"
            ),
            "{source:?}"
        );
    }
    assert_eq!(
        observe_line("1. [ ]\u{a0}foo", 0),
        "ListItem ordered=true number=1 task=none indent=0 checkbox=- display=\"[ ]\\u{a0}foo\" strike=- link=- map=ok"
    );
}

/// Examples 285 and 367: an empty marker does not interrupt a paragraph.
#[test]
fn empty_marker_does_not_interrupt_paragraph() {
    let ex285 = observe_doc("ex285", "foo\n*\n\nfoo\n1.\n");
    assert_eq!(
        ex285,
        "\
ex285[0] Paragraph display=\"foo\" strike=- link=- map=ok
ex285[1] Paragraph display=\"*\" strike=- link=- map=ok
ex285[2] Empty display=\"\" strike=- link=- map=ok
ex285[3] Paragraph display=\"foo\" strike=- link=- map=ok
ex285[4] Paragraph display=\"1.\" strike=- link=- map=ok
ex285[5] Empty display=\"\" strike=- link=- map=ok"
    );
    let ex367 = observe_doc("ex367", "*foo bar\n*\n");
    assert_eq!(
        ex367,
        "\
ex367[0] Paragraph display=\"*foo bar\" strike=- link=- map=ok
ex367[1] Paragraph display=\"*\" strike=- link=- map=ok
ex367[2] Empty display=\"\" strike=- link=- map=ok"
    );
    let dash = observe_doc("dash", "foo\n-\n");
    assert_eq!(
        dash,
        "\
dash[0] Paragraph display=\"foo\" strike=- link=- map=ok
dash[1] Paragraph display=\"-\" strike=- link=- map=ok
dash[2] Empty display=\"\" strike=- link=- map=ok"
    );
}

/// Example 304: after paragraph text, only an ordered marker with start 1 interrupts.
#[test]
fn ordered_marker_interrupts_only_when_start_is_one() {
    let paren = observe_doc(
        "paren304",
        "The number of windows in my house is\n14)  The number of doors is 6.\n",
    );
    assert_eq!(
        paren,
        "\
paren304[0] Paragraph display=\"The number of windows in my house is\" strike=- link=- map=ok
paren304[1] Paragraph display=\"14)  The number of doors is 6.\" strike=- link=- map=ok
paren304[2] Empty display=\"\" strike=- link=- map=ok"
    );
    let dot = observe_doc(
        "ex304",
        "The number of windows in my house is\n14.  The number of doors is 6.\n",
    );
    assert_eq!(
        dot,
        "\
ex304[0] Paragraph display=\"The number of windows in my house is\" strike=- link=- map=ok
ex304[1] Paragraph display=\"14.  The number of doors is 6.\" strike=- link=- map=ok
ex304[2] Empty display=\"\" strike=- link=- map=ok"
    );
    let start = observe_doc(
        "ex305",
        "The number of windows in my house is\n1.  The number of doors is 6.\n",
    );
    assert_eq!(
        start,
        "\
ex305[0] Paragraph display=\"The number of windows in my house is\" strike=- link=- map=ok
ex305[1] ListItem ordered=true number=1 task=none indent=0 checkbox=- display=\"The number of doors is 6.\" strike=- link=- map=ok
ex305[2] Empty display=\"\" strike=- link=- map=ok"
    );
    let start_paren = observe_doc(
        "paren305",
        "The number of windows in my house is\n1)  The number of doors is 6.\n",
    );
    assert_eq!(
        start_paren,
        "\
paren305[0] Paragraph display=\"The number of windows in my house is\" strike=- link=- map=ok
paren305[1] ListItem ordered=true number=1 task=none indent=0 checkbox=- display=\"The number of doors is 6.\" strike=- link=- map=ok
paren305[2] Empty display=\"\" strike=- link=- map=ok"
    );
}

/// A marker at column 4 or later is not a list item unless a parent item
/// contains it: at document level the line is indented code.
#[test]
fn marker_at_column_four_is_not_a_list_item() {
    let tabbed = observe_doc("tab", " \t63.\tnot\n");
    assert_eq!(
        tabbed,
        "\
tab[0] CodeLine display=\"63.\\tnot\" strike=- link=- map=ok
tab[1] Empty display=\"\" strike=- link=- map=ok"
    );
    let indented = observe_doc("code", "    - code\n");
    assert_eq!(
        indented,
        "\
code[0] CodeLine display=\"- code\" strike=- link=- map=ok
code[1] Empty display=\"\" strike=- link=- map=ok"
    );
    let nested = observe_doc("nest", "10) foo\n    - bar\n");
    assert_eq!(
        nested,
        "\
nest[0] ListItem ordered=true number=10 task=none indent=0 checkbox=- display=\"foo\" strike=- link=- map=ok
nest[1] ListItem ordered=false number=0 task=none indent=4 checkbox=- display=\"bar\" strike=- link=- map=ok
nest[2] Empty display=\"\" strike=- link=- map=ok"
    );
}

#[test]
fn checkbox_click_ticks_tab_vt_ff_and_unticks() {
    fn click(source: &str) -> String {
        let doc = parse_document(source);
        let LineKind::ListItem {
            checkbox_source: Some(range),
            ..
        } = &doc.lines[0].kind
        else {
            panic!("no checkbox in {source:?}");
        };
        let replacement = checkbox_click_replacement(&source[range.clone()]);
        let mut out = source.to_string();
        out.replace_range(range.clone(), replacement);
        out
    }

    assert_eq!(click("1. [\t] a"), "1. [x] a");
    assert_eq!(click("1. [\u{b}] a"), "1. [x] a");
    assert_eq!(click("1. [\u{c}] a"), "1. [x] a");
    assert_eq!(click("1) [\t] a"), "1) [x] a");
    assert_eq!(click("- [\t] a"), "- [x] a");
    assert_eq!(click("1. [ ] a"), "1. [x] a");
    assert_eq!(click("1. [x] a"), "1. [ ] a");
    assert_eq!(click("1. [X] a"), "1. [ ] a");
    assert_eq!(click(&click("1. [\t] a")), "1. [ ] a");
    assert_eq!(click(&click("1. [\u{b}] a")), "1. [ ] a");
    assert_eq!(click(&click("1. [\u{c}] a")), "1. [ ] a");
}

/// CommonMark list items, exception 1 (spec lines 4123–4129): an empty
/// marker, or an ordered marker whose start is not 1, does not interrupt
/// paragraph continuation text. That includes the paragraph inside a
/// parent item. A blank line ends the paragraph, and a start of 1 with
/// text still opens a nested list.
#[test]
fn nested_marker_does_not_interrupt_item_paragraph() {
    assert_eq!(
        observe_doc("star", "- foo\n  *\n"),
        "\
star[0] ListItem ordered=false number=0 task=none indent=0 checkbox=- display=\"foo\" strike=- link=- map=ok
star[1] Paragraph display=\"  *\" strike=- link=- map=ok
star[2] Empty display=\"\" strike=- link=- map=ok"
    );
    assert_eq!(
        observe_doc("one", "- foo\n  1.\n"),
        "\
one[0] ListItem ordered=false number=0 task=none indent=0 checkbox=- display=\"foo\" strike=- link=- map=ok
one[1] Paragraph display=\"  1.\" strike=- link=- map=ok
one[2] Empty display=\"\" strike=- link=- map=ok"
    );
    assert_eq!(
        observe_doc("two", "- foo\n  bar\n  2) baz\n"),
        "\
two[0] ListItem ordered=false number=0 task=none indent=0 checkbox=- display=\"foo\" strike=- link=- map=ok
two[1] Paragraph display=\"  bar\" strike=- link=- map=ok
two[2] Paragraph display=\"  2) baz\" strike=- link=- map=ok
two[3] Empty display=\"\" strike=- link=- map=ok"
    );
    assert_eq!(
        observe_doc("fourteen", "- parent\n  14) a\n"),
        "\
fourteen[0] ListItem ordered=false number=0 task=none indent=0 checkbox=- display=\"parent\" strike=- link=- map=ok
fourteen[1] Paragraph display=\"  14) a\" strike=- link=- map=ok
fourteen[2] Empty display=\"\" strike=- link=- map=ok"
    );
    assert_eq!(
        observe_doc("checked", "- parent\n  14) [x] a\n"),
        "\
checked[0] ListItem ordered=false number=0 task=none indent=0 checkbox=- display=\"parent\" strike=- link=- map=ok
checked[1] Paragraph display=\"  14) [x] a\" strike=- link=- map=ok
checked[2] Empty display=\"\" strike=- link=- map=ok"
    );
    assert_eq!(
        observe_doc("blank", "- parent\n\n  *\n"),
        "\
blank[0] ListItem ordered=false number=0 task=none indent=0 checkbox=- display=\"parent\" strike=- link=- map=ok
blank[1] Empty display=\"\" strike=- link=- map=ok
blank[2] ListItem ordered=false number=0 task=none indent=2 checkbox=- display=\"\" strike=- link=- map=ok
blank[3] Empty display=\"\" strike=- link=- map=ok"
    );
    assert_eq!(
        observe_doc("start", "- foo\n  1. x\n"),
        "\
start[0] ListItem ordered=false number=0 task=none indent=0 checkbox=- display=\"foo\" strike=- link=- map=ok
start[1] ListItem ordered=true number=1 task=none indent=2 checkbox=- display=\"x\" strike=- link=- map=ok
start[2] Empty display=\"\" strike=- link=- map=ok"
    );
    assert_eq!(
        observe_doc("after", "- parent\n\n  14) a\n"),
        "\
after[0] ListItem ordered=false number=0 task=none indent=0 checkbox=- display=\"parent\" strike=- link=- map=ok
after[1] Empty display=\"\" strike=- link=- map=ok
after[2] ListItem ordered=true number=14 task=none indent=2 checkbox=- display=\"a\" strike=- link=- map=ok
after[3] Empty display=\"\" strike=- link=- map=ok"
    );
}

/// Laziness (spec lines 4814–4821): `bar` continues the item, and `2. baz`
/// then starts a new list. `foo` then `2. baz` stays one paragraph
/// (example 304).
#[test]
fn ordered_marker_after_dedent_stays_list() {
    assert_eq!(
        observe_doc("lazy", "- foo\nbar\n2. baz\n"),
        "\
lazy[0] ListItem ordered=false number=0 task=none indent=0 checkbox=- display=\"foo\" strike=- link=- map=ok
lazy[1] Paragraph display=\"bar\" strike=- link=- map=ok
lazy[2] ListItem ordered=true number=2 task=none indent=0 checkbox=- display=\"baz\" strike=- link=- map=ok
lazy[3] Empty display=\"\" strike=- link=- map=ok"
    );
    assert_eq!(
        observe_doc("para", "foo\n2. baz\n"),
        "\
para[0] Paragraph display=\"foo\" strike=- link=- map=ok
para[1] Paragraph display=\"2. baz\" strike=- link=- map=ok
para[2] Empty display=\"\" strike=- link=- map=ok"
    );
}

/// Example 283 with CR LF: `2.\r` is an empty item, so `3. bar` stays item 3.
#[test]
fn crlf_empty_item_does_not_drop_next_item() {
    assert_eq!(
        observe_doc("crlf", "1. foo\r\n2.\r\n3. bar"),
        "\
crlf[0] ListItem ordered=true number=1 task=none indent=0 checkbox=- display=\"foo\" strike=- link=- map=ok
crlf[1] ListItem ordered=true number=2 task=none indent=0 checkbox=- display=\"\" strike=- link=- map=ok
crlf[2] ListItem ordered=true number=3 task=none indent=0 checkbox=- display=\"bar\" strike=- link=- map=ok"
    );
    assert_eq!(
        observe_doc("bare", "1.\rfoo"),
        "\
bare[0] ListItem ordered=true number=1 task=none indent=0 checkbox=- display=\"\" strike=- link=- map=ok
bare[1] Paragraph display=\"foo\" strike=- link=- map=ok"
    );
}

/// Exact `---`, `***`, and `___` are rules, including after HTML, which is
/// BASE (no HTML-block guard). Every other shape is the paragraph or list
/// BASE already parses.
#[test]
fn extended_rules_inside_html_stay_visible_text() {
    let outside_src = "Foo\n****\n";
    let outside = parse_document(outside_src);
    assert!(matches!(outside.lines[1].kind, LineKind::Paragraph));
    assert_eq!(outside.lines[1].display_text, "****");
    assert_eq!(&outside_src[outside.lines[1].source.clone()], "****");
    let kept = "- item\n<br>\n---\n";
    let list_rule = parse_document(kept);
    assert!(matches!(list_rule.lines[2].kind, LineKind::HorizontalRule));
    assert_eq!(list_rule.lines[2].display_text, "");
    assert_eq!(&kept[list_rule.lines[2].source.clone()], "---");
    let indented = "- item\n  <div>\n---\n";
    let indented_rule = parse_document(indented);
    assert!(matches!(
        indented_rule.lines[2].kind,
        LineKind::HorizontalRule
    ));
    assert_eq!(indented_rule.lines[2].display_text, "");
    assert_eq!(&indented[indented_rule.lines[2].source.clone()], "---");
    let indented_blank = "- item\n\n  <div>\n---\n";
    let blank_rule = parse_document(indented_blank);
    assert!(matches!(blank_rule.lines[3].kind, LineKind::HorizontalRule));
    assert_eq!(blank_rule.lines[3].display_text, "");
    assert_eq!(&indented_blank[blank_rule.lines[3].source.clone()], "---");
    let div_rule = "<div>\n---\n";
    let div_doc = parse_document(div_rule);
    assert_eq!(&div_rule[div_doc.lines[1].source.clone()], "---");
    assert_eq!(div_doc.lines[1].display_text, "");
    assert!(matches!(div_doc.lines[1].kind, LineKind::HorizontalRule));

    let cases = [
        ("<pre>\n****\n", 1, "****"),
        ("<div>\n****\n", 1, "****"),
        ("<!--\n****\n", 1, "****"),
        ("<script>\n****\n", 1, "****"),
        ("<pre>\n____\n", 1, "____"),
        ("<pre>\n_ _ _\n", 1, "_ _ _"),
        ("<pre>\n\n****\n", 2, "****"),
        ("Para\n<pre>\n****\n", 2, "****"),
        ("<pre>\n   ****\n", 1, "   ****"),
        ("<pre>\r****\r", 1, "****"),
    ];
    for (src, index, display) in cases {
        let doc = parse_document(src);
        let line = &doc.lines[index];
        assert_eq!(&src[line.source.clone()], display, "{src:?}");
        assert_eq!(line.display_text, display, "{src:?}");
        assert!(
            matches!(line.kind, LineKind::Paragraph),
            "{src:?} kind {:?}",
            line.kind
        );
    }

    let stars = "<pre>\n* * *\n";
    let star_doc = parse_document(stars);
    assert_eq!(&stars[star_doc.lines[1].source.clone()], "* * *");
    assert_eq!(star_doc.lines[1].display_text, "* *");
    assert!(matches!(
        star_doc.lines[1].kind,
        LineKind::ListItem {
            ordered: false,
            indent_cols: 0,
            ..
        }
    ));
    let dashes = "<pre>\n- - -\n";
    let dash_doc = parse_document(dashes);
    assert_eq!(&dashes[dash_doc.lines[1].source.clone()], "- - -");
    assert_eq!(dash_doc.lines[1].display_text, "- -");
    assert!(matches!(
        dash_doc.lines[1].kind,
        LineKind::ListItem {
            ordered: false,
            indent_cols: 0,
            ..
        }
    ));
    let tabs = "<pre>\n*\t*\t*\n";
    let tab_doc = parse_document(tabs);
    assert_eq!(&tabs[tab_doc.lines[1].source.clone()], "*\t*\t*");
    assert_eq!(tab_doc.lines[1].display_text, "*\t*");
    assert!(matches!(
        tab_doc.lines[1].kind,
        LineKind::ListItem {
            ordered: false,
            indent_cols: 0,
            ..
        }
    ));
}

/// R6-9. Four spaces before `---` is indented code once a bare CR is a
/// line break, never a rule. Pulldown, which does not split CR,
/// returns one code block for the unsplit bytes. The editor shows `---`
/// as a code line, then paragraph `next`.
#[test]
fn four_space_dash_rule_after_bare_cr_is_code() {
    let src = "    ---\rnext\r";
    let doc = parse_document(src);
    assert!(
        !matches!(doc.lines[0].kind, LineKind::HorizontalRule),
        "kind {:?}",
        doc.lines[0].kind
    );
    assert!(matches!(doc.lines[0].kind, LineKind::CodeLine));
    assert_eq!(doc.lines[0].display_text, "---");
    assert_eq!(&src[doc.lines[0].source.clone()], "    ---");
    assert!(matches!(doc.lines[1].kind, LineKind::Paragraph));
    assert_eq!(doc.lines[1].display_text, "next");
    assert_eq!(&src[doc.lines[1].source.clone()], "next");
}

/// A line that stays text despite four columns of indent (a paragraph or
/// quote continuation, list content) parses every inline format, not
/// only the `**` that the flanking fallback used to catch.
#[test]
fn four_space_text_lines_parse_every_inline_format() {
    for src in [
        "foo\n    <u>x</u> `y` ~~z~~ *w*",
        "- item\n    <u>x</u> `y` ~~z~~ *w*",
        "> q\n    <u>x</u> `y` ~~z~~ *w*",
        "foo\n\t<u>x</u> `y` ~~z~~ *w*",
    ] {
        let doc = parse_document(src);
        let line = &doc.lines[1];
        assert!(
            matches!(line.kind, LineKind::Paragraph),
            "{src:?}: {:?}",
            line.kind
        );
        let flags = |text: &str| {
            line.styled_segments
                .iter()
                .find(|s| &src[s.source.clone()] == text)
                .map(|s| (s.underline, s.code, s.strike, s.italic))
        };
        assert_eq!(flags("x"), Some((true, false, false, false)), "{src:?}");
        assert_eq!(flags("y"), Some((false, true, false, false)), "{src:?}");
        assert_eq!(flags("z"), Some((false, false, true, false)), "{src:?}");
        assert_eq!(flags("w"), Some((false, false, false, true)), "{src:?}");
    }
}

/// A blank line leaves the item open. Four spaces is two past `- `'s
/// content column, so the line is a rule. The same bytes at document
/// level are indented code.
#[test]
fn four_space_rule_inside_an_open_list_is_a_rule() {
    let top = "    ---\nnext\n";
    let top_doc = parse_document(top);
    assert!(matches!(top_doc.lines[0].kind, LineKind::CodeLine));
    assert_eq!(top_doc.lines[0].display_text, "---");

    let cases = [
        ("- item\n\n    ---\n", 2),
        ("1. item\n\n    ---\n", 2),
        ("10) item\n\n    ---\n", 2),
        ("- [ ] item\n\n    ---\n", 2),
        ("1. [ ] item\n\n    ---\n", 2),
        ("- a\n  - b\n\n    ---\n", 3),
        ("- a\n  - b\n    <!DOCTYPE html>\n    ---\n", 3),
    ];
    for (src, index) in cases {
        let doc = parse_document(src);
        assert!(
            matches!(doc.lines[index].kind, LineKind::HorizontalRule),
            "{src:?} kind {:?}",
            doc.lines[index].kind
        );
        assert_eq!(doc.lines[index].display_text, "", "{src:?}");
        assert_eq!(&src[doc.lines[index].source.clone()], "    ---", "{src:?}");
    }
}

/// K1. A tab inside an open list is a column stop of 4, same as BASE
/// `leading_column`. Extra indent past the item content column is 2, so
/// the line is a rule. A tab at column 0 is indented code.
#[test]
fn tab_rule_inside_an_open_list_is_a_rule() {
    let top = "\t---\n";
    let top_doc = parse_document(top);
    assert!(matches!(top_doc.lines[0].kind, LineKind::CodeLine));
    assert_eq!(top_doc.lines[0].display_text, "---");
    assert_eq!(&top[top_doc.lines[0].source.clone()], "\t---");

    let cases = [
        ("- item\n\n\t---\n", 2, "\t---"),
        ("- item\n\t___\n", 1, "\t___"),
        ("* item\n\t***\n", 1, "\t***"),
    ];
    for (src, index, source) in cases {
        let doc = parse_document(src);
        assert!(
            matches!(doc.lines[index].kind, LineKind::HorizontalRule),
            "{src:?} kind {:?}",
            doc.lines[index].kind
        );
        assert_eq!(doc.lines[index].display_text, "", "{src:?}");
        assert_eq!(&src[doc.lines[index].source.clone()], source, "{src:?}");
    }
}

/// BASE `1d03313` display of `# foo #\t` is `foo #\t` (the tab stays, and
/// the hash is text). `#\t#` stays a heading whose display is `#`.
#[test]
fn tab_after_a_closing_hash_stays_in_the_heading_text() {
    let doc = parse_document("# foo #\t");
    assert!(matches!(doc.lines[0].kind, LineKind::Heading { level: 1 }));
    assert_eq!(doc.lines[0].display_text, "foo #\t");

    let kept = parse_document("# foo\t#");
    assert_eq!(kept.lines[0].display_text, "foo\t#");

    let trailed = parse_document("# foo\t");
    assert_eq!(trailed.lines[0].display_text, "foo\t");

    let spaces = parse_document("# foo #   ");
    assert_eq!(spaces.lines[0].display_text, "foo");

    let empty = parse_document("### ###");
    assert_eq!(empty.lines[0].display_text, "");

    let tab_hash = parse_document("#\t#");
    assert!(matches!(
        tab_hash.lines[0].kind,
        LineKind::Heading { level: 1 }
    ));
    assert_eq!(tab_hash.lines[0].display_text, "#");
}
fn shape_of(markdown: &str) -> String {
    let doc = parse_document(markdown);
    let mut out = String::new();
    for (index, line) in doc.lines.iter().enumerate() {
        let code = line
            .styled_segments
            .iter()
            .filter(|segment| segment.code)
            .map(|segment| markdown[segment.source.clone()].to_string())
            .collect::<Vec<_>>();
        let body = match &line.kind {
            LineKind::Empty => "Empty".to_string(),
            LineKind::Paragraph => "Paragraph".to_string(),
            LineKind::Heading { level } => format!("Heading level={level}"),
            LineKind::Blockquote => "Blockquote".to_string(),
            LineKind::ListItem { .. } => "ListItem".to_string(),
            LineKind::CodeFence { .. } => "CodeFence".to_string(),
            LineKind::CodeLine => "CodeLine".to_string(),
            LineKind::HorizontalRule => "HorizontalRule".to_string(),
            LineKind::TableDelimiter => "TableDelimiter".to_string(),
            LineKind::TableRow {
                is_header,
                ref body,
                ..
            } => {
                let TableRowBody {
                    cells, alignments, ..
                } = &**body;
                let texts = cells
                    .iter()
                    .map(|range| markdown[range.clone()].to_string())
                    .collect::<Vec<_>>();
                let align = alignments
                    .iter()
                    .map(|align| match align {
                        TableColumnAlign::None => "none",
                        TableColumnAlign::Left => "left",
                        TableColumnAlign::Center => "center",
                        TableColumnAlign::Right => "right",
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                format!("TableRow header={is_header} align=[{align}] cells={texts:?}")
            }
        };
        out.push_str(&format!(
            "line[{index}] {body} display={:?} code={code:?}\n",
            line.display_text
        ));
    }
    out
}

fn shape_mismatch(name: &str, markdown: &str, lines: &[&str]) -> Option<String> {
    let mut expected = String::new();
    for line in lines {
        expected.push_str(line);
        expected.push('\n');
    }
    let got = shape_of(markdown);
    if got == expected {
        None
    } else {
        Some(format!(
            "case {name}\n--- expected ---\n{expected}--- got ---\n{got}"
        ))
    }
}

#[test]
fn gfm_table_columns_follow_the_delimiter() {
    let mut mismatches = Vec::new();
    let mut check = |name: &str, markdown: &str, lines: &[&str]| {
        if let Some(mismatch) = shape_mismatch(name, markdown, lines) {
            mismatches.push(mismatch);
        }
    };
    check(
        "gfm198",
        "| foo | bar |\n| --- | --- |\n| baz | bim |",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["foo", "bar"] display="| foo | bar |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["baz", "bim"] display="| baz | bim |" code=[]"#,
        ],
    );
    check(
        "gfm199",
        "| abc | defghi |\n:-: | -----------:\nbar | baz",
        &[
            r#"line[0] TableRow header=true align=[center,right] cells=["abc", "defghi"] display="| abc | defghi |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[center,right] cells=["bar", "baz"] display="bar | baz" code=[]"#,
        ],
    );
    check(
        "gfm200",
        "| f\\|oo  |\n| ------ |\n| b `\\|` az |\n| b **\\|** im |",
        &[
            r#"line[0] TableRow header=true align=[none] cells=["f\\|oo "] display="| f\\|oo  |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none] cells=["b `\\|` az"] display="| b `\\|` az |" code=["\\|"]"#,
            r#"line[3] TableRow header=false align=[none] cells=["b **\\|** im"] display="| b **\\|** im |" code=[]"#,
        ],
    );
    check(
        "gfm203",
        "| abc | def |\n| --- |\n| bar |",
        &[
            r#"line[0] Paragraph display="| abc | def |" code=[]"#,
            r#"line[1] Paragraph display="| --- |" code=[]"#,
            r#"line[2] Paragraph display="| bar |" code=[]"#,
        ],
    );
    check(
        "gfm204",
        "| abc | def |\n| --- | --- |\n| bar |\n| bar | baz | boo |",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["abc", "def"] display="| abc | def |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["bar", ""] display="| bar |" code=[]"#,
            r#"line[3] TableRow header=false align=[none,none] cells=["bar", "baz"] display="| bar | baz | boo |" code=[]"#,
        ],
    );
    check(
        "code-pipe",
        "| a | b |\n| --- | --- |\n| `c|d` | e |",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["`c|d`", "e"] display="| `c|d` | e |" code=["c|d"]"#,
        ],
    );
    check(
        "pipe-row-no-delim",
        "| a | b |\n| c | d |",
        &[
            r#"line[0] Paragraph display="| a | b |" code=[]"#,
            r#"line[1] Paragraph display="| c | d |" code=[]"#,
        ],
    );
    drop(check);
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}

fn align_label(align: &TableColumnAlign) -> &'static str {
    match align {
        TableColumnAlign::None => "none",
        TableColumnAlign::Left => "left",
        TableColumnAlign::Center => "center",
        TableColumnAlign::Right => "right",
    }
}

fn pulldown_align(align: &pulldown_cmark::Alignment) -> &'static str {
    match align {
        pulldown_cmark::Alignment::None => "none",
        pulldown_cmark::Alignment::Left => "left",
        pulldown_cmark::Alignment::Center => "center",
        pulldown_cmark::Alignment::Right => "right",
    }
}

fn exporter_table_options() -> pulldown_cmark::Options {
    let mut options = pulldown_cmark::Options::empty();
    options.insert(pulldown_cmark::Options::ENABLE_TABLES);
    options.insert(pulldown_cmark::Options::ENABLE_FOOTNOTES);
    options.insert(pulldown_cmark::Options::ENABLE_STRIKETHROUGH);
    options.insert(pulldown_cmark::Options::ENABLE_TASKLISTS);
    options
}

/// Column count, body-plus-header rows, alignments, and the in-table line mask.
fn editor_table_membership(markdown: &str) -> (String, Vec<bool>) {
    let doc = parse_document(markdown);
    let mut tables = Vec::new();
    let mut mask = Vec::new();
    let mut cols = 0usize;
    let mut rows = 0usize;
    let mut align = String::new();
    let mut in_table = false;
    for line in &doc.lines {
        let table_line = matches!(
            line.kind,
            LineKind::TableRow { .. } | LineKind::TableDelimiter
        );
        mask.push(table_line);
        if table_line && !in_table {
            in_table = true;
            rows = 0;
        }
        if let LineKind::TableRow { ref body, .. } = &line.kind {
            let TableRowBody { alignments, .. } = &**body;
            cols = alignments.len();
            rows += 1;
            align = alignments
                .iter()
                .map(align_label)
                .collect::<Vec<_>>()
                .join(",");
        }
        if in_table && !table_line {
            tables.push(format!("cols={cols} rows={rows} align={align}"));
            in_table = false;
        }
    }
    if in_table {
        tables.push(format!("cols={cols} rows={rows} align={align}"));
    }
    let sig = if tables.is_empty() {
        "none".to_string()
    } else {
        tables.join(" | ")
    };
    (sig, mask)
}

fn pulldown_table_membership(markdown: &str) -> (String, Vec<bool>) {
    let doc = parse_document(markdown);
    let mut mask = vec![false; doc.lines.len()];
    let mut tables = Vec::new();
    let mut current: Option<(usize, String, usize)> = None;
    let events =
        pulldown_cmark::Parser::new_ext(markdown, exporter_table_options()).into_offset_iter();
    for (event, range) in events {
        match event {
            pulldown_cmark::Event::Start(pulldown_cmark::Tag::Table(aligns)) => {
                let align = aligns
                    .iter()
                    .map(pulldown_align)
                    .collect::<Vec<_>>()
                    .join(",");
                current = Some((aligns.len(), align, 0));
                for (index, line) in doc.lines.iter().enumerate() {
                    if line.source.start < range.end && line.source.end > range.start {
                        mask[index] = true;
                    }
                }
            }
            pulldown_cmark::Event::Start(pulldown_cmark::Tag::TableHead)
            | pulldown_cmark::Event::Start(pulldown_cmark::Tag::TableRow) => {
                if let Some(table) = current.as_mut() {
                    table.2 += 1;
                }
            }
            pulldown_cmark::Event::End(pulldown_cmark::TagEnd::Table) => {
                if let Some((cols, align, rows)) = current.take() {
                    tables.push(format!("cols={cols} rows={rows} align={align}"));
                }
            }
            _ => {}
        }
    }
    let sig = if tables.is_empty() {
        "none".to_string()
    } else {
        tables.join(" | ")
    };
    (sig, mask)
}

/// Exact kinds and cells for the checker cases plus GFM examples 198–205.
/// pulldown-cmark 0.12.2 with the exporter options is the oracle for which
/// lines belong to a table, and for the column count and alignment.
/// `double-backslash-pipe` stays a table: GFM says "If a backslash is itself
/// escaped, the following character is not", so an even run leaves a real pipe.
/// A delimiter indented four or more columns does not open a table.
#[test]
fn table_membership_kinds_and_cells() {
    let mut mismatches = Vec::new();
    let mut check = |name: &str, markdown: &str, lines: &[&str], preserved: bool| {
        if let Some(mismatch) = shape_mismatch(name, markdown, lines) {
            mismatches.push(mismatch);
        }
        let editor = editor_table_membership(markdown);
        let pulldown = pulldown_table_membership(markdown);
        let same = editor == pulldown;
        if preserved && same {
            mismatches.push(format!(
                "case {name} preserved divergence now matches pulldown\neditor {:?} {:?}\npulldown {:?} {:?}",
                editor.0, editor.1, pulldown.0, pulldown.1
            ));
        } else if !preserved && !same {
            mismatches.push(format!(
                "case {name} membership diverges from pulldown\neditor {:?} {:?}\npulldown {:?} {:?}",
                editor.0, editor.1, pulldown.0, pulldown.1
            ));
        }
    };
    check(
        "gfm198",
        "| foo | bar |\n| --- | --- |\n| baz | bim |",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["foo", "bar"] display="| foo | bar |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["baz", "bim"] display="| baz | bim |" code=[]"#,
        ],
        false,
    );
    check(
        "gfm199",
        "| abc | defghi |\n:-: | -----------:\nbar | baz",
        &[
            r#"line[0] TableRow header=true align=[center,right] cells=["abc", "defghi"] display="| abc | defghi |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[center,right] cells=["bar", "baz"] display="bar | baz" code=[]"#,
        ],
        false,
    );
    check(
        "gfm200",
        "| f\\|oo  |\n| ------ |\n| b `\\|` az |\n| b **\\|** im |",
        &[
            r#"line[0] TableRow header=true align=[none] cells=["f\\|oo "] display="| f\\|oo  |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none] cells=["b `\\|` az"] display="| b `\\|` az |" code=["\\|"]"#,
            r#"line[3] TableRow header=false align=[none] cells=["b **\\|** im"] display="| b **\\|** im |" code=[]"#,
        ],
        false,
    );
    check(
        "gfm201",
        "| abc | def |\n| --- | --- |\n| bar | baz |\n> bar",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["abc", "def"] display="| abc | def |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["bar", "baz"] display="| bar | baz |" code=[]"#,
            r#"line[3] Blockquote display="bar" code=[]"#,
        ],
        false,
    );
    check(
        "gfm202",
        "| abc | def |\n| --- | --- |\n| bar | baz |\nbar\n\nbar",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["abc", "def"] display="| abc | def |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["bar", "baz"] display="| bar | baz |" code=[]"#,
            r#"line[3] TableRow header=false align=[none,none] cells=["bar", ""] display="bar" code=[]"#,
            r#"line[4] Empty display="" code=[]"#,
            r#"line[5] Paragraph display="bar" code=[]"#,
        ],
        false,
    );
    check(
        "gfm203",
        "| abc | def |\n| --- |\n| bar |",
        &[
            r#"line[0] Paragraph display="| abc | def |" code=[]"#,
            r#"line[1] Paragraph display="| --- |" code=[]"#,
            r#"line[2] Paragraph display="| bar |" code=[]"#,
        ],
        false,
    );
    check(
        "gfm204",
        "| abc | def |\n| --- | --- |\n| bar |\n| bar | baz | boo |",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["abc", "def"] display="| abc | def |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["bar", ""] display="| bar |" code=[]"#,
            r#"line[3] TableRow header=false align=[none,none] cells=["bar", "baz"] display="| bar | baz | boo |" code=[]"#,
        ],
        false,
    );
    check(
        "gfm205",
        "| abc | def |\n| --- | --- |",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["abc", "def"] display="| abc | def |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
        ],
        false,
    );
    check(
        "code-pipe",
        "| a | b |\n| --- | --- |\n| `c|d` | e |",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["`c|d`", "e"] display="| `c|d` | e |" code=["c|d"]"#,
        ],
        false,
    );
    check(
        "no-delim",
        "| a | b |\n| c | d |",
        &[
            r#"line[0] Paragraph display="| a | b |" code=[]"#,
            r#"line[1] Paragraph display="| c | d |" code=[]"#,
        ],
        false,
    );
    check(
        "delim-wider",
        "| a | b |\n| --- | --- | --- |\n| x | y | z |",
        &[
            r#"line[0] Paragraph display="| a | b |" code=[]"#,
            r#"line[1] Paragraph display="| --- | --- | --- |" code=[]"#,
            r#"line[2] Paragraph display="| x | y | z |" code=[]"#,
        ],
        false,
    );
    check(
        "header-wider",
        "| a | b | c |\n| --- | --- |\n| x | y |",
        &[
            r#"line[0] Paragraph display="| a | b | c |" code=[]"#,
            r#"line[1] Paragraph display="| --- | --- |" code=[]"#,
            r#"line[2] Paragraph display="| x | y |" code=[]"#,
        ],
        false,
    );
    check(
        "one-col",
        "| a |\n| --- |\n| b |",
        &[
            r#"line[0] TableRow header=true align=[none] cells=["a"] display="| a |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none] cells=["b"] display="| b |" code=[]"#,
        ],
        false,
    );
    check(
        "delim-no-pipe-dash",
        "foo\n---\nbar",
        &[
            r#"line[0] Paragraph display="foo" code=[]"#,
            r#"line[1] HorizontalRule display="" code=[]"#,
            r#"line[2] Paragraph display="bar" code=[]"#,
        ],
        false,
    );
    check(
        "delim-no-pipe-colon",
        "foo\n:-:\nbar",
        &[
            r#"line[0] Paragraph display="foo" code=[]"#,
            r#"line[1] Paragraph display=":-:" code=[]"#,
            r#"line[2] Paragraph display="bar" code=[]"#,
        ],
        false,
    );
    check(
        "piped-header-bare-colon",
        "| foo |\n:-:\nbar",
        &[
            r#"line[0] Paragraph display="| foo |" code=[]"#,
            r#"line[1] Paragraph display=":-:" code=[]"#,
            r#"line[2] Paragraph display="bar" code=[]"#,
        ],
        false,
    );
    check(
        "pipe-only-header",
        "|\n| --- | --- |\n| a | b |",
        &[
            r#"line[0] Paragraph display="|" code=[]"#,
            r#"line[1] Paragraph display="| --- | --- |" code=[]"#,
            r#"line[2] Paragraph display="| a | b |" code=[]"#,
        ],
        false,
    );
    check(
        "pipe-only-body",
        "| a | b |\n| --- | --- |\n| c | d |\n|\n| e | f |",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
            r#"line[3] Paragraph display="|" code=[]"#,
            r#"line[4] Paragraph display="| e | f |" code=[]"#,
        ],
        false,
    );
    check(
        "pipe-only-tail",
        "| a | b |\n| --- | --- |\n|",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] Paragraph display="|" code=[]"#,
        ],
        false,
    );
    check(
        "double-pipe-body",
        "| a | b |\n| --- | --- |\n||",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["", ""] display="||" code=[]"#,
        ],
        false,
    );
    check(
        "empty-cells",
        "| a | | b |\n| --- | --- | --- |\n| | x | |",
        &[
            r#"line[0] TableRow header=true align=[none,none,none] cells=["a", "", "b"] display="| a | | b |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none,none] cells=["", "x", ""] display="| | x | |" code=[]"#,
        ],
        false,
    );
    check(
        "space-only-cells",
        "| a |     |\n| --- | --- |\n| b | c |",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["a", "   "] display="| a |     |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["b", "c"] display="| b | c |" code=[]"#,
        ],
        false,
    );
    check(
        "tab-only-cells",
        "| \tb | \t |\n| --- | --- |\n| c | d |",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["\tb", "\t"] display="| \tb | \t |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
        ],
        false,
    );
    check(
        "escaped-pipe",
        "| a \\| b | c |\n| --- | --- |\n| d | e |",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["a \\| b", "c"] display="| a \\| b | c |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["d", "e"] display="| d | e |" code=[]"#,
        ],
        false,
    );
    check(
        "double-backslash-pipe",
        "| a \\\\| b |\n| --- | --- |\n| c | d |",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["a \\\\", "b"] display="| a \\\\| b |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
        ],
        true,
    );
    check(
        "unequal-backticks-body",
        "| a | b |\n| --- | --- |\n| `c|d`` | e |",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["`c", "d``"] display="| `c|d`` | e |" code=[]"#,
        ],
        false,
    );
    check(
        "unequal-backticks-header",
        "| `a|b`` | c |\n| --- | --- |\n| d | e |",
        &[
            r#"line[0] Paragraph display="| `a|b`` | c |" code=[]"#,
            r#"line[1] Paragraph display="| --- | --- |" code=[]"#,
            r#"line[2] Paragraph display="| d | e |" code=[]"#,
        ],
        false,
    );
    check(
        "unclosed-code",
        "| `a|b | c |\n| --- | --- |\n| d | e |",
        &[
            r#"line[0] Paragraph display="| `a|b | c |" code=[]"#,
            r#"line[1] Paragraph display="| --- | --- |" code=[]"#,
            r#"line[2] Paragraph display="| d | e |" code=[]"#,
        ],
        false,
    );
    check(
        "double-backtick-pipe",
        "| a | b |\n| --- | --- |\n| ``c|d`` | e |",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["``c|d``", "e"] display="| ``c|d`` | e |" code=["c|d"]"#,
        ],
        false,
    );
    check(
        "blank-ends",
        "| a | b |\n| --- | --- |\n| c | d |\n\npara",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
            r#"line[3] Empty display="" code=[]"#,
            r#"line[4] Paragraph display="para" code=[]"#,
        ],
        false,
    );
    check(
        "plain-row",
        "| a | b |\n| --- | --- |\nhello",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["hello", ""] display="hello" code=[]"#,
        ],
        false,
    );
    check(
        "list-ends",
        "| a | b |\n| --- | --- |\n| c | d |\n- item",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
            r#"line[3] ListItem display="item" code=[]"#,
        ],
        false,
    );
    check(
        "quote-ends",
        "| a | b |\n| --- | --- |\n| c | d |\n> bar",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
            r#"line[3] Blockquote display="bar" code=[]"#,
        ],
        false,
    );
    check(
        "quote-no-space",
        "| a | b |\n| --- | --- |\n| c | d |\n>bar",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
            r#"line[3] Paragraph display=">bar" code=[]"#,
        ],
        false,
    );
    check(
        "fence-ends",
        "| a | b |\n| --- | --- |\n| c | d |\n```\ncode\n```",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
            r#"line[3] CodeFence display="" code=[]"#,
            r#"line[4] CodeLine display="code" code=["code"]"#,
            r#"line[5] CodeFence display="" code=[]"#,
        ],
        false,
    );
    // pulldown 0.12.2: code-open: / code-line / code-close.
    // HTML after the table: <pre><code>code\n</code></pre>
    // The tilde fence ends the table and holds `code`, as pulldown does.
    check(
        "tilde-fence",
        "| a | b |\n| --- | --- |\n| c | d |\n~~~\ncode\n~~~",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
            r#"line[3] CodeFence display="" code=[]"#,
            r#"line[4] CodeLine display="code" code=["code"]"#,
            r#"line[5] CodeFence display="" code=[]"#,
        ],
        false,
    );
    check(
        "heading-ends",
        "| a | b |\n| --- | --- |\n| c | d |\n# heading",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
            r#"line[3] Heading level=1 display="heading" code=[]"#,
        ],
        false,
    );
    check(
        "hr-exact",
        "| a | b |\n| --- | --- |\n| c | d |\n---",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
            r#"line[3] HorizontalRule display="" code=[]"#,
        ],
        false,
    );
    check(
        "hr-four-dash",
        "| a | b |\n| --- | --- |\n| c | d |\n----",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
            r#"line[3] Paragraph display="----" code=[]"#,
        ],
        false,
    );
    check(
        "hr-four-star",
        "| a | b |\n| --- | --- |\n| c | d |\n****",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
            r#"line[3] Paragraph display="****" code=[]"#,
        ],
        false,
    );
    check(
        "hr-four-under",
        "| a | b |\n| --- | --- |\n| c | d |\n____",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
            r#"line[3] Paragraph display="____" code=[]"#,
        ],
        false,
    );
    check(
        "hr-spaced-dash",
        "| a | b |\n| --- | --- |\n| c | d |\n- - -",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
            r#"line[3] ListItem display="- -" code=[]"#,
        ],
        false,
    );
    check(
        "hr-spaced-star",
        "| a | b |\n| --- | --- |\n| c | d |\n* * *",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
            r#"line[3] ListItem display="* *" code=[]"#,
        ],
        false,
    );
    check(
        "hr-star-pairs",
        "| a | b |\n| --- | --- |\n| c | d |\n** ** **",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
            r#"line[3] Paragraph display="** ** **" code=[]"#,
        ],
        false,
    );
    check(
        "hr-indented-four-dash",
        "| a | b |\n| --- | --- |\n| c | d |\n   ----",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
            r#"line[3] Paragraph display="   ----" code=[]"#,
        ],
        false,
    );
    // Main builds a ListItem for a paren ordered marker.
    check(
        "paren-list",
        "| a | b |\n| --- | --- |\n| c | d |\n1) item",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
            r#"line[3] ListItem display="item" code=[]"#,
        ],
        false,
    );
    check(
        "html-div",
        "| a | b |\n| --- | --- |\n| c | d |\n<div>",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
            r#"line[3] Paragraph display="<div>" code=[]"#,
        ],
        false,
    );
    check(
        "html-pre",
        "| a | b |\n| --- | --- |\n| c | d |\n<pre>",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
            r#"line[3] Paragraph display="<pre>" code=[]"#,
        ],
        false,
    );
    check(
        "html-comment",
        "| a | b |\n| --- | --- |\n| c | d |\n<!--",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
            r#"line[3] Paragraph display="<!--" code=[]"#,
        ],
        false,
    );
    check(
        "html-pi",
        "| a | b |\n| --- | --- |\n| c | d |\n<?",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
            r#"line[3] Paragraph display="<?" code=[]"#,
        ],
        false,
    );
    check(
        "html-decl",
        "| a | b |\n| --- | --- |\n| c | d |\n<!DOCTYPE html>",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
            r#"line[3] Paragraph display="<!DOCTYPE html>" code=[]"#,
        ],
        false,
    );
    check(
        "html-cdata",
        "| a | b |\n| --- | --- |\n| c | d |\n<![CDATA[",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
            r#"line[3] Paragraph display="<![CDATA[" code=[]"#,
        ],
        false,
    );
    check(
        "indent4-table",
        "    | a | b |\n    | --- | --- |\n    | c | d |",
        &[
            r#"line[0] CodeLine display="| a | b |" code=["| a | b |"]"#,
            r#"line[1] CodeLine display="| --- | --- |" code=["| --- | --- |"]"#,
            r#"line[2] CodeLine display="| c | d |" code=["| c | d |"]"#,
        ],
        false,
    );
    check(
        "indent4-delim",
        "| a | b |\n    | --- | --- |\n| c | d |",
        &[
            r#"line[0] Paragraph display="| a | b |" code=[]"#,
            r#"line[1] Paragraph display="    | --- | --- |" code=[]"#,
            r#"line[2] Paragraph display="| c | d |" code=[]"#,
        ],
        false,
    );
    check(
        "indent3-table",
        "   | a | b |\n   | --- | --- |\n   | c | d |",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="   | a | b |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="   | c | d |" code=[]"#,
        ],
        false,
    );
    check(
        "crlf",
        "| a | b |\r\n| --- | --- |\r\n| c | d |",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
        ],
        false,
    );
    check(
        "multibyte",
        "| 你好 | 世界 |\n| --- | --- |\n| 😀 | ខ |",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["你好", "世界"] display="| 你好 | 世界 |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["😀", "ខ"] display="| 😀 | ខ |" code=[]"#,
        ],
        false,
    );
    check(
        "delim-inner-space",
        "| a | b |\n| : --- | --- : |\n| c | d |",
        &[
            r#"line[0] TableRow header=true align=[left,right] cells=["a", "b"] display="| a | b |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[left,right] cells=["c", "d"] display="| c | d |" code=[]"#,
        ],
        false,
    );
    check(
        "delim-tab",
        "| a | b |\n|\t---\t|\t---\t|\n| c | d |",
        &[
            r#"line[0] Paragraph display="| a | b |" code=[]"#,
            r#"line[1] Paragraph display="|\t---\t|\t---\t|" code=[]"#,
            r#"line[2] Paragraph display="| c | d |" code=[]"#,
        ],
        false,
    );
    check(
        "inside-fence",
        "```\n| a | b |\n| --- | --- |\n```",
        &[
            r#"line[0] CodeFence display="" code=[]"#,
            r#"line[1] CodeLine display="| a | b |" code=["| a | b |"]"#,
            r#"line[2] CodeLine display="| --- | --- |" code=["| --- | --- |"]"#,
            r#"line[3] CodeFence display="" code=[]"#,
        ],
        false,
    );
    check(
        "after-fence",
        "```\ncode\n```\n| a | b |\n| --- | --- |\n| c | d |",
        &[
            r#"line[0] CodeFence display="" code=[]"#,
            r#"line[1] CodeLine display="code" code=["code"]"#,
            r#"line[2] CodeFence display="" code=[]"#,
            r#"line[3] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
            r#"line[4] TableDelimiter display="" code=[]"#,
            r#"line[5] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
        ],
        false,
    );
    check(
        "unclosed-fence",
        "```\n| a | b |\n| --- | --- |\n| c | d |",
        &[
            r#"line[0] CodeFence display="" code=[]"#,
            r#"line[1] CodeLine display="| a | b |" code=["| a | b |"]"#,
            r#"line[2] CodeLine display="| --- | --- |" code=["| --- | --- |"]"#,
            r#"line[3] CodeLine display="| c | d |" code=["| c | d |"]"#,
        ],
        false,
    );
    check(
        "header-no-pipe",
        "hello\n| --- |\nworld",
        &[
            r#"line[0] Paragraph display="hello" code=[]"#,
            r#"line[1] Paragraph display="| --- |" code=[]"#,
            r#"line[2] Paragraph display="world" code=[]"#,
        ],
        false,
    );
    check(
        "header-code-only-pipe",
        "`a|b`\n| --- |\nc",
        &[
            r#"line[0] Paragraph display="a|b" code=["a|b"]"#,
            r#"line[1] Paragraph display="| --- |" code=[]"#,
            r#"line[2] Paragraph display="c" code=[]"#,
        ],
        false,
    );
    check(
        "align-four",
        "| a | b | c | d |\n| :--- | :---: | ---: | --- |\n| e | f | g | h |",
        &[
            r#"line[0] TableRow header=true align=[left,center,right,none] cells=["a", "b", "c", "d"] display="| a | b | c | d |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[left,center,right,none] cells=["e", "f", "g", "h"] display="| e | f | g | h |" code=[]"#,
        ],
        false,
    );
    check(
        "single-hyphen",
        "| a | b |\n| - | - |\n| c | d |",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
        ],
        false,
    );
    check(
        "delim-empty-cell",
        "| a | b |\n| --- || --- |\n| c | d |",
        &[
            r#"line[0] Paragraph display="| a | b |" code=[]"#,
            r#"line[1] Paragraph display="| --- || --- |" code=[]"#,
            r#"line[2] Paragraph display="| c | d |" code=[]"#,
        ],
        false,
    );
    check(
        "body-looks-like-delim",
        "| a | b |\n| --- | --- |\n| c | d |\n| --- | --- |",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
            r#"line[3] TableRow header=false align=[none,none] cells=["---", "---"] display="| --- | --- |" code=[]"#,
        ],
        false,
    );
    check(
        "two-tables",
        "| a | b |\n| --- | --- |\n| c | d |\n\n| e | f |\n| --- | --- |\n| g | h |",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
            r#"line[3] Empty display="" code=[]"#,
            r#"line[4] TableRow header=true align=[none,none] cells=["e", "f"] display="| e | f |" code=[]"#,
            r#"line[5] TableDelimiter display="" code=[]"#,
            r#"line[6] TableRow header=false align=[none,none] cells=["g", "h"] display="| g | h |" code=[]"#,
        ],
        false,
    );
    check(
        "code-escaped-pipe-cell",
        "| a | b |\n| --- | --- |\n| c `\\|` d | e |",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["c `\\|` d", "e"] display="| c `\\|` d | e |" code=["\\|"]"#,
        ],
        false,
    );
    check(
        "ws-line-ends",
        "| a | b |\n| --- | --- |\n| c | d |\n   \n| e | f |",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
            r#"line[3] Empty display="   " code=[]"#,
            r#"line[4] Paragraph display="| e | f |" code=[]"#,
        ],
        false,
    );
    check(
        "tab-line",
        "| a | b |\n| --- | --- |\n| c | d |\n\t\n| e | f |",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
            r#"line[3] Empty display="\t" code=[]"#,
            r#"line[4] Paragraph display="| e | f |" code=[]"#,
        ],
        false,
    );
    check(
        "ordered-list",
        "| a | b |\n| --- | --- |\n| c | d |\n1. item",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
            r#"line[3] ListItem display="item" code=[]"#,
        ],
        false,
    );
    check(
        "ordered-two",
        "| a | b |\n| --- | --- |\n| c | d |\n2. item",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
            r#"line[3] ListItem display="item" code=[]"#,
        ],
        false,
    );
    check(
        "ordered-ten",
        "| a | b |\n| --- | --- |\n| c | d |\n1234567890. item",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
            r#"line[3] TableRow header=false align=[none,none] cells=["1234567890. item", ""] display="1234567890. item" code=[]"#,
        ],
        false,
    );
    check(
        "task-list",
        "| a | b |\n| --- | --- |\n| c | d |\n- [ ] item",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
            r#"line[3] ListItem display="item" code=[]"#,
        ],
        false,
    );
    check(
        "indent4-body",
        "| a | b |\n| --- | --- |\n| c | d |\n    more",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
            r#"line[3] TableRow header=false align=[none,none] cells=["more", ""] display="    more" code=[]"#,
        ],
        false,
    );
    check(
        "indent4-bare",
        "    a | b\n    --- | ---\n    c | d",
        &[
            r#"line[0] CodeLine display="a | b" code=["a | b"]"#,
            r#"line[1] CodeLine display="--- | ---" code=["--- | ---"]"#,
            r#"line[2] CodeLine display="c | d" code=["c | d"]"#,
        ],
        false,
    );
    check(
        "no-outer-pipes",
        "a | b\n--- | ---\nc | d",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="a | b" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="c | d" code=[]"#,
        ],
        false,
    );
    check(
        "short-header-match",
        "a | b | c\n--- | ---",
        &[
            r#"line[0] Paragraph display="a | b | c" code=[]"#,
            r#"line[1] Paragraph display="--- | ---" code=[]"#,
        ],
        false,
    );
    check(
        "indent4-hr",
        "| a | b |\n| --- | --- |\n| c | d |\n    ----",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
            r#"line[3] Paragraph display="    ----" code=[]"#,
        ],
        false,
    );
    check(
        "tab-hr",
        "| a | b |\n| --- | --- |\n| c | d |\n\t----",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
            r#"line[3] Paragraph display="\t----" code=[]"#,
        ],
        false,
    );
    check(
        "indent4-heading",
        "| a | b |\n| --- | --- |\n| c | d |\n    # heading",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
            r#"line[3] Paragraph display="    # heading" code=[]"#,
        ],
        false,
    );
    check(
        "tab-heading",
        "| a | b |\n| --- | --- |\n| c | d |\n\t# heading",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
            r#"line[3] Heading level=1 display="heading" code=[]"#,
        ],
        false,
    );
    check(
        "indent4-quote",
        "| a | b |\n| --- | --- |\n| c | d |\n    > bar",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
            r#"line[3] Blockquote display="bar" code=[]"#,
        ],
        false,
    );
    check(
        "tab-quote",
        "| a | b |\n| --- | --- |\n| c | d |\n\t> bar",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
            r#"line[3] Blockquote display="bar" code=[]"#,
        ],
        false,
    );
    // Main leaves a four-column marker outside a parent item as text.
    check(
        "indent4-list",
        "| a | b |\n| --- | --- |\n| c | d |\n    - item",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
            r#"line[3] Paragraph display="    - item" code=[]"#,
        ],
        false,
    );
    check(
        "indent4-html",
        "| a | b |\n| --- | --- |\n| c | d |\n    <div>",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
            r#"line[3] Paragraph display="    <div>" code=[]"#,
        ],
        false,
    );
    check(
        "tab-html",
        "| a | b |\n| --- | --- |\n| c | d |\n\t<div>",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
            r#"line[3] Paragraph display="\t<div>" code=[]"#,
        ],
        false,
    );
    // `    ~~~` right after a row is not indented code (it cannot follow
    // the table without a blank line), so it stays text; the column-0
    // `~~~` below opens a fence that runs to the end.
    check(
        "indent4-tilde",
        "| a | b |\n| --- | --- |\n| c | d |\n    ~~~\ncode\n~~~",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
            r#"line[3] Paragraph display="    ~~~" code=[]"#,
            r#"line[4] Paragraph display="code" code=[]"#,
            r#"line[5] CodeFence display="" code=[]"#,
        ],
        false,
    );
    // Main opens its trimmed backtick fence after the table.
    check(
        "tail-fence-indent4",
        "| a | b |\n| --- | --- |\n| c | d |\n    ```\ncode",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
            r#"line[3] CodeFence display="" code=[]"#,
            r#"line[4] CodeLine display="code" code=["code"]"#,
        ],
        false,
    );
    check(
        "indent4-footnote",
        "| a | b |\n| --- | --- |\n| c | d |\n    [^1]: note",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
            r#"line[3] Paragraph display="    [^1]: note" code=[]"#,
        ],
        false,
    );
    check(
        "tab-delim",
        "| a | b |\n\t| --- | --- |\n| c | d |",
        &[
            r#"line[0] Paragraph display="| a | b |" code=[]"#,
            r#"line[1] Paragraph display="\t| --- | --- |" code=[]"#,
            r#"line[2] Paragraph display="| c | d |" code=[]"#,
        ],
        false,
    );
    check(
        "space-tab-delim",
        "| a | b |\n \t| --- | --- |\n| c | d |",
        &[
            r#"line[0] Paragraph display="| a | b |" code=[]"#,
            r#"line[1] Paragraph display=" \t| --- | --- |" code=[]"#,
            r#"line[2] Paragraph display="| c | d |" code=[]"#,
        ],
        false,
    );
    check(
        "tail-footnote",
        "| a | b |\n| --- | --- |\n| c | d |\n[^1]: note",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
            r#"line[3] Paragraph display="[^1]: note" code=[]"#,
        ],
        false,
    );
    check(
        "tail-footnote-name",
        "| a | b |\n| --- | --- |\n| c | d |\n[^note]: text",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
            r#"line[3] Paragraph display="[^note]: text" code=[]"#,
        ],
        false,
    );
    check(
        "backtick-info",
        "| a | b |\n| --- | --- |\n| c | d |\n``` not`fence",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
            r#"line[3] TableRow header=false align=[none,none] cells=["``` not`fence", ""] display="``` not`fence" code=[]"#,
        ],
        false,
    );
    // A tilde fence holds the table text as code, as pulldown does.
    check(
        "inside-tilde",
        "~~~\n| a | b |\n| --- | --- |\n| c | d |\n~~~",
        &[
            r#"line[0] CodeFence display="" code=[]"#,
            r#"line[1] CodeLine display="| a | b |" code=["| a | b |"]"#,
            r#"line[2] CodeLine display="| --- | --- |" code=["| --- | --- |"]"#,
            r#"line[3] CodeLine display="| c | d |" code=["| c | d |"]"#,
            r#"line[4] CodeFence display="" code=[]"#,
        ],
        false,
    );
    check(
        "tail-vtab",
        "| a | b |\n| --- | --- |\n| c | d |\n\u{000b}",
        &[
            r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
            r#"line[1] TableDelimiter display="" code=[]"#,
            r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
            r#"line[3] Empty display="\u{b}" code=[]"#,
        ],
        false,
    );
    drop(check);
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
    let table_rows = |markdown: &str| {
        parse_markdown(markdown)
            .into_iter()
            .filter_map(|block| match block {
                Block::Table { rows, .. } => Some(rows.len() + 1),
                Block::Heading { .. }
                | Block::Paragraph { .. }
                | Block::Blockquote { .. }
                | Block::CodeBlock { .. }
                | Block::ListItem { .. }
                | Block::HorizontalRule { .. }
                | Block::EmptyLine { .. } => None,
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        table_rows("| a | b |\n| --- | --- |\n| c | d |\n    ```\ncode"),
        vec![2],
        "tail-fence-indent4"
    );
    assert_eq!(
        table_rows("~~~\n| a | b |\n| --- | --- |\n| c | d |\n~~~"),
        vec![2],
        "inside-tilde"
    );
    assert_eq!(
        table_rows("| a | b |\n| --- | --- |\n| c | d |\n\u{000b}"),
        vec![2],
        "tail-vtab"
    );
}

fn wrap_snippet_contexts(inner: &str) -> Vec<(&'static str, String)> {
    vec![
        ("line1", inner.to_string()),
        ("line2", format!("x\n{inner}")),
        ("list", format!("- {inner}")),
        ("heading", format!("# {inner}")),
        ("quote", format!("> {inner}")),
        (
            "table",
            format!("| {inner} | z |\n| --- | --- |"),
        ),
    ]
}

fn word_style(
    md: &str,
    word: &str,
) -> (bool, bool, bool, bool, bool) {
    let start = md.find(word).unwrap_or_else(|| panic!("{word:?} not in {md:?}"));
    let end = start + word.len();
    let doc = parse_document(md);
    let segs: Vec<_> = doc
        .lines
        .iter()
        .flat_map(|line| line.styled_segments.iter())
        .filter(|s| s.source.start < end && s.source.end > start)
        .collect();
    assert!(
        !segs.is_empty(),
        "word {word:?} missing in {md:?} segs={:?}",
        doc.lines
            .iter()
            .flat_map(|line| line.styled_segments.iter())
            .map(|s| md.get(s.source.clone()).unwrap_or("?"))
            .collect::<Vec<_>>()
    );
    let s = segs[0];
    (s.bold, s.italic, s.underline, s.strike, s.code)
}

fn display_of(md: &str, line: usize) -> String {
    parse_document(md).lines[line].display_text.clone()
}

/// Already-parsed pairs must not be re-synthesized when `base > 0`.
/// Line 2, list items, headings, quotes, and table cells all use a
/// non-zero base; without adding it, outer underline is overwritten.
#[test]
fn synthesize_does_not_clobber_parsed_pairs_when_base_is_nonzero() {
    for (label, md) in wrap_snippet_contexts("<u>**hello**</u>") {
        assert_eq!(
            word_style(&md, "hello"),
            (true, false, true, false, false),
            "{label}: underline+bold lost in {md:?}"
        );
    }
    for (label, md) in wrap_snippet_contexts("<u>*hi*</u>") {
        assert_eq!(
            word_style(&md, "hi"),
            (false, true, true, false, false),
            "{label}: underline+italic lost in {md:?}"
        );
    }
    // The exact quote and table repros from the bug report.
    assert_eq!(
        word_style("> <u>*hi*</u> x", "hi"),
        (false, true, true, false, false),
        "quote repro dropped underline"
    );
    assert_eq!(
        word_style("| <u>**a**</u> | b |\n| --- | --- |", "a"),
        (true, false, true, false, false),
        "table-cell repro dropped underline"
    );
}

/// A closer preceded by a space is not right-flanking; the pair must
/// stay literal instead of italicizing the preceding word.
#[test]
fn synthesize_skips_closer_preceded_by_whitespace() {
    for (label, md) in wrap_snippet_contexts("x*foo *bar") {
        let (bold, italic, underline, strike, code) = word_style(&md, "foo");
        assert!(
            !bold && !italic && !underline && !strike && !code,
            "{label}: foo was styled in {md:?}"
        );
        let doc = parse_document(&md);
        let line = doc
            .lines
            .iter()
            .find(|l| l.display_text.contains("foo"))
            .unwrap_or(&doc.lines[0]);
        assert!(
            line.display_text.contains('*'),
            "{label}: asterisks were hidden in {:?} of {md:?}",
            line.display_text
        );
    }
    assert_eq!(display_of("x*foo *bar", 0), "x*foo *bar");
    assert_eq!(display_of("x\nx*foo *bar", 1), "x*foo *bar");
}

/// Synthesizing an outer pair must OR the new style onto nested
/// segments instead of replacing them with a single un-nested span.
#[test]
fn synthesize_ors_style_onto_nested_segments() {
    for (label, md) in wrap_snippet_contexts("x*a **b** c*y") {
        assert_eq!(
            word_style(&md, "a"),
            (false, true, false, false, false),
            "{label}: a lost italic in {md:?}"
        );
        assert_eq!(
            word_style(&md, "b"),
            (true, true, false, false, false),
            "{label}: b is not bold+italic in {md:?}"
        );
        assert_eq!(
            word_style(&md, "c"),
            (false, true, false, false, false),
            "{label}: c lost italic in {md:?}"
        );
        let doc = parse_document(&md);
        if label != "table" {
            let line = doc
                .lines
                .iter()
                .find(|l| {
                    l.styled_segments
                        .iter()
                        .any(|s| md.get(s.source.clone()) == Some("b"))
                })
                .unwrap_or(&doc.lines[0]);
            assert!(
                !line.display_text.contains('*'),
                "{label}: raw markers in {:?} of {md:?}",
                line.display_text
            );
        }
    }
}

// --- Tabs shown as spaces in raw rows ---

#[test]
fn expand_tabs_pads_to_the_next_stop_and_maps_offsets() {
    let (display, map) = RowOffsetMap::expand_tabs("a\tbc\td", 100, 4).unwrap();
    assert_eq!(display, "a   bc  d");
    // Source offsets: a=100, tab=101, b=102, c=103, tab=104, d=105, end=106.
    let shown: Vec<usize> = (100..=106).map(|src| map.source_to_display(src)).collect();
    assert_eq!(shown, vec![0, 1, 4, 5, 6, 8, 9]);
    // Rows without a tab are left to the identity map.
    assert!(RowOffsetMap::expand_tabs("abc", 0, 4).is_none());
    assert!(RowOffsetMap::expand_tabs("a\tb", 0, 0).is_none());
}

#[test]
fn clicking_in_or_after_a_tab_lands_on_the_nearer_edge() {
    let (display, map) = RowOffsetMap::expand_tabs("\tx", 10, 4).unwrap();
    assert_eq!(display, "    x");
    // Display columns 0..=1 are the tab's first half, 2..=3 the second.
    assert_eq!(map.display_to_source(0), 10);
    assert_eq!(map.display_to_source(1), 10);
    assert_eq!(map.display_to_source(2), 11);
    assert_eq!(map.display_to_source(3), 11);
    // Clicking right after the tab, on or past "x", puts the caret by "x".
    assert_eq!(map.display_to_source(4), 11);
    assert_eq!(map.display_to_source(5), 12);
}

#[test]
fn selection_across_tabs_maps_both_ends() {
    let (_, map) = RowOffsetMap::expand_tabs("a\t\tb", 0, 4).unwrap();
    // Selecting "\t\t" (source 1..3) covers display columns 1..8.
    assert_eq!(map.source_to_display(1), 1);
    assert_eq!(map.source_to_display(3), 8);
    assert_eq!(map.display_to_source(8), 3);
}

#[test]
fn expanded_tabs_keep_multibyte_text_aligned() {
    let (display, map) = RowOffsetMap::expand_tabs("é\tü", 0, 4).unwrap();
    assert_eq!(display, "é   ü");
    // é is 2 bytes: the tab is source 2..3 and display 2..5.
    assert_eq!(map.source_to_display(2), 2);
    assert_eq!(map.source_to_display(3), 5);
    assert_eq!(map.display_to_source(5), 3);
}
