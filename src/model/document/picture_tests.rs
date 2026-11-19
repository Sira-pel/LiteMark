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
