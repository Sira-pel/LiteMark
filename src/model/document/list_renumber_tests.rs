//! Ordered lists stay in sequence after edits that add, remove or move items.

use super::*;

fn doc(content: &str, mode: ViewMode) -> DocumentTab {
    let mut d = DocumentTab::new("lr".into(), "lr.md".into(), content.into(), None, None);
    d.set_view_mode(mode);
    d
}

const MODES: [ViewMode; 2] = [ViewMode::Raw, ViewMode::Wysiwyg];

fn fix(s: &str, crlf: bool) -> String {
    if crlf {
        s.replace('\n', "\r\n")
    } else {
        s.to_string()
    }
}

/// Run `edit` on `src` in both views, in LF and CRLF, and check the text and
/// that one undo restores the source and one redo the result.
fn check(src: &str, expect: &str, edit: impl Fn(&mut DocumentTab, &str)) {
    for mode in MODES {
        for crlf in [false, true] {
            let (src, expect) = (fix(src, crlf), fix(expect, crlf));
            let mut d = doc(&src, mode);
            edit(&mut d, &src);
            assert_eq!(d.content, expect, "{mode:?} crlf={crlf} from {src:?}");
            assert!(d.undo(), "{mode:?} crlf={crlf}");
            assert_eq!(d.content, src, "one undo, {mode:?} crlf={crlf}");
            assert!(d.redo(), "{mode:?} crlf={crlf}");
            assert_eq!(d.content, expect, "redo, {mode:?} crlf={crlf}");
        }
    }
}

/// Offset just after `needle` in the source.
fn after(src: &str, needle: &str) -> usize {
    src.find(needle).expect("needle") + needle.len()
}

fn enter_after(needle: &'static str) -> impl Fn(&mut DocumentTab, &str) {
    move |d, src| {
        d.place(after(src, needle), false);
        d.smart_newline(false);
    }
}

fn tab_in(needle: &'static str) -> impl Fn(&mut DocumentTab, &str) {
    move |d, src| {
        d.place(after(src, needle), false);
        d.indent();
    }
}

fn shift_tab_in(needle: &'static str) -> impl Fn(&mut DocumentTab, &str) {
    move |d, src| {
        d.place(after(src, needle), false);
        d.outdent();
    }
}

fn select(d: &mut DocumentTab, src: &str, from: &str, to: &str) {
    let start = src.find(from).unwrap();
    d.caret.range = start..after(src, to);
}

fn remove_lines(from: &'static str, to: &'static str) -> impl Fn(&mut DocumentTab, &str) {
    move |d, src| {
        d.caret.range = src.find(from).unwrap()..src.find(to).unwrap();
        let mode = d.view_mode;
        d.smart_backspace_in_mode(mode);
    }
}

fn toggle_ol(from: &'static str, to: &'static str) -> impl Fn(&mut DocumentTab, &str) {
    move |d, src| {
        select(d, src, from, to);
        let mode = d.view_mode;
        d.apply_format_in_mode("ol", mode);
    }
}

// ---- Enter ----

#[test]
fn enter_in_the_middle_shifts_the_following_items_up() {
    check(
        "1. a\n2. b\n3. c",
        "1. a\n2. \n3. b\n4. c",
        enter_after("1. a"),
    );
    check("5. a\n6. b", "5. a\n6. \n7. b", enter_after("5. a"));
    check("1) a\n2) b", "1) a\n2) \n3) b", enter_after("1) a"));
}

#[test]
fn enter_on_the_last_item_has_nothing_to_shift() {
    check("1. a\n2. b", "1. a\n2. b\n3. ", enter_after("2. b"));
}

#[test]
fn enter_mid_text_splits_and_shifts() {
    check("1. ab\n2. c", "1. a\n2. b\n3. c", enter_after("1. a"));
}

#[test]
fn enter_keeps_the_caret_when_a_number_grows() {
    check(
        "9. a\n10. b\n11. c",
        "9. a\n10. \n11. b\n12. c",
        |d, src| {
            d.place(after(src, "9. a"), false);
            d.smart_newline(false);
            let nl = if src.contains("\r\n") { 2 } else { 1 };
            assert_eq!(d.cursor_offset(), "9. a".len() + nl + "10. ".len());
        },
    );
}

#[test]
fn enter_leaves_a_list_that_was_not_in_sequence() {
    check(
        "1. a\n1. b\n1. c",
        "1. a\n2. \n1. b\n1. c",
        enter_after("1. a"),
    );
    check(
        "1. a\n3. b\n4. c",
        "1. a\n2. \n3. b\n4. c",
        enter_after("1. a"),
    );
    check(
        "1. a\n2. b\n4. c",
        "1. a\n2. \n2. b\n4. c",
        enter_after("1. a"),
    );
}

#[test]
fn enter_renumbers_only_the_same_list() {
    // Sub-list items are skipped; a bullet, a heading, or text after a blank
    // line ends the list.
    check(
        "1. a\n   1. x\n2. b",
        "1. a\n2. \n   1. x\n3. b",
        enter_after("1. a"),
    );
    check(
        "1. a\n2. b\n- c\n3. d",
        "1. a\n2. \n3. b\n- c\n3. d",
        enter_after("1. a"),
    );
    check(
        "1. a\n2. b\n# h\n3. c",
        "1. a\n2. \n3. b\n# h\n3. c",
        enter_after("1. a"),
    );
    check(
        "1. a\n2. b\n\ntext\n\n3. c",
        "1. a\n2. \n3. b\n\ntext\n\n3. c",
        enter_after("1. a"),
    );
    check(
        "1. a\n   1. x\n   2. y\n2. b",
        "1. a\n   1. x\n   2. \n   3. y\n2. b",
        enter_after("1. x"),
    );
}

#[test]
fn enter_renumbers_across_a_blank_line_inside_the_list() {
    check(
        "1. a\n\n2. b\n\n3. c",
        "1. a\n2. \n\n3. b\n\n4. c",
        enter_after("1. a"),
    );
}

// ---- Tab and Shift+Tab ----

#[test]
fn tab_renumbers_the_list_the_item_left() {
    check("1. a\n2. b\n3. c", "1. a\n   1. b\n2. c", tab_in("2. b"));
    check(
        "1. a\n2. b\n3. c\n4. d",
        "1. a\n   1. b\n2. c\n3. d",
        tab_in("2. b"),
    );
}

#[test]
fn tab_with_a_selection_renumbers_the_list_it_left() {
    check(
        "1. a\n2. b\n3. c\n4. d",
        "1. a\n   1. b\n   2. c\n2. d",
        |d, src| {
            select(d, src, "2. b", "3. c");
            d.indent();
        },
    );
}

#[test]
fn tab_renumbers_the_list_the_item_joined() {
    check(
        "1. a\n2. b\n   1. y\n3. c",
        "1. a\n   1. b\n   2. y\n2. c",
        tab_in("2. b"),
    );
    check(
        "1. a\n   1. x\n2. b\n   1. y\n3. c",
        "1. a\n   1. x\n   2. b\n   3. y\n2. c",
        tab_in("2. b"),
    );
}

#[test]
fn tab_leaves_a_list_that_was_not_in_sequence() {
    check("1. a\n1. b\n1. c", "1. a\n   1. b\n1. c", tab_in("1. b"));
    check("1. a\n2. b\n5. c", "1. a\n   1. b\n5. c", tab_in("2. b"));
}

#[test]
fn shift_tab_renumbers_both_lists() {
    check(
        "1. a\n   1. x\n   2. y\n2. b",
        "1. a\n2. x\n   1. y\n3. b",
        shift_tab_in("1. x"),
    );
    check(
        "1. a\n   1. x\n   2. y\n2. b",
        "1. a\n   1. x\n2. y\n3. b",
        shift_tab_in("2. y"),
    );
}

#[test]
fn shift_tab_with_a_selection_renumbers_both_lists() {
    check(
        "1. a\n   1. x\n   2. y\n   3. z\n2. b",
        "1. a\n2. x\n3. y\n   1. z\n4. b",
        |d, src| {
            select(d, src, "1. x", "2. y");
            d.outdent();
        },
    );
}

#[test]
fn tab_keeps_the_caret_on_the_same_text_when_a_number_shrinks() {
    check("9. a\n10. b\n11. c", "9. a\n   1. b\n10. c", |d, src| {
        d.place(after(src, "10. b"), false);
        d.indent();
        assert_eq!(d.cursor_offset(), after(&d.content, "1. b"));
    });
}

// ---- Removing items ----

#[test]
fn backspace_on_an_empty_item_shifts_the_following_items_down() {
    check("1. a\n2. \n3. c", "1. a\n\n2. c", |d, src| {
        d.place(after(src, "2. "), false);
        let mode = d.view_mode;
        d.smart_backspace_in_mode(mode);
    });
}

#[test]
fn backspace_at_the_first_marker_keeps_the_list_start() {
    check("9. a\n10. b\n11. c", "a\n9. b\n10. c", |d, _| {
        d.place("9. ".len(), false);
        let mode = d.view_mode;
        d.smart_backspace_in_mode(mode);
        assert_eq!(d.cursor_offset(), 0);
    });
}

#[test]
fn enter_on_an_empty_item_shifts_the_following_items_down() {
    // The empty item's text is cleared and the items after it close the gap.
    for mode in MODES {
        let src = "1. a\n2. \n3. c\n4. d";
        let mut d = doc(src, mode);
        enter_after("2. ")(&mut d, src);
        assert_eq!(d.content, "1. a\n\n2. c\n3. d", "{mode:?}");
        assert!(d.undo());
        assert_eq!(d.content, src, "one undo, {mode:?}");
        // CRLF: the marker takes the CR with it, as it always did.
        let src = fix(src, true);
        let mut d = doc(&src, mode);
        enter_after("2. ")(&mut d, &src);
        assert_eq!(d.content, "1. a\r\n\n2. c\r\n3. d", "{mode:?} crlf");
        assert!(d.undo());
        assert_eq!(d.content, src, "one undo, {mode:?} crlf");
    }
}

#[test]
fn delete_that_joins_an_item_shifts_the_following_items_down() {
    // Delete at the end of a line pulls the next item up and drops its marker.
    for crlf in [false, true] {
        let src = fix("1. a\n2. b\n3. c", crlf);
        let mut d = doc(&src, ViewMode::Wysiwyg);
        d.place(after(&src, "1. a"), false);
        d.delete_forward_in_mode(ViewMode::Wysiwyg);
        assert_eq!(d.content, fix("1. ab\n2. c", crlf), "crlf={crlf}");
        assert!(d.undo());
        assert_eq!(d.content, src);
    }
}

#[test]
fn deleting_whole_item_lines_shifts_the_following_items_down() {
    check(
        "1. a\n2. b\n3. c\n4. d",
        "1. a\n2. c\n3. d",
        remove_lines("2. b", "3. c"),
    );
    check("1. a\n2. b\n3. c", "1. b\n2. c", |d, src| {
        d.caret.range = 0..src.find("2. b").unwrap();
        let mode = d.view_mode;
        d.smart_backspace_in_mode(mode);
    });
    check("1. a\n2. b\n3. c\n4. d", "1. a\n2. d", |d, src| {
        d.caret.range = src.find("2. b").unwrap()..src.find("4. d").unwrap();
        let mode = d.view_mode;
        d.delete_forward_in_mode(mode);
    });
}

#[test]
fn removing_items_leaves_irregular_lists_alone() {
    check(
        "1. a\n1. b\n1. c",
        "1. a\n1. c",
        remove_lines("1. b", "1. c"),
    );
}

#[test]
fn removing_a_bullet_does_not_touch_numbers() {
    check(
        "- a\n- b\n1. c\n3. d",
        "- a\n1. c\n3. d",
        remove_lines("- b", "1. c"),
    );
}

// ---- Numbered-list toggle ----

#[test]
fn toggling_numbering_in_the_middle_of_a_list_numbers_on_from_the_item_above() {
    check(
        "1. a\n- b\n2. c\n3. d",
        "1. a\n2. b\n3. c\n4. d",
        toggle_ol("- b", "- b"),
    );
    check(
        "1. a\n- b\n- c\n2. d",
        "1. a\n2. b\n3. c\n4. d",
        toggle_ol("- b", "- c"),
    );
}

#[test]
fn toggling_numbering_off_closes_the_gap() {
    check(
        "1. a\n2. b\n3. c",
        "1. a\nb\n2. c",
        toggle_ol("2. b", "2. b"),
    );
}

#[test]
fn toggling_numbering_leaves_lazy_lists_alone() {
    check(
        "1. a\n- b\n1. c\n1. d",
        "1. a\n2. b\n1. c\n1. d",
        toggle_ol("- b", "- b"),
    );
}

#[test]
fn toggling_numbering_on_plain_lines_still_starts_at_one() {
    check("x\ny", "1. x\n2. y", |d, _| {
        d.select_all();
        let mode = d.view_mode;
        d.apply_format_in_mode("ol", mode);
    });
}
