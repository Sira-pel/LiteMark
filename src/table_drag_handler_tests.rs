use super::*;
use crate::markdown::TableColumnAlign;
use crate::ui::table_chrome::column_pill_bounds;
use crate::ui::{TableColGeom, TableRowGeom};
use gpui::{ClipboardItem, Entity, TestAppContext, VisualTestContext};

// `use gpui::*` in this crate re-exports a `test` attribute that is not
// Rust's `#[test]`. Pin the std one so these stay ordinary unit tests.
use ::core::prelude::v1::test as std_test;

const WORD: &str = "hello";
const LINES: &str = "hello\nworld";
const TABLE: &str = "| A | B |\n| --- | --- |\n| d | e |\n";

fn harness(content: &str) -> (Entity<LiteMarkApp>, VisualTestContext) {
    let mut cx = TestAppContext::single();
    let (view, cx) = cx.add_window_view(|_window, cx| LiteMarkApp::test_new(content, cx));
    (view, cx.clone())
}

fn hold_press(view: &Entity<LiteMarkApp>, cx: &mut VisualTestContext) {
    view.update(cx, |app, _cx| {
        app.table_drag_press_held = true;
        app.table_drag = None;
        app.field_focus = TextInput::Editor;
        app.field_owns_keys = false;
    });
}

fn content_and_caret(view: &Entity<LiteMarkApp>, cx: &VisualTestContext) -> (String, usize) {
    view.read_with(cx, |app, _cx| {
        let tab = app.active_tab().expect("tab");
        (tab.content.clone(), tab.caret.cursor())
    })
}

fn set_caret(view: &Entity<LiteMarkApp>, cx: &mut VisualTestContext, offset: usize) {
    view.update(cx, |app, _cx| {
        if let Some(tab) = app.active_tab_mut() {
            tab.caret.move_to(offset);
        }
    });
}

fn select_all_word(view: &Entity<LiteMarkApp>, cx: &mut VisualTestContext) {
    view.update(cx, |app, _cx| {
        if let Some(tab) = app.active_tab_mut() {
            tab.caret.move_to(0);
            tab.caret.select_to(tab.content.len());
        }
    });
}

fn sample_table() -> TableGeom {
    TableGeom {
        source: 0..TABLE.len(),
        bounds: Bounds {
            origin: point(px(100.0), px(80.0)),
            size: Size {
                width: px(180.0),
                height: px(48.0),
            },
        },
        columns: vec![
            TableColGeom {
                header_bounds: Bounds {
                    origin: point(px(100.0), px(80.0)),
                    size: Size {
                        width: px(90.0),
                        height: px(24.0),
                    },
                },
                align: TableColumnAlign::None,
                anchor: 2,
            },
            TableColGeom {
                header_bounds: Bounds {
                    origin: point(px(190.0), px(80.0)),
                    size: Size {
                        width: px(90.0),
                        height: px(24.0),
                    },
                },
                align: TableColumnAlign::Left,
                anchor: 10,
            },
        ],
        rows: vec![
            TableRowGeom {
                index: 0,
                bounds: Bounds {
                    origin: point(px(100.0), px(80.0)),
                    size: Size {
                        width: px(180.0),
                        height: px(24.0),
                    },
                },
                anchors: vec![2, 10],
            },
            TableRowGeom {
                index: 1,
                bounds: Bounds {
                    origin: point(px(100.0), px(104.0)),
                    size: Size {
                        width: px(180.0),
                        height: px(24.0),
                    },
                },
                anchors: vec![40, 50],
            },
        ],
    }
}

fn load_table_paint(view: &Entity<LiteMarkApp>, cx: &mut VisualTestContext) {
    view.update(cx, |app, _cx| {
        app.editor_paint.borrow_mut().tables = vec![sample_table()];
    });
}

fn click_at(
    view: &Entity<LiteMarkApp>,
    cx: &mut VisualTestContext,
    position: Point<Pixels>,
    right: bool,
) {
    let event = MouseDownEvent {
        button: if right {
            MouseButton::Right
        } else {
            MouseButton::Left
        },
        position,
        click_count: 1,
        ..MouseDownEvent::default()
    };
    cx.update(|window, cx| {
        view.update(cx, |app, cx| {
            if right {
                app.editor_right_mouse_down(&event, window, cx);
            } else {
                app.editor_mouse_down(&event, window, cx);
            }
        });
    });
}

#[std_test]
fn handle_paste_does_not_edit_while_drag_press_is_held() {
    let (view, mut cx) = harness(WORD);
    set_caret(&view, &mut cx, WORD.len());
    hold_press(&view, &mut cx);
    cx.write_to_clipboard(ClipboardItem::new_string("Z".into()));
    view.update(&mut cx, |app, cx| {
        app.handle_paste(cx);
    });
    let (after, _) = content_and_caret(&view, &cx);
    assert_eq!(after, WORD);
}

#[std_test]
fn handle_paste_edits_when_press_is_not_held() {
    let (view, mut cx) = harness(WORD);
    set_caret(&view, &mut cx, WORD.len());
    cx.write_to_clipboard(ClipboardItem::new_string("Z".into()));
    view.update(&mut cx, |app, cx| {
        app.handle_paste(cx);
    });
    let (after, _) = content_and_caret(&view, &cx);
    assert_ne!(after, WORD);
    assert!(after.contains('Z'));
}

#[std_test]
fn replace_text_in_range_does_not_edit_while_drag_press_is_held() {
    let (view, mut cx) = harness(WORD);
    set_caret(&view, &mut cx, WORD.len());
    hold_press(&view, &mut cx);
    cx.update(|window, cx| {
        view.update(cx, |app, cx| {
            app.replace_text_in_range(None, "x", window, cx);
        });
    });
    let (after, _) = content_and_caret(&view, &cx);
    assert_eq!(after, WORD);
}

#[std_test]
fn replace_text_in_range_edits_when_press_is_not_held() {
    let (view, mut cx) = harness(WORD);
    set_caret(&view, &mut cx, WORD.len());
    cx.update(|window, cx| {
        view.update(cx, |app, cx| {
            app.replace_text_in_range(None, "x", window, cx);
        });
    });
    let (after, _) = content_and_caret(&view, &cx);
    assert_ne!(after, WORD);
    assert!(after.contains('x'));
}

#[std_test]
fn editor_move_does_not_move_while_drag_press_is_held() {
    let (view, mut cx) = harness(LINES);
    set_caret(&view, &mut cx, 0);
    hold_press(&view, &mut cx);
    view.update(&mut cx, |app, cx| {
        app.editor_move(cx, |tab| tab.move_line_down());
    });
    let (after, caret) = content_and_caret(&view, &cx);
    assert_eq!(after, LINES);
    assert_eq!(caret, 0);
}

#[std_test]
fn editor_move_moves_when_press_is_not_held() {
    let (view, mut cx) = harness(LINES);
    set_caret(&view, &mut cx, 0);
    view.update(&mut cx, |app, cx| {
        app.editor_move(cx, |tab| tab.move_line_down());
    });
    let (after, _) = content_and_caret(&view, &cx);
    assert_ne!(after, LINES);
}

#[std_test]
fn editor_backspace_does_not_edit_while_drag_press_is_held() {
    let (view, mut cx) = harness(WORD);
    set_caret(&view, &mut cx, WORD.len());
    hold_press(&view, &mut cx);
    view.update(&mut cx, |app, cx| {
        app.editor_backspace(cx);
    });
    let (after, _) = content_and_caret(&view, &cx);
    assert_eq!(after, WORD);
}

#[std_test]
fn editor_backspace_edits_when_press_is_not_held() {
    let (view, mut cx) = harness(WORD);
    set_caret(&view, &mut cx, WORD.len());
    view.update(&mut cx, |app, cx| {
        app.editor_backspace(cx);
    });
    let (after, _) = content_and_caret(&view, &cx);
    assert_eq!(after, "hell");
}

#[std_test]
fn swallow_blocks_format_and_bound_keys_while_drag_press_is_held() {
    let (view, mut cx) = harness(WORD);
    select_all_word(&view, &mut cx);
    hold_press(&view, &mut cx);
    view.update(&mut cx, |app, cx| {
        app.handle_format_cmd("bold", cx);
        app.editor_indent(cx);
        app.editor_newline(cx);
        app.editor_delete(cx);
    });
    let (after, _) = content_and_caret(&view, &cx);
    assert_eq!(after, WORD);
}

#[std_test]
fn format_and_tab_edit_when_press_is_not_held() {
    let (view, mut cx) = harness(WORD);
    select_all_word(&view, &mut cx);
    view.update(&mut cx, |app, cx| {
        app.handle_format_cmd("bold", cx);
    });
    let (after, _) = content_and_caret(&view, &cx);
    assert_ne!(after, WORD);

    let (view, mut cx) = harness(WORD);
    set_caret(&view, &mut cx, 0);
    view.update(&mut cx, |app, cx| {
        app.editor_indent(cx);
    });
    let (after, _) = content_and_caret(&view, &cx);
    assert_ne!(after, WORD);
}

#[std_test]
fn editor_mouse_up_clears_press_held() {
    let (view, mut cx) = harness(WORD);
    hold_press(&view, &mut cx);
    cx.update(|window, cx| {
        view.update(cx, |app, cx| {
            app.editor_mouse_up(&MouseUpEvent::default(), window, cx);
        });
    });
    view.read_with(&cx, |app, _| {
        assert!(!app.table_drag_press_held);
    });
}

#[std_test]
fn editor_mouse_down_clears_press_held() {
    let (view, mut cx) = harness(WORD);
    hold_press(&view, &mut cx);
    click_at(&view, &mut cx, point(px(0.0), px(0.0)), false);
    view.read_with(&cx, |app, _| {
        assert!(!app.table_drag_press_held);
    });
}

#[std_test]
fn handle_mouse_down_out_clears_press_held() {
    let (view, mut cx) = harness(WORD);
    hold_press(&view, &mut cx);
    view.update(&mut cx, |app, _cx| {
        app.table_drag = Some(TableDrag {
            source: 0..1,
            kind: TableDragKind::Row,
            from: 1,
            anchor: 0,
            gap: 1,
            drop_visible: true,
        });
        app.handle_mouse_down_out();
    });
    view.read_with(&cx, |app, _| {
        assert!(!app.table_drag_press_held);
        assert!(app.table_drag.is_none());
    });
}

#[std_test]
fn handle_window_deactivated_clears_press_held() {
    let (view, mut cx) = harness(WORD);
    hold_press(&view, &mut cx);
    view.update(&mut cx, |app, _cx| {
        app.handle_window_deactivated();
    });
    view.read_with(&cx, |app, _| {
        assert!(!app.table_drag_press_held);
    });
}

#[std_test]
fn handle_close_modal_clears_press_held() {
    let (view, mut cx) = harness(WORD);
    hold_press(&view, &mut cx);
    cx.update(|window, cx| {
        view.update(cx, |app, cx| {
            app.handle_close_modal(window, cx);
        });
    });
    view.read_with(&cx, |app, _| {
        assert!(!app.table_drag_press_held);
    });
}

#[std_test]
fn editor_right_mouse_down_clears_press_held() {
    let (view, mut cx) = harness(WORD);
    hold_press(&view, &mut cx);
    click_at(&view, &mut cx, point(px(10.0), px(10.0)), true);
    view.read_with(&cx, |app, _| {
        assert!(!app.table_drag_press_held);
    });
}

#[std_test]
fn chrome_align_clears_press_held() {
    let (view, mut cx) = harness(TABLE);
    load_table_paint(&view, &mut cx);
    hold_press(&view, &mut cx);
    let pill = column_pill_bounds(sample_table().columns[0].header_bounds);
    let width = pill.size.width / 3.0;
    let at = point(pill.left() + width * 1.5, pill.top() + pill.size.height / 2.0);
    click_at(&view, &mut cx, at, false);
    view.read_with(&cx, |app, _| {
        assert!(!app.table_drag_press_held);
    });
}

#[std_test]
fn chrome_plus_clears_press_held() {
    let (view, mut cx) = harness(TABLE);
    load_table_paint(&view, &mut cx);
    hold_press(&view, &mut cx);
    let pill = column_pill_bounds(sample_table().columns[0].header_bounds);
    let width = pill.size.width / 3.0;
    let at = point(pill.left() + width * 2.5, pill.top() + pill.size.height / 2.0);
    click_at(&view, &mut cx, at, false);
    view.read_with(&cx, |app, _| {
        assert!(!app.table_drag_press_held);
    });
}

#[std_test]
fn root_wires_each_press_end_path() {
    let src = include_str!("app.rs");
    for needle in [
        ".on_mouse_up_out(",
        ".on_mouse_down_out(",
        "observe_window_activation",
        "this.handle_window_deactivated()",
        "this.handle_mouse_down_out()",
        "this.handle_close_modal(window, cx)",
        "this.editor_mouse_up(event, window, cx)",
        "self.clear_table_drag_press()",
    ] {
        assert!(src.contains(needle), "app.rs lost press-end path {needle}");
    }
    let chrome = src.matches("self.clear_table_drag_press()").count();
    assert!(
        chrome >= 10,
        "expected each clear path to call clear_table_drag_press, found {chrome}"
    );
}
