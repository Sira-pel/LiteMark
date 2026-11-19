//! `ParsedDocument::apply_edit` must give exactly `parse_document` of the
//! new text, and leave checkpoints a fresh parse would agree with.

use super::*;

struct Lcg(u64);

impl Lcg {
    fn next(&mut self, n: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 33) as usize) % n.max(1)
    }
}

const LINES: &[&str] = &[
    "",
    "",
    "",
    "Plain paragraph text",
    "more *italic* and **bold** words",
    "a [link](https://x.y) and `code`",
    "# Heading",
    "## Sub *heading* ##",
    "- bullet",
    "- [ ] task",
    "- [x] done",
    "  - nested item",
    "    - deeper",
    "1. one",
    "2) two",
    "10. ten",
    "> quote",
    "> > nested quote",
    "```",
    "```rust",
    "~~~",
    "~~~~",
    "    indented code",
    "\tTab code",
    "---",
    "***",
    "___",
    "| a | b |",
    "| --- | :-: |",
    "|---|---|",
    "| 1 | 2 |",
    "| x | **y** |",
    "a | b",
    "- | c |",
    "text with | pipe",
    "~~strike~~ <u>u</u>",
    "   ```",
    " - spaced bullet",
    "+ plus",
    "* star item",
    "line\\",
];

const SNIPPETS: &[&str] = &[
    "a",
    " ",
    "\n",
    "\n\n",
    "|",
    "| x |",
    "```",
    "~~~",
    "#",
    "# ",
    "- ",
    "1. ",
    "> ",
    "    ",
    "\t",
    "*",
    "**",
    "`",
    "[",
    "](u)",
    "---",
    "\r\n",
    "\r",
    "| --- |",
    "x\n| --- | --- |\n",
];

fn random_doc(rng: &mut Lcg) -> String {
    let n = 40 + rng.next(320);
    let endings = ["\n", "\n", "\n", "\r\n", "\r"];
    let mixed = rng.next(4) == 0;
    let ending = endings[rng.next(3)];
    let mut s = String::new();
    for _ in 0..n {
        s.push_str(LINES[rng.next(LINES.len())]);
        s.push_str(if mixed {
            endings[rng.next(endings.len())]
        } else {
            ending
        });
    }
    if rng.next(3) == 0 {
        s.push_str(LINES[rng.next(LINES.len())]);
    }
    s
}

fn floor(s: &str, mut i: usize) -> usize {
    i = i.min(s.len());
    while !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn random_edit(rng: &mut Lcg, text: &str) -> (std::ops::Range<usize>, String) {
    let start = floor(text, rng.next(text.len() + 1));
    let len = match rng.next(4) {
        0 => 0,
        1 => rng.next(3),
        2 => rng.next(20),
        _ => rng.next(200),
    };
    let end = floor(text, start + len).max(start);
    let mut insert = String::new();
    for _ in 0..rng.next(4) {
        insert.push_str(SNIPPETS[rng.next(SNIPPETS.len())]);
    }
    (start..end, insert)
}

/// State before every line of a full parse of `text`.
fn all_states(text: &str) -> Vec<ParseState> {
    let mut raw = LineSource::all(text);
    let mut lines = Vec::new();
    let mut state = ParseState::default();
    let mut states = Vec::new();
    parse_lines(&mut raw, 0, &mut state, &mut lines, |_, _, now| {
        states.push(now.clone());
        Step::Parse
    });
    states
}

#[test]
fn incremental_parse_matches_a_full_parse() {
    let mut rng = Lcg(0x5eed_1234_abcd_0001);
    let mut resumed_mid_document = 0;
    let cases: usize = std::env::var("INCR_CASES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(250);
    for case in 0..cases {
        let mut text = random_doc(&mut rng);
        let mut doc = parse_document(&text);
        for step in 0..12 {
            let (range, insert) = random_edit(&mut rng, &text);
            let mut next = text.clone();
            next.replace_range(range.clone(), &insert);
            doc.apply_edit(&next, range.clone(), insert.len());
            let full = parse_document(&next);
            if doc != full {
                let ix = doc.lines.iter().zip(&full.lines).position(|(a, b)| a != b);
                let detail = match ix {
                    Some(ix) => format!(
                        "first different line {ix} (of {} vs {})\nincremental: {:?}\nfull:        {:?}\nsplice: {:?}",
                        doc.lines.len(),
                        full.lines.len(),
                        doc.lines[ix],
                        full.lines[ix],
                        doc.meta.last_splice
                    ),
                    None => format!(
                        "line counts {} vs {}, code blocks equal: {}",
                        doc.lines.len(),
                        full.lines.len(),
                        doc.code_blocks == full.code_blocks
                    ),
                };
                panic!("case {case} step {step}: edit {range:?} -> {insert:?}\n{detail}");
            }
            if let Some((_, start, removed, inserted)) = doc.meta.last_splice {
                if start > 0 && removed + inserted < doc.lines.len() {
                    resumed_mid_document += 1;
                }
            }
            let states = all_states(&next);
            for (line, state) in &doc.meta.checkpoints {
                assert_eq!(
                    state, &states[*line],
                    "case {case} step {step}: checkpoint at line {line}"
                );
            }
            text = next;
        }
    }
    // Most edits should touch a small part of the document.
    assert!(
        resumed_mid_document > 1000,
        "only {resumed_mid_document} local updates"
    );
}

#[test]
fn typing_into_a_large_document_reparses_a_few_lines() {
    let mut text = String::new();
    for i in 0..5000 {
        text.push_str(&format!(
            "Paragraph {i} with **bold** text.\n\n- item {i}\n\n"
        ));
    }
    let mut doc = parse_document(&text);
    let at = text.len() / 2;
    let at = text[..at].rfind("Paragraph").unwrap() + 4;
    text.insert(at, 'x');
    doc.apply_edit(&text, at..at, 1);
    assert_eq!(doc, parse_document(&text));
    let (_, _, removed, inserted) = doc.meta.last_splice.unwrap();
    assert!(removed <= 3 * CHECKPOINT_EVERY, "re-parsed {removed} lines");
    assert_eq!(removed, inserted);
}

#[test]
fn slice_bounds_match_slice_display() {
    let mut rng = Lcg(0x51ce_b0d5);
    for _ in 0..40 {
        let doc = parse_document(&random_doc(&mut rng));
        for line in &doc.lines {
            let len = line.display_text.len();
            for _ in 0..4 {
                let a = rng.next(len + 1);
                let b = a + rng.next(len + 1 - a);
                let sliced = line.offset_map.slice_display(a..b);
                assert_eq!(
                    line.offset_map.slice_bounds(a..b),
                    (sliced.line_start, sliced.line_end),
                    "{:?} {a}..{b}",
                    line.display_text
                );
            }
        }
    }
}
