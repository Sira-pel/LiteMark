//! The editor shapes only rows near the visible part of the window, but keeps
//! the geometry of every row so clicks, arrow keys and scrolling still work.

use super::*;
use gpui::{Entity, TestAppContext, VisualTestContext};

// `use gpui::*` in this crate re-exports a `test` attribute that is not
// Rust's `#[test]`. Pin the std one so these stay ordinary unit tests.
use ::core::prelude::v1::test as std_test;

const LINES: usize = 3000;

fn long_doc() -> String {
    (0..LINES).map(|i| format!("line {i}\n")).collect()
}

fn harness(content: &str, mode: ViewMode) -> (Entity<LiteMarkApp>, VisualTestContext) {
    let mut cx = TestAppContext::single();
    let (view, cx) = cx.add_window_view(|_window, cx| {
        let mut app = LiteMarkApp::test_new(content, cx);
        app.settings.view_mode = mode;
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

/// Byte range, origin and height of every painted row.
type Geometry = Vec<(usize, usize, Point<Pixels>, Pixels)>;

fn geometry(view: &Entity<LiteMarkApp>, cx: &VisualTestContext) -> Geometry {
    view.read_with(cx, |app, _| {
        app.editor_paint
            .borrow()
            .rows
            .iter()
            .map(|r| (r.byte_start, r.byte_end, r.origin, r.line_height))
            .collect()
    })
}

/// Indices of rows whose text was shaped this frame.
fn shaped_rows(view: &Entity<LiteMarkApp>, cx: &VisualTestContext) -> Vec<usize> {
    view.read_with(cx, |app, _| {
        app.editor_paint
            .borrow()
            .rows
            .iter()
            .enumerate()
            .filter(|(_, r)| !r.shaped.text.is_empty() || !r.cells.is_empty())
            .map(|(i, _)| i)
            .collect()
    })
}

fn row_of(view: &Entity<LiteMarkApp>, cx: &VisualTestContext, offset: usize) -> usize {
    view.read_with(cx, |app, _| {
        app.editor_paint
            .borrow()
            .rows
            .iter()
            .rposition(|r| r.byte_start <= offset)
            .expect("row")
    })
}

fn culls_far_rows_and_keeps_geometry(mode: ViewMode) {
    let content = long_doc();
    let (view, mut cx) = harness(&content, mode);
    let at_top = geometry(&view, &cx);
    assert!(at_top.len() >= LINES, "one row per line: {}", at_top.len());
    let shaped_at_top = shaped_rows(&view, &cx);
    assert!(shaped_at_top.contains(&0));
    assert!(
        shaped_at_top.len() < LINES / 10,
        "only rows near the view are shaped: {}",
        shaped_at_top.len()
    );

    // Moving the caret to the far end shapes rows there, but no row moves.
    let end = content.len() - "line 2999\n".len() + 3;
    place_caret(&view, &mut cx, end);
    assert_eq!(geometry(&view, &cx), at_top);
    let caret_row = row_of(&view, &cx, end);
    let shaped = shaped_rows(&view, &cx);
    for row in caret_row.saturating_sub(2)..=caret_row {
        assert!(shaped.contains(&row), "row {row} near the caret is shaped");
    }
}

#[std_test]
fn wysiwyg_culls_far_rows_and_keeps_geometry() {
    culls_far_rows_and_keeps_geometry(ViewMode::Wysiwyg);
}

#[std_test]
fn raw_culls_far_rows_and_keeps_geometry() {
    culls_far_rows_and_keeps_geometry(ViewMode::Raw);
}

#[std_test]
fn selection_end_far_away_is_shaped() {
    let content = long_doc();
    let (view, mut cx) = harness(&content, ViewMode::Wysiwyg);
    let end = content.len() - 2;
    view.update(&mut cx, |app, _cx| {
        let tab = app.active_tab_mut().expect("tab");
        tab.caret.range = 0..end;
    });
    redraw(&mut cx);
    let shaped = shaped_rows(&view, &cx);
    assert!(shaped.contains(&0));
    assert!(shaped.contains(&row_of(&view, &cx, end)));
}

#[std_test]
fn off_screen_table_keeps_its_height_without_shaping() {
    let table = "| A | B |\n| --- | --- |\n| one | two |\n| three | four |\n";
    let content = format!("{}{table}", long_doc());
    let table_start = content.len() - table.len();
    let (view, mut cx) = harness(&content, ViewMode::Wysiwyg);
    let first = geometry(&view, &cx);

    // The first frame has no cached heights, so it shapes the table once.
    // The next frame reuses them and leaves the far table unshaped.
    redraw(&mut cx);
    let second = geometry(&view, &cx);
    assert_eq!(second, first, "cached heights match the shaped ones");
    let table_row = row_of(&view, &cx, table_start);
    assert!(!shaped_rows(&view, &cx).contains(&table_row));
    let tables = view.read_with(&cx, |app, _| app.editor_paint.borrow().tables.len());
    assert_eq!(tables, 0, "an off-screen table records no chrome");

    // Put the caret in the table: it is shaped as a whole, same geometry.
    place_caret(&view, &mut cx, table_start + 2);
    assert_eq!(geometry(&view, &cx), first);
    let shaped = shaped_rows(&view, &cx);
    // Header and two body rows; the delimiter line has no row.
    for row in table_row..table_row + 3 {
        assert!(shaped.contains(&row), "table row {row} is shaped");
    }
    let tables = view.read_with(&cx, |app, _| app.editor_paint.borrow().tables.len());
    assert_eq!(tables, 1);
}

#[std_test]
fn arrow_down_from_a_far_caret_moves_one_line() {
    let content = long_doc();
    let (view, mut cx) = harness(&content, ViewMode::Wysiwyg);
    let line_2500 = content.find("line 2500\n").expect("line");
    place_caret(&view, &mut cx, line_2500);
    cx.dispatch_action(EditorDown);
    let caret = view.read_with(&cx, |app, _| app.active_tab().expect("tab").caret.cursor());
    let line_2501 = content.find("line 2501\n").expect("line");
    assert_eq!(caret, line_2501);
}
