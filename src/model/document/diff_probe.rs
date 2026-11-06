//! Randomized check that incremental re-parsing matches a full parse.
//!
//! `DocumentTab` re-parses only the changed part of a document after each
//! edit. This probe takes a few seed documents and applies random WYSIWYG
//! edits to them: typing, deleting, inline format toggles, undo and newlines.
//! After every step it checks that `t.parsed()` equals
//! `parse_document(&t.content)`. It also counts panics. Any mismatch
//! (DIVERGE) or panic (PANIC) fails the test.
//!
//! It is `#[ignore]`d because it takes about 10–25 seconds. Run it with:
//!
//! ```text
//! cargo test --bin litemark incremental_parse_matches_full_parse -- --ignored
//! ```
//!
//! - `DIFF_SEED=<u64 or 0x-hex>`: change the random seed. A failing run
//!   replays exactly with the same seed. An unparseable value panics; an
//!   empty one means the default seed.
//! - `DIFF_OUT=<path>`: write a log of every step (seed/run/step, op, status,
//!   caret, content) to that file.

use super::*;

/// Small xorshift generator, so a run is reproducible from its seed alone.
struct R(u64);
impl R {
    fn n(&mut self, m: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % m as u64) as usize
    }
}
#[test]
#[ignore = "slow randomized probe (~10-25 s); run with --ignored"]
fn incremental_parse_matches_full_parse() {
    let seeds = [
        "abc **x**\n---\nnext *i*\n===\n\n[foo]: http://a\nsee [foo] and [bar]\n\n1. one\nlazy *t*\n\n<div>\nraw **b**\n</div>\n    indented code\n",
        "Hello **bold** and *it* <u>u</u> ~~s~~ `c` [l](http://x) 😀 tiếng ខ្មែរ\n",
        "# Head **b**\n- item *i*\n> quote <u>u</u>\n| a | **b** |\n|---|---|\n| *c* | d |\n\n```\ncode **x**\n```\n",
        "plain text\nline two with ***both*** here\n\nnew para `code` end\n",
    ];
    let toks = [
        "-", "=", "#", " [foo]", "| ", "> ", "1. ", "```", "a", "b", " ", "😀", "ế", "x y", "*",
        "\n",
    ];
    let cmds = ["bold", "italic", "underline", "strike", "code", "link"];
    let mut out = String::new();
    let mut bad = 0usize;
    let base_seed: u64 = match std::env::var("DIFF_SEED") {
        Ok(raw) if !raw.trim().is_empty() => {
            let s = raw.trim();
            let parsed = if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
                u64::from_str_radix(hex, 16).ok()
            } else {
                s.parse().ok()
            };
            // A bad value must not fall back to the default seed: that
            // would look like a replay of the requested seed.
            parsed.unwrap_or_else(|| {
                panic!("DIFF_SEED={raw:?} is not a u64 (decimal or 0x-hex)")
            })
        }
        _ => 0x9E3779B97F4A7C15,
    };
    for (si, seed) in seeds.iter().enumerate() {
        for run in 0..150u64 {
            let mut r = R(base_seed ^ (si as u64 * 1000 + run + 1));
            let mut t = DocumentTab::new("a".into(), "a.md".into(), seed.repeat(3), None, None);
            for step in 0..60 {
                let len = t.content.len();
                let a = floor_boundary(&t.content, r.n(len + 1));
                let op = r.n(10);
                let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    match op {
                        0..=3 => {
                            t.caret.move_to(a);
                            let p = t.caret.cursor();
                            t.insert_text_in_mode(
                                p..p,
                                toks[r.n(toks.len())],
                                ViewMode::Wysiwyg,
                            );
                        }
                        4 => {
                            t.caret.move_to(a);
                            t.apply_format_in_mode(cmds[r.n(6)], ViewMode::Wysiwyg);
                        }
                        5 => {
                            let b = floor_boundary(&t.content, (a + r.n(12)).min(len));
                            t.caret.move_to(a);
                            t.caret.select_to(b);
                            t.apply_format_in_mode(cmds[r.n(6)], ViewMode::Wysiwyg);
                        }
                        6 => {
                            let b = floor_boundary(&t.content, (a + r.n(4)).min(len));
                            t.insert_text_in_mode(a..b, "", ViewMode::Wysiwyg);
                        }
                        7 => {
                            t.undo();
                        }
                        8 => {
                            t.caret.move_to(a);
                            let p = t.caret.cursor();
                            t.apply_format_in_mode(cmds[r.n(6)], ViewMode::Wysiwyg);
                            t.insert_text_in_mode(p..p, "zz", ViewMode::Wysiwyg);
                        }
                        _ => {
                            t.caret.move_to(a);
                            let p = t.caret.cursor();
                            t.insert_text_in_mode(p..p, "\n", ViewMode::Wysiwyg);
                        }
                    }
                }));
                let st = if res.is_err() {
                    "PANIC"
                } else if *t.parsed() != parse_document(&t.content) {
                    "DIVERGE"
                } else {
                    "ok"
                };
                if st != "ok" {
                    bad += 1;
                }
                out.push_str(&format!(
                    "{si}/{run}/{step} op{op} {st} c={} wc={:?} {:?}\n",
                    t.caret.cursor(),
                    t.parsed().lines.len(),
                    t.content
                ));
                if res.is_err() {
                    break;
                }
            }
        }
    }
    if let Ok(path) = std::env::var("DIFF_OUT") {
        std::fs::write(path, &out).unwrap();
    }
    assert_eq!(bad, 0, "diff_probe DIVERGE/PANIC count={bad}");
}
