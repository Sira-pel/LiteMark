use super::*;

const MODE: ViewMode = ViewMode::Wysiwyg;
const PIC: &str = "![cat](pics/cat.png)";

fn doc(src: &str) -> DocumentTab {
    let mut doc = DocumentTab::new("t".into(), "t.md".into(), src.to_string(), None, None);
    doc.view_mode = MODE;
    doc
}

fn start_of(src: &str) -> usize {
    src.find("![").unwrap()
}

#[test]
fn arrow_keys_never_stop_inside_a_picture_line() {
    let src = format!("intro\n{PIC}\nafter\n");
    let (start, end) = (start_of(&src), start_of(&src) + PIC.len());
    let mut d = doc(&src);

    // Right: end of "intro" -> picture start (line break) -> picture end -> next line.
    d.place(5, false);
    let mut seen = vec![];
    for _ in 0..4 {
        d.move_right_in_mode(false, MODE);
        seen.push(d.cursor_offset());
    }
    assert_eq!(seen, vec![start, end, end + 1, end + 2]);
    assert!(
        seen.iter().all(|p| !(start < *p && *p < end)),
        "caret stepped inside {seen:?}"
    );

    // Left from after the picture: back over it in one step.
    d.place(end + 1, false);
    let mut seen = vec![];
    for _ in 0..3 {
        d.move_left_in_mode(false, MODE);
        seen.push(d.cursor_offset());
    }
    assert_eq!(seen, vec![end, start, start - 1]);
}

#[test]
fn clicking_inside_a_picture_never_leaves_the_caret_there() {
    let src = format!("intro\n{PIC}\nafter\n");
    let start = start_of(&src);
    let mut d = doc(&src);
    d.place(start + 4, false);
    assert_eq!(d.cursor_offset(), start);
    d.place(start + PIC.len() - 2, false);
    assert_eq!(d.cursor_offset(), start + PIC.len());
}

#[test]
fn typing_while_selected_leaves_the_image_markdown_alone() {
    let src = format!("intro\n{PIC}\nafter\n");
    let start = start_of(&src);
    let mut d = doc(&src);
    d.select_picture(start..start + PIC.len());
    assert_eq!(d.selected_picture(), Some(start..start + PIC.len()));

    let sel = d.selection_range();
    d.insert_text_in_mode(sel, "x", MODE);
    assert_eq!(d.content, format!("intro\n{PIC}\nx\nafter\n"));
    assert_eq!(d.cursor_offset(), start + PIC.len() + 2);
}

#[test]
fn typing_at_the_ends_of_a_picture_goes_on_its_own_line() {
    let src = format!("{PIC}\n");
    let mut d = doc(&src);
    d.place(PIC.len(), false);
    d.insert_text_in_mode(PIC.len()..PIC.len(), "a", MODE);
    assert_eq!(d.content, format!("{PIC}\na\n"));

    let mut d = doc(&src);
    d.place(0, false);
    d.insert_text_in_mode(0..0, "a", MODE);
    assert_eq!(d.content, format!("a\n{PIC}\n"));
}

#[test]
fn enter_while_selected_adds_a_line_after_the_picture() {
    let src = format!("{PIC}\nafter\n");
    let mut d = doc(&src);
    d.select_picture(0..PIC.len());
    d.smart_newline(false);
    assert_eq!(d.content, format!("{PIC}\n\nafter\n"));
    assert_eq!(d.cursor_offset(), PIC.len() + 1);
}

#[test]
fn backspace_removes_a_selected_picture_line_and_undo_restores_it() {
    let src = format!("intro\n{PIC}\nafter\n");
    let start = start_of(&src);
    let mut d = doc(&src);
    d.select_picture(start..start + PIC.len());
    d.smart_backspace_in_mode(MODE);
    assert_eq!(d.content, "intro\nafter\n");
    assert!(d.undo());
    assert_eq!(d.content, src);
}

#[test]
fn delete_removes_a_selected_picture_on_the_last_line() {
    let src = format!("intro\n{PIC}");
    let start = start_of(&src);
    let mut d = doc(&src);
    d.select_picture(start..start + PIC.len());
    d.delete_forward_in_mode(MODE);
    assert_eq!(d.content, "intro");
    assert!(d.undo());
    assert_eq!(d.content, src);
}

#[test]
fn f2_shows_the_raw_line_until_the_caret_leaves() {
    let src = format!("intro\n{PIC}\nafter\n");
    let start = start_of(&src);
    let mut d = doc(&src);
    d.select_picture(start..start + PIC.len());
    assert!(d.show_picture_source());
    assert_eq!(d.picture_block_at(start), None, "raw while editing");

    // Typing now edits the Markdown itself.
    let at = d.cursor_offset();
    d.insert_text_in_mode(at..at, "x", MODE);
    assert!(d.content.contains("![cat](pics/cat.png)x"));

    // Leaving the line ends raw editing; the line is plain text now, so fix it first.
    d.edit(d.cursor_offset() - 1..d.cursor_offset(), "");
    d.place(2, false);
    assert_eq!(d.picture_raw_line, None);
    assert!(d.picture_block_at(start).is_some());
}

#[test]
fn ranges_on_a_picture_are_recognised_so_wrapping_can_be_skipped() {
    let src = format!("intro\n{PIC}\nafter\n");
    let start = start_of(&src);
    let mut d = doc(&src);
    assert!(d.range_on_picture(&(start..start + PIC.len())));
    assert!(d.range_on_picture(&(start..start)));
    assert!(!d.range_on_picture(&(0..3)));
}

#[test]
fn input_method_text_gets_its_own_line_after_a_selected_picture() {
    let src = format!("{PIC}\nafter\n");
    let mut d = doc(&src);
    let range = 0..PIC.len();
    let at = d.picture_open_line(&range).expect("on a picture");
    d.edit(at, "你");
    assert_eq!(d.content, format!("{PIC}\n你\nafter\n"));
}

#[test]
fn input_method_text_at_the_start_goes_above_the_picture() {
    let src = format!("{PIC}\n");
    let mut d = doc(&src);
    let at = d.picture_open_line(&(0..0)).expect("on a picture");
    d.edit(at, "あ");
    assert_eq!(d.content, format!("あ\n{PIC}\n"));
}

#[test]
fn paste_target_is_after_the_picture_or_before_at_its_start() {
    let src = format!("{PIC}\n");
    let mut d = doc(&src);
    assert_eq!(
        d.picture_target(&(0..PIC.len())),
        Some(PIC.len()..PIC.len())
    );
    assert_eq!(
        d.picture_target(&(PIC.len()..PIC.len())),
        Some(PIC.len()..PIC.len())
    );
    assert_eq!(d.picture_target(&(0..0)), Some(0..0));
}

#[test]
fn backspace_at_a_picture_start_moves_up_instead_of_joining() {
    let src = format!("intro\n{PIC}\n");
    let start = start_of(&src);
    let mut d = doc(&src);
    d.place(start, false);
    d.smart_backspace_in_mode(MODE);
    assert_eq!(d.content, src);
    assert_eq!(d.cursor_offset(), 5);
}

#[test]
fn delete_at_the_end_of_the_line_above_selects_the_picture() {
    let src = format!("intro\n{PIC}\n");
    let start = start_of(&src);
    let mut d = doc(&src);
    d.place(5, false);
    d.delete_forward_in_mode(MODE);
    assert_eq!(d.content, src);
    assert_eq!(d.selected_picture(), Some(start..start + PIC.len()));
}

#[test]
fn backspace_below_a_picture_selects_it_instead_of_joining() {
    let src = format!("{PIC}\nafter\n");
    let mut d = doc(&src);
    d.place(PIC.len() + 1, false);
    d.smart_backspace_in_mode(MODE);
    assert_eq!(d.content, src);
    assert_eq!(d.selected_picture(), Some(0..PIC.len()));
}

#[test]
fn delete_right_after_a_picture_steps_to_the_next_line() {
    let src = format!("{PIC}\nafter\n");
    let mut d = doc(&src);
    d.place(PIC.len(), false);
    d.delete_forward_in_mode(MODE);
    assert_eq!(d.content, src);
    assert_eq!(d.cursor_offset(), PIC.len() + 1);
}

#[test]
fn cancelled_input_method_text_takes_its_line_back() {
    for (src, range) in [
        (format!("{PIC}\nafter\n"), 0..PIC.len()),
        (format!("{PIC}\n"), 0..0),
    ] {
        let mut d = doc(&src);
        let at = d.picture_open_line(&range).expect("on a picture");
        d.edit(at.clone(), "あ");
        d.edit(at.start..at.start + "あ".len(), "");
        d.cancel_picture_ime_line();
        assert_eq!(d.content, src);
        assert!(!d.undo(), "a cancelled input leaves no undo step");
        assert_eq!(d.content, src);
    }
}

#[test]
fn input_method_line_and_text_undo_in_one_step() {
    for (src, range, expect) in [
        (
            format!(
                "{PIC}
after
"
            ),
            0..PIC.len(),
            format!(
                "{PIC}
你
after
"
            ),
        ),
        (
            format!(
                "{PIC}
"
            ),
            0..0,
            format!(
                "你
{PIC}
"
            ),
        ),
    ] {
        let mut d = doc(&src);
        let at = d.picture_open_line(&range).expect("on a picture");
        d.edit(at.clone(), "你");
        assert_eq!(d.content, expect);
        assert!(d.undo());
        assert_eq!(d.content, src, "one undo undoes line and text");
        assert!(!d.undo());
    }
}

#[test]
fn picture_text_follows_the_notes_crlf_line_endings() {
    let src = format!(
        "{PIC}
after
"
    );
    let mut d = doc(&src);
    d.select_picture(0..PIC.len());
    let sel = d.selection_range();
    d.insert_text_in_mode(sel, "x", MODE);
    assert_eq!(
        d.content,
        format!(
            "{PIC}
x
after
"
        )
    );
}

#[test]
fn formatting_never_wraps_a_picture() {
    let src = format!("intro\n{PIC}\n");
    let start = start_of(&src);
    for cmd in [
        "bold",
        "italic",
        "underline",
        "strike",
        "code",
        "h1",
        "ul",
        "quote",
    ] {
        let mut d = doc(&src);
        d.select_picture(start..start + PIC.len());
        d.apply_format_in_mode(cmd, MODE);
        assert_eq!(d.content, src, "{cmd} changed the picture line");
    }
}

#[test]
fn word_deletes_remove_the_picture_not_pieces_of_its_markdown() {
    let src = format!("intro\n{PIC}\nafter\n");
    let start = start_of(&src);
    let mut d = doc(&src);
    d.place(start + PIC.len(), false);
    d.delete_word_left();
    assert_eq!(d.content, "intro\nafter\n");

    let mut d = doc(&src);
    d.place(start, false);
    d.delete_word_right();
    assert_eq!(d.content, "intro\nafter\n");
}

#[test]
fn cutting_a_selected_picture_removes_its_line_and_indent_leaves_it_alone() {
    let src = format!("intro\n{PIC}\nafter\n");
    let start = start_of(&src);
    let mut d = doc(&src);
    d.select_picture(start..start + PIC.len());
    d.indent();
    d.outdent();
    assert_eq!(d.content, src);
    assert!(d.cut_selected_picture());
    assert_eq!(d.content, "intro\nafter\n");
}

#[test]
fn ctrl_backspace_below_a_picture_selects_it_instead_of_joining() {
    let src = format!("{PIC}\nafter\n");
    let mut d = doc(&src);
    d.place(PIC.len() + 1, false);
    d.delete_word_left();
    assert_eq!(d.content, src, "the picture's Markdown is untouched");
    assert_eq!(d.selection_range(), 0..PIC.len());
}

#[test]
fn text_typed_after_committed_input_method_text_is_its_own_undo_step() {
    let src = format!("{PIC}\nafter\n");
    let mut d = doc(&src);
    let at = d.picture_open_line(&(0..PIC.len())).expect("on a picture");
    d.edit(at.clone(), "你");
    // What the input handler does once the text is committed.
    d.seal_history();
    let end = at.start + "你".len();
    d.edit(end..end, "x");
    assert!(d.undo());
    assert_eq!(d.content, format!("{PIC}\n你\nafter\n"));
}

#[test]
fn line_formats_never_touch_a_picture_line() {
    let src = format!("intro\n{PIC}\nafter\n");
    let end = start_of(&src) + PIC.len();
    for caret in [start_of(&src), end] {
        for cmd in ["h1", "ul", "ol", "checklist", "quote", "codeblock", "hr"] {
            let mut d = doc(&src);
            d.place(caret, false);
            d.apply_format_in_mode(cmd, MODE);
            assert_eq!(d.content, src, "{cmd} at {caret}");
        }
    }
}

#[test]
fn bold_beside_a_picture_applies_to_the_text_typed_next() {
    let src = format!("{PIC}\nafter\n");
    let mut d = doc(&src);
    d.place(PIC.len(), false);
    d.apply_format_in_mode("bold", MODE);
    assert!(d.pending_formats.bold);
    let at = d.cursor_offset();
    d.insert_text_in_mode(at..at, "hi", MODE);
    assert_eq!(d.content, format!("{PIC}\n**hi**\nafter\n"));
}

#[test]
fn link_beside_a_picture_goes_on_its_own_line() {
    let src = format!("{PIC}\nafter\n");
    let mut d = doc(&src);
    d.place(PIC.len(), false);
    d.apply_format_in_mode("link", MODE);
    assert_eq!(d.content, format!("{PIC}\n[link](https://)\nafter\n"));
    assert_eq!(&d.content[d.selection_range()], "link");

    let mut d = doc(&src);
    d.place(0, false);
    d.apply_format_in_mode("link", MODE);
    assert_eq!(d.content, format!("[link](https://)\n{PIC}\nafter\n"));
}

#[test]
fn bold_text_beside_a_picture_is_one_undo_step() {
    for (caret, expect) in [
        (PIC.len(), format!("{PIC}\n**hi**\nafter\n")),
        (0, format!("**hi**\n{PIC}\nafter\n")),
    ] {
        let src = format!("{PIC}\nafter\n");
        let mut d = doc(&src);
        d.place(caret, false);
        d.apply_format_in_mode("bold", MODE);
        let at = d.cursor_offset();
        d.insert_text_in_mode(at..at, "hi", MODE);
        assert_eq!(d.content, expect);
        assert!(d.undo());
        assert_eq!(d.content, src, "one undo takes back line and text");
        assert!(!d.undo());
    }
}

#[test]
fn cycling_the_size_steps_through_the_presets_and_back() {
    let src = format!("intro\n{PIC}\nafter\n");
    let start = start_of(&src);
    let mut d = doc(&src);
    d.select_picture(start..start + PIC.len());
    let line = |d: &mut DocumentTab| d.content.lines().nth(1).unwrap().to_string();
    for expected in [
        "![cat|200](pics/cat.png)",
        "![cat|400](pics/cat.png)",
        "![cat|600](pics/cat.png)",
        PIC,
    ] {
        assert!(d.cycle_picture_size());
        assert_eq!(line(&mut d), expected);
        // The picture stays selected, so the next press resizes it again.
        assert!(d.selected_picture().is_some());
    }
}

#[test]
fn a_size_change_is_one_undo_step() {
    let src = format!("{PIC}\n");
    let mut d = doc(&src);
    d.select_picture(0..PIC.len());
    assert!(d.set_picture_width(Some(300)));
    assert!(d.content.starts_with("![cat|300](pics/cat.png)"));
    d.undo();
    assert_eq!(d.content.as_ref() as &str, src);
}

#[test]
fn sizing_does_nothing_off_a_picture() {
    let mut d = doc("plain text\n");
    d.place(2, false);
    assert!(!d.cycle_picture_size());
    assert_eq!(d.content.as_ref() as &str, "plain text\n");
}

const ROW: &str = "![a](a.png) ![b](b.png) ![c](c.png)";
const A: std::ops::Range<usize> = 0..11;
const B: std::ops::Range<usize> = 12..23;
const C: std::ops::Range<usize> = 24..35;

fn row_doc() -> DocumentTab {
    doc(&format!("{ROW}\nafter\n"))
}

#[test]
fn a_row_of_images_is_one_picture_block() {
    let mut d = row_doc();
    assert_eq!(d.picture_block_at(14), Some(0..ROW.len()));
    // The caret rests only before or after the row.
    d.place(14, false);
    assert!([0, ROW.len()].contains(&d.cursor_offset()));
}

#[test]
fn one_image_of_a_row_can_be_selected_and_stays_selected() {
    let mut d = row_doc();
    d.select_picture(B);
    assert_eq!(d.selected_image(), Some(B));
    assert_eq!(d.selected_picture(), None);
    d.expire_picture_raw();
    d.refresh_cursor();
    assert_eq!(d.selection_range(), B);
    // The whole row is the picture, not one of its images.
    d.select_picture(0..ROW.len());
    assert_eq!(d.selected_image(), None);
    assert_eq!(d.selected_picture(), Some(0..ROW.len()));
}

#[test]
fn deleting_an_image_takes_the_blank_beside_it() {
    for (image, left, selected) in [
        (A, "![b](b.png) ![c](c.png)", 0..11),
        (B, "![a](a.png) ![c](c.png)", 12..23),
        (C, "![a](a.png) ![b](b.png)", 12..23),
    ] {
        let mut d = row_doc();
        d.select_picture(image);
        d.smart_backspace_in_mode(MODE);
        assert_eq!(d.content, format!("{left}\nafter\n"));
        assert_eq!(d.selection_range(), selected);
        assert!(d.undo());
        assert_eq!(d.content, format!("{ROW}\nafter\n"));
    }
}

#[test]
fn two_images_left_become_one_then_a_plain_picture_line() {
    let mut d = doc("![a](a.png) ![b](b.png)\n");
    d.select_picture(0..11);
    d.delete_forward_in_mode(MODE);
    assert_eq!(d.content, "![b](b.png)\n");
    // One image left: the whole line is the picture again.
    assert_eq!(d.selected_picture(), Some(0..11));
}

#[test]
fn resizing_changes_only_the_selected_image() {
    let mut d = row_doc();
    d.select_picture(B);
    assert!(d.cycle_picture_size());
    assert_eq!(
        d.content,
        "![a](a.png) ![b|200](b.png) ![c](c.png)\nafter\n"
    );
    assert_eq!(d.selected_image(), Some(12..27));
    // With the whole row selected nothing is resized.
    d.select_picture(0..d.content.find('\n').unwrap());
    let before = d.content.clone();
    assert!(!d.cycle_picture_size());
    assert_eq!(d.content, before);
}

#[test]
fn typing_with_an_image_selected_goes_on_a_new_line_after_the_row() {
    let mut d = row_doc();
    d.select_picture(B);
    let sel = d.selection_range();
    d.insert_text_in_mode(sel, "x", MODE);
    assert_eq!(d.content, format!("{ROW}\nx\nafter\n"));
}

#[test]
fn cutting_one_image_leaves_the_rest() {
    let mut d = row_doc();
    d.select_picture(A);
    assert!(d.cut_selected_picture());
    assert_eq!(d.content, "![b](b.png) ![c](c.png)\nafter\n");
}

#[test]
fn formatting_never_touches_one_selected_image_of_a_row() {
    for cmd in [
        "bold",
        "italic",
        "underline",
        "strike",
        "code",
        "h1",
        "ul",
        "ol",
        "checklist",
        "quote",
        "codeblock",
        "link",
    ] {
        let mut d = row_doc();
        d.select_picture(B);
        d.apply_format_in_mode(cmd, MODE);
        assert_eq!(d.content, format!("{ROW}\nafter\n"), "{cmd}");
    }
    let mut d = row_doc();
    d.select_picture(B);
    d.indent();
    d.outdent();
    assert_eq!(d.content, format!("{ROW}\nafter\n"));
}

#[test]
fn row_edits_follow_crlf_and_work_on_the_last_line() {
    let mut d = doc(&format!("x\r\n{ROW}"));
    let b = 3 + B.start..3 + B.end;
    d.select_picture(b);
    d.delete_forward_in_mode(MODE);
    assert_eq!(d.content, "x\r\n![a](a.png) ![c](c.png)");
    // Last image of the last line, no trailing line break.
    let last = 3 + 12..3 + 23;
    d.select_picture(last);
    d.smart_backspace_in_mode(MODE);
    assert_eq!(d.content, "x\r\n![a](a.png)");
    assert!(d.undo());
    assert_eq!(d.content, "x\r\n![a](a.png) ![c](c.png)");
}

#[test]
fn a_row_with_trailing_blanks_or_tabs_still_edits_cleanly() {
    let mut d = doc("![a](a.png)\t![b](b.png)  \n");
    assert_eq!(d.picture_block_at(3), Some(0..25));
    d.select_picture(12..23);
    d.smart_backspace_in_mode(MODE);
    assert_eq!(d.content, "![a](a.png)  \n");
    // Remaining single image still behaves as a picture.
    assert!(d.picture_block_at(3).is_some());
}

#[test]
fn size_changes_and_deletes_redo_after_undo() {
    let mut d = row_doc();
    d.select_picture(B);
    assert!(d.cycle_picture_size());
    let sized = d.content.clone();
    assert!(d.undo());
    assert_eq!(d.content, format!("{ROW}\nafter\n"));
    assert!(d.redo());
    assert_eq!(d.content, sized);
    // Two steps back and forward with a delete in between.
    d.select_picture(12..27);
    d.smart_backspace_in_mode(MODE);
    let deleted = d.content.clone();
    assert!(d.undo());
    assert_eq!(d.content, sized);
    assert!(d.redo());
    assert_eq!(d.content, deleted);
}

#[test]
fn a_single_picture_cycles_sizes_and_keeps_its_title() {
    let line = "![cat|600](<my cat.png> \"A cat\")";
    let mut d = doc(&format!("{line}\n"));
    d.select_picture(0..line.len());
    assert!(d.cycle_picture_size());
    assert_eq!(d.content, "![cat](<my cat.png> \"A cat\")\n");
    assert!(d.cycle_picture_size());
    assert_eq!(d.content, "![cat|200](<my cat.png> \"A cat\")\n");
    // A width that is not a preset starts again at the first one.
    assert!(d.set_picture_width(Some(333)));
    assert!(d.cycle_picture_size());
    assert!(d.content.starts_with("![cat|200]"));
}

#[test]
fn sizing_a_picture_with_the_caret_beside_it_resizes_the_whole_picture() {
    let mut d = doc("![a](a.png)\n");
    d.place(11, false);
    assert!(d.cycle_picture_size());
    assert_eq!(d.content, "![a|200](a.png)\n");
}

#[test]
fn in_raw_view_pictures_are_plain_text_and_sizing_does_nothing() {
    let mut d = row_doc();
    d.view_mode = ViewMode::Raw;
    d.place(14, false);
    assert!(!d.cycle_picture_size());
    assert_eq!(d.content, format!("{ROW}\nafter\n"));
}

#[test]
fn f2_shows_a_rows_markdown_for_editing_until_the_caret_leaves() {
    let mut d = row_doc();
    d.select_picture(B);
    assert!(d.show_picture_source());
    // Raw while editing: no picture block, so text goes into the Markdown.
    assert_eq!(d.picture_block_at(5), None);
    let at = d.cursor_offset();
    d.insert_text_in_mode(at..at, " ", MODE);
    assert!(d.content.starts_with("![a](a.png) ![b](b.png) "));
    // Moving off the line makes it a row of pictures again.
    d.place(d.content.len(), false);
    d.expire_picture_raw();
    assert!(d.picture_block_at(5).is_some());
}

#[test]
fn enter_with_one_image_selected_opens_a_line_after_the_row() {
    for plain in [false, true] {
        let mut d = row_doc();
        d.select_picture(B);
        if plain {
            d.smart_newline(true);
        } else {
            d.smart_newline(false);
        }
        assert_eq!(d.content, format!("{ROW}\n\nafter\n"), "plain={plain}");
    }
}

#[test]
fn backspace_after_a_row_selects_its_last_image_first() {
    let mut d = row_doc();
    d.place(ROW.len(), false);
    d.smart_backspace_in_mode(MODE);
    assert_eq!(d.content, format!("{ROW}\nafter\n"));
    assert_eq!(d.selected_image(), Some(C));
    // The next press removes only that image.
    d.smart_backspace_in_mode(MODE);
    assert_eq!(d.content, "![a](a.png) ![b](b.png)\nafter\n");
}

#[test]
fn delete_before_a_row_selects_its_first_image_first() {
    let mut d = row_doc();
    d.place(0, false);
    d.delete_forward_in_mode(MODE);
    assert_eq!(d.content, format!("{ROW}\nafter\n"));
    assert_eq!(d.selected_image(), Some(A));
    d.delete_forward_in_mode(MODE);
    assert_eq!(d.content, "![b](b.png) ![c](c.png)\nafter\n");
}

#[test]
fn setting_a_width_at_a_range_changes_only_that_picture() {
    let mut d = row_doc();
    // The caret is elsewhere; the range alone says which image.
    d.place(ROW.len() + 3, false);
    assert!(d.set_picture_width_at(B, Some(333)));
    assert_eq!(
        d.content,
        "![a](a.png) ![b|333](b.png) ![c](c.png)\nafter\n"
    );
    assert_eq!(d.selected_image(), Some(12..27));
    // One undo step.
    assert!(d.undo());
    assert_eq!(d.content, format!("{ROW}\nafter\n"));
}

#[test]
fn setting_the_same_width_is_no_edit_and_no_undo_step() {
    let mut d = doc("![a|200](a.png)\n");
    let before = d.content.clone();
    assert!(!d.set_picture_width_at(0..15, Some(200)));
    assert_eq!(d.content, before);
    assert!(!d.undo());
    // A lone picture takes a width from its first drag.
    let mut d = doc("![a](a.png)\n");
    assert!(d.set_picture_width_at(0..11, Some(120)));
    assert_eq!(d.content, "![a|120](a.png)\n");
    assert!(!d.set_picture_width_at(0..3, Some(5)));
}

#[test]
fn a_width_and_height_are_written_and_a_later_width_drops_the_height() {
    let mut d = doc("![a](a.png)\n");
    assert!(d.set_picture_size_at(0..11, Some(120), Some(40)));
    assert_eq!(d.content, "![a|120x40](a.png)\n");
    assert!(!d.set_picture_size_at(0..19, Some(120), Some(40)));
    // F3 steps by width alone, so the stretch is gone.
    d.place(3, false);
    assert!(d.cycle_picture_size());
    assert_eq!(d.content, "![a|200](a.png)\n");
    assert!(d.undo());
    assert_eq!(d.content, "![a|120x40](a.png)\n");
}

#[test]
fn one_image_of_a_row_takes_a_width_and_height_by_itself() {
    let mut d = row_doc();
    assert!(d.set_picture_size_at(B, Some(90), Some(60)));
    assert_eq!(
        d.content,
        "![a](a.png) ![b|90x60](b.png) ![c](c.png)\nafter\n"
    );
}

#[test]
fn a_read_only_tab_ignores_every_picture_resize() {
    let mut d = doc("![cat](c.png)\n");
    d.file_type = FileType::Pdf;
    d.select_picture(0..13);
    let before = (d.content.clone(), d.selection_range());
    assert!(!d.set_picture_width(Some(200)));
    assert!(!d.set_picture_size_at(0..13, Some(200), Some(100)));
    assert!(!d.cycle_picture_size());
    assert_eq!((d.content.clone(), d.selection_range()), before);
}

#[test]
fn ctrl_left_and_right_jump_cleanly_over_picture_blocks() {
    let text = "hello\n![cat](pics/cat.png)\nworld";
    let pic_start = text.find("![").unwrap();
    let pic_end = text.find("\nworld").unwrap();
    let world_start = text.find("world").unwrap();
    let mut d = doc(text);

    // Forward walk: 0 -> end of "hello" -> start of picture -> end of picture -> end of "world".
    d.place(0, false);
    d.move_word_right(false);
    assert_eq!(d.cursor_offset(), 5);
    d.move_word_right(false);
    assert_eq!(
        d.cursor_offset(),
        pic_start,
        "Ctrl+Right from before a picture stops at its start"
    );
    d.move_word_right(false);
    assert_eq!(
        d.cursor_offset(),
        pic_end,
        "Ctrl+Right from picture start jumps to its end"
    );
    d.move_word_right(false);
    assert_eq!(d.cursor_offset(), text.len());

    // Backward walk: end -> start of "world" -> start of picture -> 0.
    d.move_word_left(false);
    assert_eq!(d.cursor_offset(), world_start);
    d.move_word_left(false);
    assert_eq!(
        d.cursor_offset(),
        pic_start,
        "Ctrl+Left must jump before picture"
    );
    d.move_word_left(false);
    assert_eq!(d.cursor_offset(), 0);

    // Starting right at picture edges jumps across the whole block in one step.
    d.place(pic_start, false);
    d.move_word_right(false);
    assert_eq!(d.cursor_offset(), pic_end);
    d.move_word_left(false);
    assert_eq!(d.cursor_offset(), pic_start);

    // Shift+Ctrl+Right / Left extends selection cleanly over the block.
    d.place(5, false);
    d.move_word_right(true);
    assert_eq!(d.selection_range(), 5..pic_start);
    d.move_word_right(true);
    assert_eq!(d.selection_range(), 5..pic_end);
    d.place(world_start, false);
    d.move_word_left(true);
    assert_eq!(d.selection_range(), pic_start..world_start);

    // Consecutive pictures: Ctrl+Right from end of A stops at start of B, then jumps to end of B.
    let two = "![a](a.png)\n![b](b.png)";
    let a_end = "![a](a.png)".len();
    let b_start = two.find("![b").unwrap();
    let mut d2 = doc(two);
    d2.place(0, false);
    d2.move_word_right(false);
    assert_eq!(d2.cursor_offset(), a_end);
    d2.move_word_right(false);
    assert_eq!(
        d2.cursor_offset(),
        b_start,
        "Ctrl+Right from end of first picture must stop at start of next picture"
    );
    d2.move_word_right(false);
    assert_eq!(d2.cursor_offset(), two.len());
    d2.move_word_left(false);
    assert_eq!(d2.cursor_offset(), b_start);
    d2.move_word_left(false);
    assert_eq!(d2.cursor_offset(), 0);
}

#[test]
fn raw_to_wysiwyg_toggle_snaps_caret_out_of_picture_interior() {
    let text = "![hello](pics/cat.png)\n";
    let mut d = doc(text);
    d.set_view_mode(ViewMode::Raw);
    d.place(4, false); // near start inside ![hello](...)
    assert_eq!(d.cursor_offset(), 4);
    d.set_view_mode(ViewMode::Wysiwyg);
    assert_eq!(
        d.cursor_offset(),
        0,
        "caret near start of picture should snap to block.start"
    );

    d.set_view_mode(ViewMode::Raw);
    d.place(18, false); // near end inside ![hello](...)
    assert_eq!(d.cursor_offset(), 18);
    d.set_view_mode(ViewMode::Wysiwyg);
    assert_eq!(
        d.cursor_offset(),
        "![hello](pics/cat.png)".len(),
        "caret near end of picture should snap to block.end"
    );
}

#[test]
fn undo_and_redo_on_thumbnail_row_restore_selection_without_trapping_caret() {
    // 1. set_picture_width on middle thumbnail
    let mut d = row_doc();
    d.select_picture(B);
    assert!(d.set_picture_width(Some(200)));
    let resized_b = B.start..B.start + "![b|200](b.png)".len();
    assert_eq!(d.selection_range(), resized_b);
    assert!(d.undo());
    assert_eq!(
        d.selection_range(),
        B,
        "undo of thumbnail width must restore thumbnail selection"
    );
    assert!(d.redo());
    assert_eq!(
        d.selection_range(),
        resized_b,
        "redo of thumbnail width must restore resized thumbnail selection"
    );

    // 2. set_picture_size_at on middle thumbnail
    let mut d = row_doc();
    d.select_picture(B);
    assert!(d.set_picture_size_at(B, Some(150), Some(90)));
    let sized_b = B.start..B.start + "![b|150x90](b.png)".len();
    assert_eq!(d.selection_range(), sized_b);
    assert!(d.undo());
    assert_eq!(d.selection_range(), B);
    assert!(d.redo());
    assert_eq!(d.selection_range(), sized_b);

    // 3. remove_row_image (Backspace on middle thumbnail)
    let mut d = row_doc();
    d.select_picture(B);
    assert!(d.smart_backspace_in_mode(MODE));
    assert_eq!(d.content, "![a](a.png) ![c](c.png)\nafter\n");
    let after_sel = d.selection_range();
    assert!(!after_sel.is_empty());
    assert!(d.undo());
    assert_eq!(d.content, format!("{ROW}\nafter\n"));
    assert_eq!(
        d.selection_range(),
        B,
        "undo of thumbnail removal must restore removed thumbnail selection"
    );
    assert!(d.redo());
    assert_eq!(d.content, "![a](a.png) ![c](c.png)\nafter\n");
    assert_eq!(
        d.selection_range(),
        after_sel,
        "redo of thumbnail removal must restore surviving thumbnail selection"
    );
}

#[test]
fn find_match_inside_wysiwyg_picture_opens_raw_picture_line() {
    let text = "intro\n![needle](pics/cat.png)\noutro needle\n";
    let mut d = doc(text);
    d.place(0, false);

    let first = d.select_match("needle", false, None, true);
    assert_eq!(first, Some(0));
    let pic_start = text.find("![").unwrap();
    let hit_start = text.find("needle").unwrap();
    assert_eq!(d.selection_range(), hit_start..hit_start + "needle".len());
    assert_eq!(
        d.picture_raw_line,
        Some(pic_start),
        "Find match inside a WYSIWYG picture must expose raw Markdown"
    );
    assert_eq!(d.picture_block_at(hit_start), None);

    // Moving to the next match on a normal line expires raw picture editing.
    let second = d.select_match("needle", false, first, true);
    assert_eq!(second, Some(1));
    assert_eq!(d.picture_raw_line, None);
    assert!(d.picture_block_at(pic_start).is_some());
}

#[test]
fn undo_and_redo_of_whole_picture_removal_restores_selection_and_caret() {
    let text = "intro\n![cat](cat.png)\noutro\n";
    let pic_start = text.find("![").unwrap();
    let pic_end = text.find("\noutro").unwrap();
    let mut d = doc(text);
    d.select_picture(pic_start..pic_end);
    assert!(d.smart_backspace_in_mode(MODE));
    assert_eq!(d.content, "intro\noutro\n");
    assert_eq!(d.selection_range(), pic_start..pic_start);

    assert!(d.undo());
    assert_eq!(d.content, text);
    assert_eq!(d.selection_range(), pic_start..pic_end);

    assert!(d.redo());
    assert_eq!(d.content, "intro\noutro\n");
    assert_eq!(d.selection_range(), pic_start..pic_start);
}

#[test]
fn finish_ime_composition_preserves_khmer_preedit_and_cancels_empty_line() {
    // Active Khmer preedit on an IME-opened line after a picture: kept and sealed.
    let mut d = doc("![cat](cat.png)\n");
    let end = "![cat](cat.png)".len();
    let opened = d.picture_open_line(&(end..end)).expect("opened line");
    let khmer = "សួស្តី";
    d.edit(opened.clone(), khmer);
    d.marked_range = Some(opened.start..opened.start + khmer.len());
    d.finish_ime_composition();
    assert!(d.marked_range.is_none());
    assert!(d.ime_opened_line.is_none());
    assert!(
        d.content.contains(khmer),
        "composed Khmer text must be preserved, got {:?}",
        d.content
    );
    assert_eq!(d.cursor_offset(), opened.start + khmer.len());

    // Empty IME-opened line with no marked text: cancelled cleanly.
    let mut d2 = doc("![cat](cat.png)\n");
    let _ = d2.picture_open_line(&(end..end)).expect("opened line");
    d2.finish_ime_composition();
    assert_eq!(d2.content, "![cat](cat.png)\n");
    assert!(d2.ime_opened_line.is_none());
}

#[test]
fn snap_wysiwyg_table_delimiter_handles_crlf_and_header_only_eof() {
    // CRLF table: must land on start of body row, never between \r and \n.
    let crlf = "| a | b |\r\n| --- | --- |\r\n| 1 | 2 |\r\n";
    let delim = crlf.find("| ---").unwrap() + 2;
    let body_start = crlf.find("| 1").unwrap();
    let mut d = doc(crlf);
    d.set_view_mode(ViewMode::Raw);
    d.place(delim, false);
    d.set_view_mode(ViewMode::Wysiwyg);
    d.snap_wysiwyg_table_delimiter();
    assert_eq!(
        d.cursor_offset(),
        body_start,
        "CRLF table delimiter snap must skip full \\r\\n"
    );

    // Header-only table at EOF (no trailing newline): must snap into header row.
    let header_only = "| a | b |\r\n| --- | --- |";
    let delim_eof = header_only.find("| ---").unwrap() + 2;
    let mut d2 = doc(header_only);
    d2.set_view_mode(ViewMode::Raw);
    d2.place(delim_eof, false);
    d2.set_view_mode(ViewMode::Wysiwyg);
    d2.snap_wysiwyg_table_delimiter();
    assert!(
        d2.cursor_offset() < header_only.find("\r\n").unwrap(),
        "header-only table at EOF must snap to header row, got {}",
        d2.cursor_offset()
    );
}
