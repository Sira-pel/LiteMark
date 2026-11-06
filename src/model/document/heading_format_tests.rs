use super::*;

fn run(src: &str, sel: std::ops::Range<usize>, cmds: &[&str], mode: ViewMode) -> String {
    let mut d = DocumentTab::new("t".into(), "t.md".into(), src.to_string(), None, None);
    d.caret.range = sel;
    for c in cmds {
        d.apply_format_in_mode(c, mode);
    }
    d.content
}

#[test]
fn inline_formats_work_on_headings_in_both_modes() {
    for mode in [ViewMode::Raw, ViewMode::Wysiwyg] {
        let h = "# Hello world here";
        assert_eq!(run(h, 2..18, &["bold"], mode), "# **Hello world here**");
        assert_eq!(run(h, 8..13, &["bold"], mode), "# Hello **world** here");
        assert_eq!(run(h, 2..18, &["italic"], mode), "# *Hello world here*");
        assert_eq!(run(h, 2..18, &["strike"], mode), "# ~~Hello world here~~");
        assert_eq!(
            run(h, 2..18, &["bold", "italic", "underline"], mode),
            "# ***<u>Hello world here</u>***"
        );
    }
}

#[test]
fn heading_format_toggles_off_and_can_be_unformatted_in_parts() {
    for mode in [ViewMode::Raw, ViewMode::Wysiwyg] {
        assert_eq!(run("# Hello world here", 2..18, &["bold", "bold"], mode), "# Hello world here");
        let b = "# **Hello world here**";
        assert_eq!(run(b, 10..15, &["bold"], mode), "# **Hello** world **here**");
        assert_eq!(run(b, 4..9, &["bold"], mode), "# Hello **world here**");
        assert_eq!(run(b, 16..20, &["bold"], mode), "# **Hello world** here");
        assert_eq!(run(b, 10..15, &["italic"], mode), "# **Hello *world* here**");
        assert_eq!(
            run("# ***Hello world here***", 11..16, &["italic"], mode),
            "# ***Hello* world *here***"
        );
    }
}

#[test]
fn heading_marker_survives_formatting_a_selection_that_includes_it() {
    assert_eq!(
        run("# Hello world here", 0..18, &["bold"], ViewMode::Wysiwyg),
        "# **Hello world here**"
    );
    assert_eq!(run("## Title", 3..8, &["bold"], ViewMode::Wysiwyg), "## **Title**");
}

#[test]
fn bold_on_a_word_in_the_welcome_heading_only_wraps_that_word() {
    let src = "# Welcome to LiteMark\n\nBody\n";
    assert_eq!(
        run(src, 13..21, &["bold"], ViewMode::Wysiwyg),
        "# Welcome to **LiteMark**\n\nBody\n"
    );
    assert_eq!(
        run(src, 10..12, &["bold"], ViewMode::Wysiwyg),
        "# Welcome **to** LiteMark\n\nBody\n"
    );
    assert_eq!(
        run(src, 13..21, &["bold", "underline"], ViewMode::Wysiwyg),
        "# Welcome to <u>**LiteMark**</u>\n\nBody\n"
    );
}
