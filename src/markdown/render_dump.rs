//! Headless dump of the production markdown render tree.
//!
//! `parse_document` is what the editor lays out. `visual_rows` is what
//! `EditorElement` uses to wrap each line's display text. Both are called
//! directly. This module exists only for `cargo test`.

use std::env;
use std::fs;
use std::ops::Range;
use std::time::Instant;

use crate::model::text_ops::visual_rows;

use super::{parse_document, LineKind, StyledSegment, TableColumnAlign};

pub fn render_tree(markdown: &str, wrap_cols: usize) -> String {
    let doc = parse_document(markdown);
    let mut out = String::new();
    out.push_str(&format!("bytes={}\n", markdown.len()));
    out.push_str(&format!("lines={}\n", doc.lines.len()));
    out.push_str(&format!("wrap_cols={wrap_cols}\n"));

    for (line_index, line) in doc.lines.iter().enumerate() {
        out.push_str(&format!(
            "line[{line_index}] {} source={} display={}\n",
            kind_token(&line.kind),
            range_token(&line.source),
            quote_token(&line.display_text),
        ));

        if let LineKind::TableRow {
            cells, alignments, ..
        } = &line.kind
        {
            for (cell_index, cell) in cells.iter().enumerate() {
                let align = alignments
                    .get(cell_index)
                    .copied()
                    .unwrap_or(TableColumnAlign::None);
                let text = slice_text(markdown, cell);
                out.push_str(&format!(
                    "cell[{line_index}][{cell_index}] source={} align={} text={}\n",
                    range_token(cell),
                    align_token(align),
                    quote_token(&text),
                ));
            }
        }

        for (segment_index, segment) in line.styled_segments.iter().enumerate() {
            out.push_str(&format!(
                "segment[{line_index}][{segment_index}] {}\n",
                segment_token(markdown, segment),
            ));
        }

        let rows = visual_rows(&line.display_text, wrap_cols);
        for (row_index, (start, end)) in rows.iter().enumerate() {
            let text = slice_text(&line.display_text, &(*start..*end));
            out.push_str(&format!(
                "row[{line_index}][{row_index}] display={start}..{end} text={}\n",
                quote_token(&text),
            ));
        }
    }

    out
}

fn kind_token(kind: &LineKind) -> String {
    match kind {
        LineKind::Empty => "kind=Empty".to_string(),
        LineKind::Paragraph => "kind=Paragraph".to_string(),
        LineKind::Heading { level } => format!("kind=Heading level={level}"),
        LineKind::Blockquote => "kind=Blockquote".to_string(),
        LineKind::ListItem {
            ordered,
            number,
            task,
            indent_cols,
            checkbox_source,
        } => {
            let checkbox = match checkbox_source {
                Some(range) => range_token(range),
                None => "-".to_string(),
            };
            format!(
                "kind=ListItem ordered={ordered} number={number} task={} indent_cols={indent_cols} checkbox={checkbox}",
                task_token(*task),
            )
        }
        LineKind::CodeFence { language, opening } => {
            let language_token = match language {
                Some(language) => quote_token(language),
                None => "-".to_string(),
            };
            format!("kind=CodeFence opening={opening} language={language_token}")
        }
        LineKind::CodeLine => "kind=CodeLine".to_string(),
        LineKind::HorizontalRule => "kind=HorizontalRule".to_string(),
        LineKind::TableDelimiter => "kind=TableDelimiter".to_string(),
        LineKind::TableRow {
            is_header, cells, ..
        } => format!("kind=TableRow header={is_header} cells={}", cells.len()),
    }
}

fn segment_token(markdown: &str, segment: &StyledSegment) -> String {
    let link = match &segment.link_url {
        Some(url) => quote_token(url),
        None => "-".to_string(),
    };
    let text = slice_text(markdown, &segment.source);
    format!(
        "source={} bold={} italic={} underline={} strike={} code={} link={} text={}",
        range_token(&segment.source),
        segment.bold,
        segment.italic,
        segment.underline,
        segment.strike,
        segment.code,
        link,
        quote_token(&text),
    )
}

fn task_token(task: Option<bool>) -> &'static str {
    match task {
        None => "none",
        Some(true) => "checked",
        Some(false) => "unchecked",
    }
}

fn align_token(align: TableColumnAlign) -> &'static str {
    match align {
        TableColumnAlign::None => "none",
        TableColumnAlign::Left => "left",
        TableColumnAlign::Center => "center",
        TableColumnAlign::Right => "right",
    }
}

fn range_token(range: &Range<usize>) -> String {
    format!("{}..{}", range.start, range.end)
}

fn quote_token(text: &str) -> String {
    format!("{text:?}")
}

fn slice_text(text: &str, range: &Range<usize>) -> String {
    if range.start > range.end
        || range.end > text.len()
        || !text.is_char_boundary(range.start)
        || !text.is_char_boundary(range.end)
    {
        return format!("<invalid {}..{}>", range.start, range.end);
    }
    text[range.start..range.end].to_string()
}

#[test]
fn render_tree_heading_line_is_exact() {
    let dump = render_tree("# Title\n", 40);
    let expected = "line[0] kind=Heading level=1 source=0..7 display=\"Title\"";
    assert!(
        dump.lines().any(|line| line == expected),
        "missing exact line {expected}\ndump:\n{dump}"
    );
}

/// Harness entry. Ignored so `cargo test` does not require fixture env vars.
///
/// ```text
/// LITEMARK_RENDER_IN=note.md LITEMARK_RENDER_OUT=/tmp/out.txt LITEMARK_WRAP_COLS=40 \
///   cargo test --bin litemark markdown::render_dump::dump_render_tree -- --ignored --exact --nocapture
/// ```
#[test]
#[ignore = "verify-litemark harness; set LITEMARK_RENDER_IN and LITEMARK_RENDER_OUT"]
fn dump_render_tree() {
    let input_path = env::var("LITEMARK_RENDER_IN").expect("LITEMARK_RENDER_IN");
    let output_path = env::var("LITEMARK_RENDER_OUT").expect("LITEMARK_RENDER_OUT");
    let wrap_cols = env::var("LITEMARK_WRAP_COLS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(40);
    let markdown =
        fs::read_to_string(&input_path).unwrap_or_else(|err| panic!("read {input_path}: {err}"));
    let started = Instant::now();
    let dump = render_tree(&markdown, wrap_cols);
    let elapsed_us = started.elapsed().as_micros();
    fs::write(&output_path, &dump).unwrap_or_else(|err| panic!("write {output_path}: {err}"));
    eprintln!("LITEMARK_ELAPSED_US={elapsed_us}");
    eprintln!("LITEMARK_RENDER_OUT={output_path}");
}
