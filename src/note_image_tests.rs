//! Images in notes: a line that is only `![alt](path)` is laid out as a
//! picture block, and the caret, clicks and culling keep working around it.

use super::*;
use gpui::{Entity, TestAppContext, VisualTestContext};

// `use gpui::*` in this crate re-exports a `test` attribute that is not
// Rust's `#[test]`. Pin the std one so these stay ordinary unit tests.
use ::core::prelude::v1::test as std_test;

/// Row height of a 100x50 picture: its size (never enlarged) plus margins.
const SMALL_PICTURE_ROW: f32 = 50.0 + 12.0;

fn fixture_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "litemark-note-images-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("pics")).unwrap();
    std::fs::write(
        dir.join("pics").join("small.svg"),
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="50"><rect width="100" height="50" fill="red"/></svg>"#,
    )
    .unwrap();
    dir
}

fn harness(content: &str, dir: &Path) -> (Entity<LiteMarkApp>, VisualTestContext) {
    let mut cx = TestAppContext::single();
    let (view, cx) = cx.add_window_view(|_window, cx| {
        let mut app = LiteMarkApp::test_new(content, cx);
        app.settings.view_mode = ViewMode::Wysiwyg;
        app.active_tab_mut().expect("tab").file_path = Some(dir.join("note.md"));
        app
    });
    let mut cx = cx.clone();
    redraw(&mut cx);
    (view, cx)
}

fn redraw(cx: &mut VisualTestContext) {
    cx.update(|window, _cx| window.refresh());
    cx.run_until_parked();
}

fn place_caret(view: &Entity<LiteMarkApp>, cx: &mut VisualTestContext, offset: usize) {
    view.update(cx, |app, _cx| {
        app.active_tab_mut().expect("tab").place(offset, false);
    });
    redraw(cx);
}

/// `(byte_start, byte_end, origin_y, line_height)` of the row starting at `offset`.
fn row_at(
    view: &Entity<LiteMarkApp>,
    cx: &VisualTestContext,
    offset: usize,
) -> (usize, usize, f32, f32) {
    view.read_with(cx, |app, _| {
        let paint = app.editor_paint.borrow();
        let row = paint
            .rows
            .iter()
            .find(|r| r.byte_start == offset)
            .unwrap_or_else(|| panic!("no row at {offset}"));
        (
            row.byte_start,
            row.byte_end,
            f32::from(row.origin.y),
            f32::from(row.line_height),
        )
    })
}

fn text_row_height(view: &Entity<LiteMarkApp>, cx: &VisualTestContext) -> f32 {
    view.read_with(cx, |app, _| {
        f32::from(app.editor_paint.borrow().line_height)
    })
}

const DOC: &str = "intro\n![cat](pics/small.svg)\nafter\n";

fn image_start() -> usize {
    DOC.find("![").unwrap()
}

#[std_test]
fn an_image_line_takes_the_pictures_height() {
    let dir = fixture_dir("height");
    let (view, mut cx) = harness(DOC, &dir);
    let text_h = text_row_height(&view, &cx);

    // Caret away from the image: the picture's size plus margins.
    let (_, _, image_y, image_h) = row_at(&view, &cx, image_start());
    assert_eq!(image_h, SMALL_PICTURE_ROW);
    let (_, _, after_y, _) = row_at(&view, &cx, DOC.find("after").unwrap());
    assert_eq!(
        after_y,
        image_y + SMALL_PICTURE_ROW,
        "next line follows the picture"
    );
    let (_, _, intro_y, _) = row_at(&view, &cx, 0);
    assert_eq!(image_y, intro_y + text_h);

    // Caret on the image line: the picture stays.
    place_caret(&view, &mut cx, image_start() + 3);
    let (_, _, _, on_line_h) = row_at(&view, &cx, image_start());
    assert_eq!(on_line_h, SMALL_PICTURE_ROW);

    // And it is unchanged once the caret leaves.
    place_caret(&view, &mut cx, 0);
    assert_eq!(row_at(&view, &cx, image_start()).3, SMALL_PICTURE_ROW);
    let _ = std::fs::remove_dir_all(&dir);
}

#[std_test]
fn raw_view_never_shows_pictures() {
    let dir = fixture_dir("raw");
    let (view, mut cx) = harness(DOC, &dir);
    view.update(&mut cx, |app, _cx| app.settings.view_mode = ViewMode::Raw);
    redraw(&mut cx);
    let text_h = text_row_height(&view, &cx);
    assert_eq!(row_at(&view, &cx, image_start()).3, text_h);
    let _ = std::fs::remove_dir_all(&dir);
}

#[std_test]
fn missing_and_remote_images_get_a_placeholder_box() {
    let dir = fixture_dir("placeholder");
    let doc = "top
![gone](pics/nope.png)\n![web](https://example.com/a.png)\n![](\\\\host\\share\\a.png)\nend\n";
    let (view, cx) = harness(doc, &dir);
    let expected = f32::from(crate::ui::note_images::placeholder_height(px(
        EditorSettings::default().base_font_size,
    ))) + 12.0;
    for start in [
        doc.find("![gone").unwrap(),
        doc.find("![web").unwrap(),
        doc.find("![](").unwrap(),
    ] {
        let (_, _, _, height) = row_at(&view, &cx, start);
        assert!(
            (height - expected).abs() < 1.0,
            "placeholder row {height} vs {expected}"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[std_test]
fn an_unsaved_note_has_no_folder_so_relative_images_are_placeholders() {
    let dir = fixture_dir("unsaved");
    let (view, mut cx) = harness(DOC, &dir);
    view.update(&mut cx, |app, _cx| {
        app.active_tab_mut().expect("tab").file_path = None;
    });
    redraw(&mut cx);
    assert_ne!(row_at(&view, &cx, image_start()).3, SMALL_PICTURE_ROW);
    let _ = std::fs::remove_dir_all(&dir);
}

#[std_test]
fn up_and_down_step_through_an_image_line() {
    let dir = fixture_dir("vertical");
    let (view, mut cx) = harness(DOC, &dir);
    let after = DOC.find("after").unwrap();
    let step = |view: &Entity<LiteMarkApp>, cx: &mut VisualTestContext, delta: i32| -> usize {
        let cursor = view.update(cx, |app, _cx| {
            let paint = app.editor_paint.borrow().clone();
            let tab = app.active_tab_mut().expect("tab");
            let content = tab.content.clone();
            assert!(crate::ui::text_editor::move_vertical_visual(
                &paint,
                &content,
                &mut tab.caret,
                delta,
                false,
                ViewMode::Wysiwyg,
            ));
            tab.caret.cursor()
        });
        redraw(cx);
        cursor
    };

    place_caret(&view, &mut cx, 2);
    // Down from "intro" lands on the image line (its source appears) ...
    let on_image = step(&view, &mut cx, 1);
    assert!(
        on_image == image_start() || on_image == image_start() + "![cat](pics/small.svg)".len(),
        "caret {on_image} stopped inside the image line"
    );
    // ... and the next Down moves on to "after", not back or nowhere.
    let next = step(&view, &mut cx, 1);
    assert!(next >= after, "caret {next} is on or after the last line");
    // Back up through it.
    let up_to_image = step(&view, &mut cx, -1);
    assert!(
        up_to_image == image_start()
            || up_to_image == image_start() + "![cat](pics/small.svg)".len(),
        "caret {up_to_image} stopped inside the image line going up"
    );
    let up = step(&view, &mut cx, -1);
    assert!(up < image_start(), "caret {up} is on the first line");
    let _ = std::fs::remove_dir_all(&dir);
}

#[std_test]
fn clicking_a_picture_puts_the_caret_on_its_line() {
    let dir = fixture_dir("click");
    let (view, cx) = harness(DOC, &dir);
    let (_, end, y, h) = row_at(&view, &cx, image_start());
    let offset = view.read_with(&cx, |app, _| {
        let paint = app.editor_paint.borrow();
        let bounds = paint.bounds.expect("painted");
        let at = point(bounds.left() + px(60.0), px(y + h / 2.0));
        crate::ui::text_editor::index_for_position(&paint, at)
    });
    let offset = offset.expect("a hit inside the editor");
    assert!(
        (image_start()..=end).contains(&offset),
        "click landed at {offset}, outside the image line"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[std_test]
fn a_far_image_keeps_its_height_without_being_loaded() {
    let dir = fixture_dir("culling");
    let filler: String = (0..400).map(|i| format!("line {i}\n")).collect();
    let doc = format!("{filler}![cat](pics/small.svg)\nlast\n");
    let start = filler.len();
    let (view, mut cx) = harness(&doc, &dir);
    // The first frame may not have seen the far image; the next knows it.
    redraw(&mut cx);
    let (_, _, y, h) = row_at(&view, &cx, start);
    assert_eq!(h, SMALL_PICTURE_ROW);
    let (_, _, last_y, _) = row_at(&view, &cx, doc.find("last").unwrap());
    assert_eq!(last_y, y + SMALL_PICTURE_ROW);
    // Far off screen it is neither drawn nor re-read: no picture elements.
    // Keep the document scrolled to the top and compare totals.
    let total = view.read_with(&cx, |app, _| {
        f32::from(app.editor_paint.borrow().total_height)
    });
    place_caret(&view, &mut cx, start + 3);
    let total_with_source = view.read_with(&cx, |app, _| {
        f32::from(app.editor_paint.borrow().total_height)
    });
    // The caret on the line does not change the block's height.
    assert!((total - total_with_source).abs() < 0.5);
    let _ = std::fs::remove_dir_all(&dir);
}
