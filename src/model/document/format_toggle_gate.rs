use std::collections::BTreeSet;
use std::ops::Range;

use super::{DocumentTab, ViewMode};
use crate::markdown::parse_document;

const CASES: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/.cursor/skills/verify-litemark/fixtures/format-toggle/cases.tsv"
));

#[derive(Clone, Copy, PartialEq, Eq)]
enum RowKind {
    Ok,
    KnownFailMain,
}

struct Observed {
    display_len: usize,
    flags_len: usize,
    detail: String,
}

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
        .replace('\t', "\\t")
}

fn caret_text(doc: &DocumentTab) -> String {
    format!("{}..{}", doc.caret.range.start, doc.caret.range.end)
}

fn observe(content: &str) -> Observed {
    let parsed = parse_document(content);
    let mut display_len = 0usize;
    let mut flags = String::new();
    for (li, line) in parsed.lines.iter().enumerate() {
        display_len += line.display_text.len();
        for seg in &line.styled_segments {
            let text = content.get(seg.source.start..seg.source.end).unwrap_or("");
            let link = seg.link_url.as_deref().unwrap_or("-");
            flags.push_str(&format!(
                "{li}:{text}:b{}i{}u{}s{}c{}:{link};",
                u8::from(seg.bold),
                u8::from(seg.italic),
                u8::from(seg.underline),
                u8::from(seg.strike),
                u8::from(seg.code),
            ));
        }
    }
    let flags_len = flags.len();
    Observed {
        display_len,
        flags_len,
        detail: flags,
    }
}

fn parse_sel(raw: &str) -> Option<Range<usize>> {
    let (start, end) = raw.split_once("..")?;
    let start = start.parse().ok()?;
    let end = end.parse().ok()?;
    if start > end {
        return None;
    }
    Some(start..end)
}

fn apply_action(doc: &mut DocumentTab, action: &str) -> Result<(), String> {
    let mode = ViewMode::Wysiwyg;
    if let Some(text) = action.strip_prefix("type:") {
        let text = decode_field(text);
        if text.is_empty() {
            return Err("empty type".into());
        }
        let range = doc.caret.range.clone();
        doc.insert_text_in_mode(range, &text, mode);
        return Ok(());
    }
    match action {
        "bold" | "italic" | "underline" | "strike" | "code" | "link" => {
            doc.apply_format_in_mode(action, mode);
        }
        "space" => {
            let range = doc.caret.range.clone();
            doc.insert_text_in_mode(range, " ", mode);
        }
        "enter" => doc.smart_newline(false),
        "shift-enter" => doc.smart_newline(true),
        "left" => doc.move_left_in_mode(false, mode),
        "right" => doc.move_right_in_mode(false, mode),
        "undo" => {
            let _ = doc.undo();
        }
        other => return Err(format!("unknown action {other}")),
    }
    Ok(())
}

fn classify(kind: RowKind, hit_expect: bool, hit_main: bool) -> &'static str {
    match (kind, hit_expect, hit_main) {
        (RowKind::Ok, true, _) => "pass",
        (RowKind::Ok, false, _) => "fail",
        (RowKind::KnownFailMain, true, _) => "xpass",
        (RowKind::KnownFailMain, false, true) => "xfail",
        (RowKind::KnownFailMain, false, false) => "fail",
    }
}

fn run_cases() -> (usize, usize, usize, usize, usize) {
    // The test harness prints `test <name> ... ` with no newline. A leading
    // line keeps every `case=` line on its own line for the gate script.
    println!("format_toggle_cases_begin");
    let mut pass = 0usize;
    let mut fail = 0usize;
    let mut xfail = 0usize;
    let mut xpass = 0usize;
    let mut parsed = 0usize;
    let mut seen = BTreeSet::new();

    for line in CASES.lines() {
        if line.is_empty() || line.starts_with('#') || line.starts_with("id\t") {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() != 11 {
            fail += 1;
            println!("case=bad-row fail");
            println!("detail=bad-row fields={}", fields.len());
            continue;
        }
        let id = fields[0];
        if !seen.insert(id.to_string()) {
            fail += 1;
            println!("case={id} fail");
            println!("detail={id} duplicate");
            continue;
        }
        let source = decode_field(fields[2]);
        let Some(sel) = parse_sel(fields[3]) else {
            fail += 1;
            println!("case={id} fail");
            println!("detail={id} bad sel {}", fields[3]);
            continue;
        };
        let expect = decode_field(fields[5]);
        let expect_caret = fields[6];
        let kind = match fields[7] {
            "ok" => RowKind::Ok,
            "known_fail_main" => RowKind::KnownFailMain,
            other => {
                fail += 1;
                println!("case={id} fail");
                println!("detail={id} bad status {other}");
                continue;
            }
        };
        let main_bytes = decode_field(fields[8]);
        let main_caret = fields[9];
        if source.len() < sel.end
            || !source.is_char_boundary(sel.start)
            || !source.is_char_boundary(sel.end)
        {
            fail += 1;
            println!("case={id} fail");
            println!("detail={id} sel outside source");
            continue;
        }

        let mut doc = DocumentTab::new("t".into(), "t.md".into(), source, None, None);
        doc.caret.range = sel;
        doc.caret.reversed = false;
        let mut action_error = None;
        for action in fields[4].split(',') {
            if let Err(err) = apply_action(&mut doc, action) {
                action_error = Some(err);
                break;
            }
        }
        if let Some(err) = action_error {
            fail += 1;
            println!("case={id} fail");
            println!("detail={id} {err}");
            continue;
        }

        let actual_caret = caret_text(&doc);
        let observed = observe(&doc.content);
        parsed += observed.display_len + observed.flags_len;
        let hit_expect = doc.content == expect && actual_caret == expect_caret;
        let hit_main = kind == RowKind::KnownFailMain
            && doc.content == main_bytes
            && actual_caret == main_caret;
        let word = classify(kind, hit_expect, hit_main);
        match word {
            "pass" => pass += 1,
            "fail" => fail += 1,
            "xfail" => xfail += 1,
            "xpass" => xpass += 1,
            _ => fail += 1,
        }
        println!("case={id} {word}");
        if word != "pass" {
            println!(
                "detail={id} actual={} caret={actual_caret} flags={}",
                encode_field(&doc.content),
                observed.detail
            );
        }
    }

    println!("parsed_display_bytes={parsed}");
    (pass, fail, xfail, xpass, parsed)
}

#[test]
#[ignore = "format-toggle gate; scripts/format-toggle-gate.sh"]
fn format_toggle_cases() {
    let (pass, fail, xfail, xpass, _) = run_cases();
    assert!(
        fail == 0 && xpass == 0,
        "format_toggle_gate fail={fail} xpass={xpass} pass={pass} xfail={xfail}"
    );
}
