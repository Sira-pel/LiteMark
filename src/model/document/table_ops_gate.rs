//! Ignored fixture gate for table ops. `scripts/table-ops-gate.sh` runs it.
//!
//! Expected bytes are the fixture literals. Cell text is read back from
//! `parse_document` and from pulldown-cmark on those literals. The caret for
//! a `cell:` spec is that parser's cell start, not the serialiser's helper.

use std::ops::Range;

use super::{DocumentTab, ViewMode};
use crate::markdown::{parse_document, LineKind, TableColumnAlign};

const CASES: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/.cursor/skills/verify-litemark/fixtures/table-ops/cases.tsv"
));

const EXPECTED_IDS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/.cursor/skills/verify-litemark/fixtures/table-ops/expected-ids.txt"
));

fn decode_field(raw: &str) -> String {
    let mut out = String::new();
    let mut chars = raw.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

fn encode_field(raw: &str) -> String {
    raw.replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

fn align_label(align: TableColumnAlign) -> &'static str {
    match align {
        TableColumnAlign::None => "none",
        TableColumnAlign::Left => "left",
        TableColumnAlign::Center => "center",
        TableColumnAlign::Right => "right",
    }
}

fn editor_sig(markdown: &str) -> String {
    let doc = parse_document(markdown);
    let mut tables = Vec::new();
    let mut rows = Vec::new();
    let mut align = String::new();
    let mut open = false;
    for line in &doc.lines {
        match &line.kind {
            LineKind::TableDelimiter => open = true,
            LineKind::TableRow {
                cells, alignments, ..
            } => {
                open = true;
                align = alignments
                    .iter()
                    .copied()
                    .map(align_label)
                    .collect::<Vec<_>>()
                    .join(",");
                let mut row = String::new();
                for cell in cells {
                    row.push('[');
                    row.push_str(markdown.get(cell.clone()).unwrap_or(""));
                    row.push(']');
                }
                rows.push(row);
            }
            _ => {
                if open {
                    tables.push(format!("align={align} {}", rows.join("//")));
                    rows.clear();
                    open = false;
                }
            }
        }
    }
    if open {
        tables.push(format!("align={align} {}", rows.join("//")));
    }
    if tables.is_empty() {
        "none".to_string()
    } else {
        tables.join(" || ")
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

fn pulldown_sig(markdown: &str) -> String {
    let mut options = pulldown_cmark::Options::empty();
    options.insert(pulldown_cmark::Options::ENABLE_TABLES);
    options.insert(pulldown_cmark::Options::ENABLE_FOOTNOTES);
    options.insert(pulldown_cmark::Options::ENABLE_STRIKETHROUGH);
    options.insert(pulldown_cmark::Options::ENABLE_TASKLISTS);
    let parser = pulldown_cmark::Parser::new_ext(markdown, options);
    let mut tables = Vec::new();
    let mut align = String::new();
    let mut rows: Vec<String> = Vec::new();
    let mut cells: Vec<String> = Vec::new();
    let mut cell = String::new();
    let mut in_cell = false;
    for event in parser {
        match event {
            pulldown_cmark::Event::Start(pulldown_cmark::Tag::Table(aligns)) => {
                align = aligns
                    .iter()
                    .map(pulldown_align)
                    .collect::<Vec<_>>()
                    .join(",");
                rows.clear();
                cells.clear();
                cell.clear();
                in_cell = false;
            }
            pulldown_cmark::Event::Start(pulldown_cmark::Tag::TableCell) => {
                cell.clear();
                in_cell = true;
            }
            pulldown_cmark::Event::Text(text) | pulldown_cmark::Event::Code(text) if in_cell => {
                cell.push_str(&text);
            }
            pulldown_cmark::Event::End(pulldown_cmark::TagEnd::TableCell) => {
                cells.push(format!("[{cell}]"));
                in_cell = false;
            }
            // pulldown's header is `TableHead` of cells, not a `TableRow`.
            pulldown_cmark::Event::End(pulldown_cmark::TagEnd::TableHead)
            | pulldown_cmark::Event::End(pulldown_cmark::TagEnd::TableRow) => {
                if !cells.is_empty() {
                    rows.push(cells.join(""));
                    cells.clear();
                }
            }
            pulldown_cmark::Event::End(pulldown_cmark::TagEnd::Table) => {
                tables.push(format!("align={align} {}", rows.join("//")));
                rows.clear();
            }
            _ => {}
        }
    }
    if tables.is_empty() {
        "none".to_string()
    } else {
        tables.join(" || ")
    }
}

fn cell_start(markdown: &str, row: usize, col: usize) -> Result<usize, String> {
    let doc = parse_document(markdown);
    let mut seen = 0usize;
    for line in &doc.lines {
        if let LineKind::TableRow { cells, .. } = &line.kind {
            if seen == row {
                let range = cells
                    .get(col)
                    .ok_or_else(|| format!("no cell {row}:{col} cells={}", cells.len()))?;
                return Ok(range.start);
            }
            seen += 1;
        }
    }
    Err(format!("no table row {row} (saw {seen})"))
}

struct Placed {
    range: Range<usize>,
    reversed: bool,
}

fn place_anchor(source: &str, anchor: &str) -> Result<Placed, String> {
    if let Some(rest) = anchor.strip_prefix("cell:") {
        let (row, col) = rest.split_once(':').ok_or("bad cell anchor")?;
        let row: usize = row.parse().map_err(|_| format!("bad row {row}"))?;
        let col: usize = col.parse().map_err(|_| format!("bad col {col}"))?;
        let at = cell_start(source, row, col)?;
        return Ok(Placed {
            range: at..at,
            reversed: false,
        });
    }
    if let Some(rest) = anchor.strip_prefix("byte:") {
        let at: usize = rest.parse().map_err(|_| format!("bad byte {rest}"))?;
        return Ok(Placed {
            range: at..at,
            reversed: false,
        });
    }
    if let Some(rest) = anchor.strip_prefix("sel:") {
        let reversed = rest.ends_with(":rev");
        let body = rest.strip_suffix(":rev").unwrap_or(rest);
        let (start, end) = body.split_once("..").ok_or("bad sel")?;
        let start: usize = start.parse().map_err(|_| format!("bad sel {body}"))?;
        let end: usize = end.parse().map_err(|_| format!("bad sel {body}"))?;
        if start > end {
            return Err(format!("sel start {start} > end {end}"));
        }
        return Ok(Placed {
            range: start..end,
            reversed,
        });
    }
    Err(format!("bad anchor {anchor}"))
}

fn expect_caret(content: &str, spec: &str, placed: &Placed) -> Result<Placed, String> {
    if spec == "keep" {
        return Ok(Placed {
            range: placed.range.clone(),
            reversed: placed.reversed,
        });
    }
    if let Some(rest) = spec.strip_prefix("cell:") {
        let (row, col) = rest.split_once(':').ok_or("bad cell caret")?;
        let row: usize = row.parse().map_err(|_| format!("bad row {row}"))?;
        let col: usize = col.parse().map_err(|_| format!("bad col {col}"))?;
        let at = cell_start(content, row, col)?;
        return Ok(Placed {
            range: at..at,
            reversed: false,
        });
    }
    if let Some(rest) = spec.strip_prefix("sel:") {
        let reversed = rest.ends_with(":rev");
        let body = rest.strip_suffix(":rev").unwrap_or(rest);
        let (start, end) = body.split_once("..").ok_or("bad caret sel")?;
        let start: usize = start.parse().map_err(|_| format!("bad caret {body}"))?;
        let end: usize = end.parse().map_err(|_| format!("bad caret {body}"))?;
        return Ok(Placed {
            range: start..end,
            reversed,
        });
    }
    Err(format!("bad caret {spec}"))
}

fn run_op(doc: &mut DocumentTab, op: &str, arg: &str) -> Result<(), String> {
    match op {
        "insert-row-above" => doc.table_add_row_above(),
        "insert-row-below" => doc.table_add_row_below(),
        "insert-col-left" => doc.table_add_col_left(),
        "insert-col-right" => doc.table_add_col_right(),
        "delete-row" => doc.table_delete_row(),
        "delete-col" => doc.table_delete_col(),
        "move-row" => {
            let index: usize = arg.parse().map_err(|_| format!("bad move-row {arg}"))?;
            doc.table_move_row(index);
        }
        "move-col" => {
            let index: usize = arg.parse().map_err(|_| format!("bad move-col {arg}"))?;
            doc.table_move_col(index);
        }
        "align-left" => doc.table_align_column(TableColumnAlign::Left),
        "align-center" => doc.table_align_column(TableColumnAlign::Center),
        "align-right" => doc.table_align_column(TableColumnAlign::Right),
        "align-none" => doc.table_align_column(TableColumnAlign::None),
        other => return Err(format!("unknown op {other}")),
    }
    Ok(())
}

fn same_caret(doc: &DocumentTab, placed: &Placed) -> bool {
    doc.caret.range == placed.range && doc.caret.reversed == placed.reversed
}

fn fail(id: &str, detail: String) {
    println!("case={id} fail");
    println!("detail={id} {detail}");
}

fn run_one(fields: &[&str]) -> bool {
    let id = fields[0];
    let op = fields[1];
    let source = decode_field(fields[2]);
    let anchor = fields[3];
    let arg = fields[4];
    let expect = decode_field(fields[5]);
    let caret_spec = fields[6];
    let editor = decode_field(fields[7]);
    let pulldown = decode_field(fields[8]);
    let after = fields[9];

    let got_editor = editor_sig(&expect);
    if got_editor != editor {
        fail(id, format!("editor got={got_editor} want={editor}"));
        return false;
    }
    let got_pd = pulldown_sig(&expect);
    if got_pd != pulldown {
        fail(id, format!("pulldown got={got_pd} want={pulldown}"));
        return false;
    }

    let placed = match place_anchor(&source, anchor) {
        Ok(placed) => placed,
        Err(err) => {
            fail(id, err);
            return false;
        }
    };
    if placed.range.end > source.len()
        || !source.is_char_boundary(placed.range.start)
        || !source.is_char_boundary(placed.range.end)
    {
        fail(id, format!("anchor outside source {}", anchor));
        return false;
    }

    let mut doc = DocumentTab::new("t".into(), "t.md".into(), source.clone(), None, None);
    doc.caret.range = placed.range.clone();
    doc.caret.reversed = placed.reversed;
    if let Err(err) = run_op(&mut doc, op, arg) {
        fail(id, err);
        return false;
    }

    if doc.content != expect {
        fail(id, format!("bytes actual={}", encode_field(&doc.content)));
        return false;
    }
    let want = match expect_caret(&doc.content, caret_spec, &placed) {
        Ok(want) => want,
        Err(err) => {
            fail(id, err);
            return false;
        }
    };
    if !same_caret(&doc, &want) {
        fail(
            id,
            format!(
                "caret got={}..{} rev={} want={}..{} rev={}",
                doc.caret.range.start,
                doc.caret.range.end,
                doc.caret.reversed,
                want.range.start,
                want.range.end,
                want.reversed
            ),
        );
        return false;
    }

    if after == "view" {
        let before = doc.caret.range.clone();
        let rev = doc.caret.reversed;
        doc.set_view_mode(ViewMode::Raw);
        doc.set_view_mode(ViewMode::Wysiwyg);
        if doc.content != expect || doc.caret.range != before || doc.caret.reversed != rev {
            fail(id, "view switch changed the caret or the bytes".into());
            return false;
        }
    } else if !after.is_empty() {
        fail(id, format!("bad after {after}"));
        return false;
    }

    let changed = expect != source;
    if !changed {
        if doc.undo() {
            fail(id, "noop recorded an undo step".into());
            return false;
        }
        println!("case={id} pass");
        return true;
    }

    if !doc.undo() {
        fail(id, "undo returned false".into());
        return false;
    }
    if doc.content != source || !same_caret(&doc, &placed) {
        fail(
            id,
            format!(
                "undo bytes={} caret={}..{} rev={}",
                encode_field(&doc.content),
                doc.caret.range.start,
                doc.caret.range.end,
                doc.caret.reversed
            ),
        );
        return false;
    }
    if doc.undo() {
        fail(id, "second undo should be empty".into());
        return false;
    }
    if !doc.redo() {
        fail(id, "redo returned false".into());
        return false;
    }
    if doc.content != expect || !same_caret(&doc, &want) {
        fail(
            id,
            format!(
                "redo bytes={} caret={}..{} rev={}",
                encode_field(&doc.content),
                doc.caret.range.start,
                doc.caret.range.end,
                doc.caret.reversed
            ),
        );
        return false;
    }
    if doc.redo() {
        fail(id, "second redo should be empty".into());
        return false;
    }
    println!("case={id} pass");
    true
}

fn expected_ids() -> Vec<&'static str> {
    EXPECTED_IDS
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect()
}

#[test]
#[ignore = "table-ops gate; scripts/table-ops-gate.sh"]
fn table_ops_cases() {
    println!("table_ops_cases_begin");
    let expected = expected_ids();
    let mut ran: Vec<&str> = Vec::new();
    let mut fail_n = 0usize;
    let mut pass_n = 0usize;
    for line in CASES.lines() {
        if line.is_empty() || line.starts_with('#') || line.starts_with("id\t") {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() != 10 {
            fail_n += 1;
            println!("case=bad-row fail");
            println!("detail=bad-row fields={}", fields.len());
            continue;
        }
        let id = fields[0];
        if id.contains('/') || id.contains("..") {
            fail_n += 1;
            println!("case={id} fail");
            println!("detail={id} id rejected");
            continue;
        }
        ran.push(id);
        if run_one(&fields) {
            pass_n += 1;
        } else {
            fail_n += 1;
        }
    }
    if ran != expected {
        fail_n += 1;
        println!("case=id-pin fail");
        println!(
            "detail=id-pin ran={} expected={}",
            ran.len(),
            expected.len()
        );
    }
    assert!(fail_n == 0, "table_ops_gate fail={fail_n} pass={pass_n}");
}
