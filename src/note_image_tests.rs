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

fn write_two_frame_gif(path: &Path) {
    use image::{codecs::gif::GifEncoder, Delay, Frame, Rgba, RgbaImage};
    let file = std::fs::File::create(path).unwrap();
    let mut encoder = GifEncoder::new(file);
    let frame1 = Frame::from_parts(
        RgbaImage::from_pixel(16, 16, Rgba([255, 0, 0, 255])),
        0,
        0,
        Delay::from_numer_denom_ms(10, 1),
    );
    let frame2 = Frame::from_parts(
        RgbaImage::from_pixel(16, 16, Rgba([0, 0, 255, 255])),
        0,
        0,
        Delay::from_numer_denom_ms(10, 1),
    );
    encoder.encode_frames([frame1, frame2]).unwrap();
}

fn write_static_png(path: &Path) {
    use image::{Rgba, RgbaImage};
    let img = RgbaImage::from_pixel(24, 24, Rgba([0, 255, 0, 255]));
    img.save(path).unwrap();
}

#[std_test]
fn swapping_animated_gif_with_static_png_on_same_line_does_not_panic() {
    let dir = fixture_dir("gif-swap");
    let gif = dir.join("pics").join("anim.gif");
    let png = dir.join("pics").join("still.png");
    write_two_frame_gif(&gif);
    write_static_png(&png);

    let (view, mut cx) = harness("![anim](pics/anim.gif)\n", &dir);
    // Render several frames so GPUI decodes and advances the animated GIF.
    for _ in 0..4 {
        redraw(&mut cx);
    }

    // Swap to a static PNG at the exact same line index (0).
    view.update(&mut cx, |app, _cx| {
        let tab = app.active_tab_mut().expect("tab");
        let len = tab.content.len();
        tab.edit(0..len, "![still](pics/still.png)\n");
    });
    redraw(&mut cx);
    redraw(&mut cx);

    // Swap back to the animated GIF, then overwrite that same file path on disk
    // with a static PNG (different size/stamp) and redraw.
    view.update(&mut cx, |app, _cx| {
        let tab = app.active_tab_mut().expect("tab");
        let len = tab.content.len();
        tab.edit(0..len, "![anim](pics/anim.gif)\n");
    });
    for _ in 0..3 {
        redraw(&mut cx);
    }
    write_static_png(&gif);
    redraw(&mut cx);
    redraw(&mut cx);

    let _ = std::fs::remove_dir_all(&dir);
}

#[std_test]
fn clicking_a_tall_picture_does_not_scroll_it_off_screen() {
    let dir = fixture_dir("tall-scroll");
    std::fs::write(
        dir.join("pics").join("tall.svg"),
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="400" height="480"><rect width="400" height="480" fill="blue"/></svg>"#,
    )
    .unwrap();
    let filler: String = (0..40).map(|i| format!("line {i}\n")).collect();
    let doc = format!("intro\n![tall](pics/tall.svg)\n{filler}");
    let pic_start = doc.find("![").unwrap();
    let pic_end = doc.find("\nline 0").unwrap();

    let (view, mut cx) = harness(&doc, &dir);
    redraw(&mut cx);

    // Select the tall picture while it is visible near the top.
    view.update(&mut cx, |app, _cx| {
        app.active_tab_mut()
            .expect("tab")
            .select_picture(pic_start..pic_end);
    });
    redraw(&mut cx);
    redraw(&mut cx);

    let (view_bounds, pic_row_y, pic_row_h) = view.read_with(&cx, |app, _| {
        let view_b = app.editor_scroll.bounds();
        let paint = app.editor_paint.borrow();
        let row = paint
            .rows
            .iter()
            .find(|r| r.byte_start == pic_start)
            .expect("tall picture row");
        (view_b, row.origin.y, row.line_height)
    });
    assert!(
        pic_row_h >= px(400.0),
        "expected a tall picture row, got {pic_row_h:?}"
    );
    assert!(
        pic_row_y + pic_row_h > view_bounds.top() && pic_row_y < view_bounds.bottom(),
        "tall picture scrolled off screen: row_y={pic_row_y:?}, row_h={pic_row_h:?}, view={view_bounds:?}"
    );
    assert!(
        pic_row_y >= view_bounds.top() - px(4.0),
        "tall picture must not be thrown off the top: row_y={pic_row_y:?}, view_top={:?}",
        view_bounds.top()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[std_test]
fn status_bar_clicks_do_not_hit_scrolled_off_pictures_or_text_underneath() {
    let dir = fixture_dir("status-bar-clip");
    std::fs::write(
        dir.join("pics").join("tall.svg"),
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="300" height="400"><rect width="300" height="400" fill="green"/></svg>"#,
    )
    .unwrap();
    let filler: String = (0..30).map(|i| format!("row {i}\n")).collect();
    let doc = format!("{filler}![tall](pics/tall.svg)\n{filler}");
    let (view, mut cx) = harness(&doc, &dir);
    redraw(&mut cx);

    // Put caret at start of document and scroll so the tall picture straddles
    // the bottom of the editor viewport and extends behind the status bar.
    let (pic_start, pic_end, view_bounds) = view.read_with(&cx, |app, _| {
        let s = doc.find("![").unwrap();
        let e = s + "![tall](pics/tall.svg)".len();
        (s, e, app.editor_scroll.bounds())
    });
    let (_, _, pic_y, _) = row_at(&view, &cx, pic_start);
    let scroll_y = -(px(pic_y) - (view_bounds.bottom() - px(40.0)));
    view.update(&mut cx, |app, _cx| {
        app.editor_scroll.set_offset(point(px(0.0), scroll_y));
        app.active_tab_mut().expect("tab").place(0, false);
    });
    redraw(&mut cx);

    // Every recorded picture hit box must be clipped inside the editor viewport.
    view.read_with(&cx, |app, _| {
        let view_b = app.editor_scroll.bounds();
        let paint = app.editor_paint.borrow();
        for (_, b) in &paint.pictures {
            assert!(
                b.bottom() <= view_b.bottom() + px(1.0),
                "picture hit bounds {b:?} leaked past editor viewport bottom {:?}",
                view_b.bottom()
            );
        }
    });

    // Click inside the status bar (below editor_scroll.bounds().bottom()).
    let status_pos = point(
        view_bounds.left() + px(80.0),
        view_bounds.bottom() + px(12.0),
    );
    cx.simulate_mouse_down(status_pos, gpui::MouseButton::Left, gpui::Modifiers::none());
    cx.run_until_parked();
    view.read_with(&cx, |app, _| {
        let sel = app.active_tab().expect("tab").selection_range();
        assert_eq!(
            sel,
            0..0,
            "status bar left-click must not select editor content"
        );
        assert!(app.image_drag.is_none());
    });

    // Scroll the selected picture so its corner resize handle sits under the
    // status bar, then verify pressing directly on that handle is ignored.
    view.update(&mut cx, |app, _cx| {
        app.active_tab_mut()
            .expect("tab")
            .select_picture(pic_start..pic_end);
    });
    redraw(&mut cx);
    redraw(&mut cx);
    let scroll_for_handle = view.read_with(&cx, |app, _| {
        let cur_offset = app.editor_scroll.offset();
        let handle = app
            .editor_paint
            .borrow()
            .picture_handle
            .clone()
            .expect("selected picture handle");
        let target_bottom = app.editor_scroll.bounds().bottom() + px(12.0);
        cur_offset.y + (target_bottom - handle.bounds.bottom())
    });
    view.update(&mut cx, |app, _cx| {
        app.editor_scroll
            .set_offset(point(px(0.0), scroll_for_handle));
    });
    redraw(&mut cx);
    let (before_sel, handle_pos) = view.read_with(&cx, |app, _| {
        let sel = app.active_tab().expect("tab").selection_range();
        let handle = app
            .editor_paint
            .borrow()
            .picture_handle
            .clone()
            .expect("selected picture handle");
        let view_b = app.editor_scroll.bounds();
        assert!(
            handle.clip.bottom() <= view_b.bottom() + px(1.0),
            "handle clip {:?} leaked below editor viewport {:?}",
            handle.clip.bottom(),
            view_b.bottom()
        );
        let pos = point(
            handle.bounds.right() - px(4.0),
            handle.bounds.bottom() - px(4.0),
        );
        assert!(
            crate::ui::note_images::handle_at(handle.bounds, &pos).is_some(),
            "test position {pos:?} must sit on handle {:?}",
            handle.bounds
        );
        assert!(
            pos.y > view_b.bottom(),
            "handle pos {pos:?} must sit under status bar below {:?}",
            view_b.bottom()
        );
        assert!(
            !handle.clip.contains(&pos),
            "clipped handle {:?} must exclude {pos:?}",
            handle.clip
        );
        (sel, pos)
    });
    cx.update(|window, app_cx| {
        view.update(app_cx, |app, cx| {
            app.editor_mouse_down(
                &gpui::MouseDownEvent {
                    button: gpui::MouseButton::Left,
                    position: status_pos,
                    modifiers: gpui::Modifiers::none(),
                    click_count: 1,
                    first_mouse: false,
                },
                window,
                cx,
            );
            app.editor_mouse_down(
                &gpui::MouseDownEvent {
                    button: gpui::MouseButton::Left,
                    position: handle_pos,
                    modifiers: gpui::Modifiers::none(),
                    click_count: 1,
                    first_mouse: false,
                },
                window,
                cx,
            );
            app.editor_right_mouse_down(
                &gpui::MouseDownEvent {
                    button: gpui::MouseButton::Right,
                    position: status_pos,
                    modifiers: gpui::Modifiers::none(),
                    click_count: 1,
                    first_mouse: false,
                },
                window,
                cx,
            );
            // Also verify handle.clip alone rejects a handle press even if the
            // pointer is inside editor_scroll bounds.
            let mut clipped = app.editor_paint.borrow().picture_handle.clone().unwrap();
            clipped.bounds = Bounds::new(
                point(view_bounds.left() + px(20.0), view_bounds.top() + px(20.0)),
                size(px(120.0), px(80.0)),
            );
            clipped.clip = Bounds::new(
                point(view_bounds.left() + px(20.0), view_bounds.top() + px(20.0)),
                size(px(120.0), px(40.0)),
            );
            let inner_clipped_pos = point(
                clipped.bounds.right() - px(4.0),
                clipped.bounds.bottom() - px(4.0),
            );
            app.editor_paint.borrow_mut().picture_handle = Some(clipped);
            assert!(
                !app.handle_picture_handle_press(
                    &gpui::MouseDownEvent {
                        button: gpui::MouseButton::Left,
                        position: inner_clipped_pos,
                        modifiers: gpui::Modifiers::none(),
                        click_count: 1,
                        first_mouse: false,
                    },
                    window,
                    cx,
                ),
                "handle.clip must reject press on clipped handle"
            );
        });
    });
    view.read_with(&cx, |app, _| {
        assert_eq!(app.active_tab().expect("tab").selection_range(), before_sel);
        assert!(app.image_drag.is_none());
        assert!(app.editor_context_menu.is_none());
    });
    let _ = std::fs::remove_dir_all(&dir);
}

#[std_test]
fn mode_and_tab_switches_clear_transient_state_and_invalidate_follow_caret() {
    let dir = fixture_dir("mode-reset");
    let doc = "| a | b |\n| --- | --- |\n| 1 | 2 |\n![cat](pics/small.svg)\n";
    let (view, mut cx) = harness(doc, &dir);
    redraw(&mut cx);

    // Dirty every transient interaction field, then switch via handle_set_view_mode.
    view.update(&mut cx, |app, _cx| {
        app.table_pressed = true;
        app.table_drag_press_held = true;
        app.editor_context_menu = Some(EditorContextMenu {
            position: point(px(10.0), px(10.0)),
            is_table: true,
        });
        app.image_drag = Some(crate::ui::note_images::ImageDrag {
            range: 0..1,
            tab_id: app.active_tab_id.clone(),
            source: String::new(),
            active: true,
            axis: crate::ui::note_images::DragAxis::Corner,
            start_x: px(0.0),
            start_y: px(0.0),
            start_width: px(100.0),
            start_height: px(50.0),
            column: px(400.0),
            width: px(120.0),
            height: px(60.0),
            had_height: false,
        });
        let tab = app.active_tab_mut().expect("tab");
        tab.pending_formats.bold = true;
        tab.marked_range = Some(0..1);
        tab.picture_raw_line = Some(doc.find("![").unwrap());
        app.handle_set_view_mode(ViewMode::Raw);
    });

    view.read_with(&cx, |app, _| {
        assert_eq!(app.settings.view_mode, ViewMode::Raw);
        assert!(app.image_drag.is_none());
        assert!(app.table_hover.is_none());
        assert!(app.table_hot.is_none());
        assert!(!app.table_pressed);
        assert!(!app.table_drag_press_held);
        assert!(app.editor_context_menu.is_none());
        assert_eq!(app.editor_paint.borrow().total_height, px(0.0));
        let tab = app.active_tab().expect("tab");
        assert!(!tab.pending_formats.bold);
        assert!(tab.marked_range.is_none());
        assert!(tab.picture_raw_line.is_none());
        assert_eq!(
            tab.cursor_offset(),
            1,
            "IME composition committed at range.end"
        );
    });

    // Place caret on the table delimiter line in Raw mode, then toggle back to
    // WYSIWYG: caret snaps off the hidden delimiter and painted_view_mode updates.
    let delim = doc.find("| ---").unwrap() + 2;
    view.update(&mut cx, |app, _cx| {
        app.active_tab_mut().expect("tab").place(delim, false);
        app.handle_toggle_mode();
    });
    redraw(&mut cx);
    redraw(&mut cx);
    view.read_with(&cx, |app, _| {
        assert_eq!(app.settings.view_mode, ViewMode::Wysiwyg);
        assert_eq!(
            app.editor_paint.borrow().painted_view_mode,
            ViewMode::Wysiwyg
        );
        let cursor = app.active_tab().expect("tab").cursor_offset();
        let delim_end = doc.find("\n| 1").unwrap();
        assert!(
            cursor < doc.find("| ---").unwrap() || cursor > delim_end,
            "caret {cursor} must snap off hidden TableDelimiter line on Raw->WYSIWYG"
        );
    });

    let _ = std::fs::remove_dir_all(&dir);
}

#[std_test]
fn remote_only_image_note_still_corrects_layout_height() {
    let dir = fixture_dir("remote-height");
    let doc = "![remote1](https://example.com/1.png)\n![remote2](https://example.com/2.png)\n![remote3](https://example.com/3.png)\nbottom\n";
    let (view, mut cx) = harness(doc, &dir);
    redraw(&mut cx);
    redraw(&mut cx);
    view.read_with(&cx, |app, _| {
        let paint = app.editor_paint.borrow();
        let bounds = paint.bounds.expect("painted");
        assert!(
            bounds.size.height >= paint.total_height - px(1.0),
            "remote-only image document bounds {:?} must cover total_height {:?}",
            bounds.size.height,
            paint.total_height
        );
    });
    let _ = std::fs::remove_dir_all(&dir);
}

/// Closing a tab by any path (here the Ctrl+W `handle_close_tab` path, which
/// used to skip the release) frees the closed note's decoded pictures on the
/// next frame, while the note that stays open keeps its own.
#[std_test]
fn closing_a_tab_releases_only_its_cached_pictures() {
    let dir = fixture_dir("close-release");
    let png = |name: &str| {
        let path = dir.join("pics").join(name);
        image::RgbaImage::from_pixel(40, 20, image::Rgba([9, 8, 7, 255]))
            .save(&path)
            .unwrap();
        path
    };
    png("closed.png");
    png("kept.png");
    let (view, mut cx) = harness("![a](pics/closed.png)\n", &dir);
    let closed_id = view.update(&mut cx, |app, _| app.active_tab_id.clone());
    view.update(&mut cx, |app, _| {
        app.handle_new_tab();
        let tab = app.active_tab_mut().expect("tab");
        tab.file_path = Some(dir.join("other.md"));
        tab.edit(0..0, "![b](pics/kept.png)\n");
        tab.is_modified = false;
    });
    redraw(&mut cx);
    redraw(&mut cx);
    let kept_id = view.update(&mut cx, |app, _| {
        let id = app.active_tab_id.clone();
        app.activate_tab(closed_id.clone());
        id
    });
    redraw(&mut cx);
    redraw(&mut cx);
    let count = |view: &Entity<LiteMarkApp>, cx: &mut VisualTestContext| {
        view.update(cx, |app, cx| app.note_image_cache.read(cx).entry_count())
    };
    assert_eq!(count(&view, &mut cx), 2, "both notes' pictures are cached");

    view.update(&mut cx, |app, _| app.handle_close_tab(closed_id.clone()));
    redraw(&mut cx);
    redraw(&mut cx);
    view.update(&mut cx, |app, _| {
        assert_eq!(app.active_tab_id, kept_id);
        assert!(!app.release_closed_tab_images, "render consumed the flag");
    });
    assert_eq!(
        count(&view, &mut cx),
        1,
        "only the open note's picture stays"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// With no active tab the editor paints render()'s fallback Welcome tab, so
/// the content it slices lines from must be that same text.
#[std_test]
fn editor_content_without_a_tab_is_the_fallback_welcome_text() {
    let mut cx = TestAppContext::single();
    let (view, cx) = cx.add_window_view(|_window, cx| LiteMarkApp::test_new("", cx));
    view.update(cx, |app, _| {
        app.tabs.clear();
        use crate::ui::text_editor::EditorContentSource;
        assert_eq!(app.editor_content(), INITIAL_MARKDOWN);
    });
}
