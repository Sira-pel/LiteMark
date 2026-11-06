use super::*;

const M: ViewMode = ViewMode::Wysiwyg;

fn tab(src: &str) -> DocumentTab {
    let mut d = DocumentTab::new("t".into(), "t.md".into(), src.to_string(), None, None);
    d.set_view_mode(M);
    d
}

fn formatted(src: &str, sel: Range<usize>, cmd: &str) -> String {
    let mut d = tab(src);
    d.caret.range = sel;
    d.apply_format_in_mode(cmd, M);
    d.content
}

/// Bold and italic on any selection of a heading must behave exactly like
/// the same selection of the same text as a paragraph.
#[test]
fn every_selection_in_a_heading_formats_like_a_paragraph() {
    let body = "Welcome to LiteMark";
    for pre in ["# ", "## ", "### "] {
        for tail in ["", "\n", "\n\nBody\n"] {
            let heading = format!("{pre}{body}{tail}");
            let para = format!("{body}{tail}");
            for cmd in ["bold", "italic"] {
                for a in 0..=heading.len() {
                    for b in a..=heading.len() {
                        if a < pre.len() && b <= pre.len() {
                            continue;
                        }
                        let got = formatted(&heading, a..b, cmd);
                        let want = format!(
                            "{pre}{}",
                            formatted(
                                &para,
                                a.saturating_sub(pre.len())..b.saturating_sub(pre.len()),
                                cmd
                            )
                        );
                        assert_eq!(got, want, "{pre:?} tail={tail:?} {cmd} sel={a}..{b}");
                    }
                }
            }
        }
    }
}

/// Selections built with the real cursor-movement methods (End, Shift+Left,
/// word jumps, double click) format a heading like a paragraph.
#[test]
fn keyboard_selections_in_a_heading_format_like_a_paragraph() {
    fn run(d: &mut DocumentTab, ops: &[&str]) {
        for op in ops {
            match *op {
                "home" => d.move_home_in_mode(false, M),
                "end" => d.move_end_in_mode(false, M),
                "right" => d.move_right_in_mode(false, M),
                "left" => d.move_left_in_mode(false, M),
                "sright" => d.move_right_in_mode(true, M),
                "sleft" => d.move_left_in_mode(true, M),
                "sword_l" => d.move_word_left(true),
                "sword_r" => d.move_word_right(true),
                "dbl" => d.select_word(),
                _ => unreachable!(),
            }
        }
    }
    let scripts: Vec<Vec<&str>> = vec![
        [vec!["end"], vec!["sleft"; 8]].concat(),
        vec!["end", "sword_l"],
        [vec!["home"], vec!["sright"; 7]].concat(),
        [vec!["home"], vec!["right"; 3], vec!["sright"; 4]].concat(),
        vec!["home", "sword_r"],
        [vec!["end"], vec!["left"; 8], vec!["sword_r"]].concat(),
        [vec!["home"], vec!["right"; 11], vec!["dbl"]].concat(),
    ];
    let body = "Welcome to LiteMark\n\nBody\n";
    for pre in ["# ", "## "] {
        for ops in &scripts {
            for cmd in ["bold", "italic"] {
                let mut h = tab(&format!("{pre}{body}"));
                let mut p = tab(body);
                h.caret.range = 0..0;
                p.caret.range = 0..0;
                run(&mut h, ops);
                run(&mut p, ops);
                h.apply_format_in_mode(cmd, M);
                p.apply_format_in_mode(cmd, M);
                assert_eq!(h.content, format!("{pre}{}", p.content), "{pre:?} {ops:?} {cmd}");
            }
        }
    }
}
