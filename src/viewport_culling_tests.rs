//! The editor shapes only rows near the visible part of the window, but keeps
//! the geometry of every row so clicks, arrow keys and scrolling still work.

use super::*;
use gpui::{Entity, TestAppContext, VisualTestContext};

// `use gpui::*` in this crate re-exports a `test` attribute that is not
// Rust's `#[test]`. Pin the std one so these stay ordinary unit tests.
use core::prelude::v1::test as std_test;

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
            .map(|r| (r.byte_start(), r.byte_end(), r.origin, r.line_height))
            .collect()
    })
}

fn assert_same_relative_geometry(edited: &Geometry, fresh: &Geometry) {
    let relative = |rows: &Geometry| -> Vec<(usize, usize, Pixels, Pixels, Pixels)> {
        let mut prev = rows.first().map_or(px(0.0), |r| r.2.y);
        rows.iter()
            .map(|&(start, end, origin, h)| {
                let step = origin.y - prev;
                prev = origin.y;
                (start, end, origin.x, step, h)
            })
            .collect()
    };
    let fresh = relative(fresh);
    let edited = relative(edited);
    assert_eq!(edited.len(), fresh.len(), "row count");
    let same = |a: &(usize, usize, Pixels, Pixels, Pixels),
                b: &(usize, usize, Pixels, Pixels, Pixels)| {
        (a.0, a.1, a.2, a.4) == (b.0, b.1, b.2, b.4)
            && (f32::from(a.3) - f32::from(b.3)).abs() < 0.01
    };
    if let Some(ix) = edited.iter().zip(&fresh).position(|(a, b)| !same(a, b)) {
        panic!(
            "row {ix} differs: edited {:?} fresh {:?}",
            edited[ix], fresh[ix]
        );
    }
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
            .rposition(|r| r.byte_start() <= offset)
            .expect("row")
    })
}

fn culls_far_rows_and_keeps_geometry(mode: ViewMode) {
    let content = long_doc();
    let (view, mut cx) = harness(&content, mode);
    let at_top = geometry(&view, &cx);
    assert!(
        at_top.len() < LINES / 10,
        "only rows near the view are materialized: {}",
        at_top.len()
    );
    let total_at_top = view.read_with(&cx, |app, _| app.editor_paint.borrow().total_height);
    let shaped_at_top = shaped_rows(&view, &cx);
    assert!(shaped_at_top.contains(&0));
    assert!(
        shaped_at_top.len() < LINES / 10,
        "only rows near the view are shaped: {}",
        shaped_at_top.len()
    );

    // Moving the caret to the far end shapes rows there, while total document
    // height and line count stay identical.
    let end = content.len() - "line 2999\n".len() + 3;
    place_caret(&view, &mut cx, end);
    let total_at_end = view.read_with(&cx, |app, _| app.editor_paint.borrow().total_height);
    assert!((f32::from(total_at_end) - f32::from(total_at_top)).abs() < 0.01);
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
    let first_total = view.read_with(&cx, |app, _| app.editor_paint.borrow().total_height);

    // The off-screen table is not materialized or shaped on idle frames.
    redraw(&mut cx);
    let second_total = view.read_with(&cx, |app, _| app.editor_paint.borrow().total_height);
    assert_eq!(second_total, first_total, "cached heights match");
    let has_table_row = view.read_with(&cx, |app, _| {
        app.editor_paint
            .borrow()
            .rows
            .iter()
            .any(|r| r.byte_start() >= table_start)
    });
    assert!(!has_table_row);
    let tables = view.read_with(&cx, |app, _| app.editor_paint.borrow().tables.len());
    assert_eq!(tables, 0, "an off-screen table records no chrome");

    // Put the caret in the table: rows next to it are shaped, same total height.
    place_caret(&view, &mut cx, table_start + 2);
    let third_total = view.read_with(&cx, |app, _| app.editor_paint.borrow().total_height);
    assert!((f32::from(third_total) - f32::from(first_total)).abs() < 20.0);
    let table_row = row_of(&view, &cx, table_start);
    let shaped = shaped_rows(&view, &cx);
    // Header and the first body row (the delimiter line has no row).
    for row in table_row..table_row + 2 {
        assert!(shaped.contains(&row), "table row {row} is shaped");
    }
    // Every row is still recorded for the table chrome, shaped or not.
    let rows = view.read_with(&cx, |app, _| {
        let paint = app.editor_paint.borrow();
        assert_eq!(paint.tables.len(), 1);
        paint.tables[0].rows.len()
    });
    assert_eq!(rows, 3);
}

fn big_table(rows: usize) -> String {
    let mut table = String::from("| Name | Value | Note |\n| --- | --- | --- |\n");
    for i in 0..rows {
        table.push_str(&format!(
            "| row {i} | {} | some **bold** text {i} |\n",
            i * 7
        ));
    }
    table
}

#[std_test]
fn caret_in_a_large_table_shapes_only_nearby_rows() {
    // F13: a caret anywhere in a table shaped every row of it on every
    // frame, and each keystroke re-measured every cell.
    let rows = 2000;
    let content = big_table(rows);
    let (view, mut cx) = harness(&content, ViewMode::Wysiwyg);
    let first = geometry(&view, &cx);
    redraw(&mut cx);

    // Caret in a cell far down the table.
    let target = content.find("| row 1500 |").unwrap() + 4;
    place_caret(&view, &mut cx, target);
    assert_same_relative_geometry(&geometry(&view, &cx), &first);
    let shaped = shaped_rows(&view, &cx);
    let caret_row = row_of(&view, &cx, target);
    assert!(shaped.contains(&caret_row), "the caret's row is shaped");
    assert!(
        shaped.len() < rows / 10,
        "{} of {rows} rows shaped",
        shaped.len()
    );

    // Typing in the cell changes the table's key; the other rows keep
    // their cached heights and stay unshaped.
    view.update(&mut cx, |app, _cx| {
        let tab = app.active_tab_mut().expect("tab");
        let at = tab.caret.range.clone();
        tab.insert_text_in_mode(at, "x", ViewMode::Wysiwyg);
    });
    redraw(&mut cx);
    let shaped = shaped_rows(&view, &cx);
    assert!(
        shaped.len() < rows / 10,
        "{} of {rows} rows shaped after typing",
        shaped.len()
    );
    let after = geometry(&view, &cx);
    assert_eq!(after.len(), first.len());
    let chrome_rows = view.read_with(&cx, |app, _| app.editor_paint.borrow().tables[0].rows.len());
    assert_eq!(chrome_rows, rows + 1, "header and every body row recorded");
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

#[std_test]
fn up_and_down_keep_the_column_across_a_short_line() {
    for mode in [ViewMode::Raw, ViewMode::Wysiwyg] {
        let content = "abcdefgh\nab\nabcdefgh\n";
        let (view, mut cx) = harness(content, mode);
        let caret = |view: &Entity<LiteMarkApp>, cx: &VisualTestContext| {
            view.read_with(cx, |app, _| app.active_tab().expect("tab").caret.cursor())
        };
        place_caret(&view, &mut cx, 6);
        cx.dispatch_action(EditorDown);
        assert_eq!(caret(&view, &cx), 11, "short line clamps to its end");
        cx.dispatch_action(EditorDown);
        assert_eq!(
            caret(&view, &cx),
            12 + 6,
            "{mode:?}: column is kept going down"
        );
        cx.dispatch_action(EditorUp);
        cx.dispatch_action(EditorUp);
        assert_eq!(caret(&view, &cx), 6, "{mode:?}: and going back up");

        // A horizontal move forgets the remembered column.
        cx.dispatch_action(EditorDown);
        cx.dispatch_action(EditorLeft);
        cx.dispatch_action(EditorDown);
        assert_eq!(caret(&view, &cx), 12 + 1, "{mode:?}: left resets it");
    }
}

fn caret_row_in_view(
    view: &Entity<LiteMarkApp>,
    cx: &VisualTestContext,
) -> (Pixels, Pixels, Pixels) {
    view.read_with(cx, |app, _| {
        let cursor = app.active_tab().expect("tab").caret.cursor();
        let paint = app.editor_paint.borrow();
        let row = paint
            .rows
            .iter()
            .rev()
            .find(|r| r.byte_start() <= cursor)
            .expect("row");
        let b = app.editor_scroll.bounds();
        (row.origin.y + row.line_height, b.bottom(), b.top())
    })
}

#[std_test]
fn enter_at_the_bottom_scrolls_the_new_line_into_view() {
    for mode in [ViewMode::Raw, ViewMode::Wysiwyg] {
        let content: String = (0..200).map(|i| format!("line {i}\n")).collect();
        let (view, mut cx) = harness(&content, mode);
        place_caret(&view, &mut cx, content.len());
        for i in 0..60 {
            cx.dispatch_action(EditorNewline);
            cx.update(|window, _| window.refresh());
            cx.run_until_parked();
            let (row_bottom, view_bottom, _) = caret_row_in_view(&view, &cx);
            assert!(
                row_bottom <= view_bottom,
                "{mode:?}: newline {i} fell below the editor"
            );
        }
        // Moving up past the top edge follows too.
        for _ in 0..90 {
            cx.dispatch_action(EditorUp);
            cx.update(|window, _| window.refresh());
            cx.run_until_parked();
        }
        let (_, _, view_top) = caret_row_in_view(&view, &cx);
        let (row_bottom, view_bottom, _) = caret_row_in_view(&view, &cx);
        assert!(
            row_bottom <= view_bottom && row_bottom >= view_top,
            "{mode:?}: up arrow lost the caret"
        );
    }
}

// Momentum is Linux-only: macOS scrolls with its own inertia, Windows has line steps.
#[cfg(target_os = "linux")]
#[std_test]
fn touchpad_flick_keeps_scrolling_after_the_events_stop() {
    let content = long_doc();
    let (view, mut cx) = harness(&content, ViewMode::Raw);
    let y_of = |view: &Entity<LiteMarkApp>, cx: &VisualTestContext| {
        view.read_with(cx, |app, _| f32::from(app.editor_scroll.offset().y))
    };
    let pos = view.read_with(&cx, |app, _| app.editor_scroll.bounds().center());
    // A fake clock moved by fixed steps, so a slow runner can't stretch the
    // gaps between events into a "fingers rested" pause.
    let advance = |view: &Entity<LiteMarkApp>, cx: &mut VisualTestContext, ms: u64| {
        view.update(cx, |app, _| {
            let now = app.scroll_clock.unwrap_or_else(std::time::Instant::now);
            app.scroll_clock = Some(now + std::time::Duration::from_millis(ms));
        });
    };
    advance(&view, &mut cx, 0);
    for _ in 0..12 {
        cx.simulate_event(ScrollWheelEvent {
            position: pos,
            delta: ScrollDelta::Pixels(point(px(0.0), px(-14.0))),
            modifiers: Modifiers::default(),
            touch_phase: TouchPhase::Moved,
        });
        advance(&view, &mut cx, 8);
    }
    let at_release = y_of(&view, &cx);
    assert!(
        at_release < -100.0,
        "the flick itself scrolled: {at_release}"
    );
    let mut last = at_release;
    for _ in 0..12 {
        advance(&view, &mut cx, 16);
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
        last = y_of(&view, &cx);
    }
    assert!(
        last < at_release - 50.0,
        "coasted on: {at_release} -> {last}"
    );

    // A fresh touch stops it dead.
    cx.simulate_event(ScrollWheelEvent {
        position: pos,
        delta: ScrollDelta::Pixels(point(px(0.0), px(2.0))),
        modifiers: Modifiers::default(),
        touch_phase: TouchPhase::Moved,
    });
    let stopped = y_of(&view, &cx);
    advance(&view, &mut cx, 60);
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert_eq!(y_of(&view, &cx), stopped);
}

#[std_test]
fn typing_above_tables_keeps_far_tables_unshaped() {
    let content: String = (0..200)
        .map(|i| format!("| a{i} | b |\n| - | - |\n| 1 | 2 |\n\n"))
        .collect();
    let (view, mut cx) = harness(&content, ViewMode::Wysiwyg);
    let mid_byte = content.len() / 2;
    let far_table_rows = |view: &Entity<LiteMarkApp>, cx: &VisualTestContext| {
        view.read_with(cx, |app, _| {
            app.editor_paint
                .borrow()
                .rows
                .iter()
                .filter(|r| r.byte_start() > mid_byte && !r.cells.is_empty())
                .count()
        })
    };
    assert_eq!(far_table_rows(&view, &cx), 0, "far tables start unshaped");

    // Typing at the top moves every table's byte range but not its text.
    view.update(&mut cx, |app, _cx| {
        let tab = app.active_tab_mut().expect("tab");
        tab.insert_text_in_mode(0..0, "x", ViewMode::Wysiwyg);
    });
    redraw(&mut cx);
    assert_eq!(
        far_table_rows(&view, &cx),
        0,
        "an edit above a table must not shape it again"
    );
}

#[std_test]
fn layout_after_edits_matches_a_fresh_layout() {
    // F15: the layout cache follows the parse's incremental updates instead
    // of starting over; the rows must be the same as laying out from scratch.
    let mut content = String::new();
    for i in 0..400 {
        content.push_str(&format!(
            "## Part {i}\n\nA long paragraph {i} that keeps going so that it wraps over several rows in the editor window, with **bold** and `code` in it.\n\n- item {i}\n\n"
        ));
    }
    let (view, mut cx) = harness(&content, ViewMode::Wysiwyg);
    let edits: [(usize, &str); 5] = [
        (
            content.len() / 2,
            "inserted words that make this paragraph wrap one more row ",
        ),
        (100, "\n\n# New heading\n\n"),
        (content.len() / 3, "```\n"),
        (content.len() / 3 + 30, "\n```\n"),
        (content.len() - 10, "x"),
    ];
    for (at, text) in edits {
        view.update(&mut cx, |app, _cx| {
            let tab = app.active_tab_mut().expect("tab");
            let mut at = at.min(tab.content.len());
            while !tab.content.is_char_boundary(at) {
                at -= 1;
            }
            tab.place(at, false);
            tab.insert_text_in_mode(at..at, text, ViewMode::Wysiwyg);
        });
        redraw(&mut cx);
    }
    let final_content = view.read_with(&cx, |app, _| app.active_tab().unwrap().content.clone());
    let caret = view.read_with(&cx, |app, _| app.active_tab().unwrap().caret.cursor());
    let edited = geometry(&view, &cx);

    let (fresh_view, mut fresh_cx) = harness(&final_content, ViewMode::Wysiwyg);
    fresh_view.update(&mut fresh_cx, |app, _cx| {
        app.active_tab_mut().unwrap().place(caret, false);
    });
    redraw(&mut fresh_cx);
    assert_same_relative_geometry(&edited, &geometry(&fresh_view, &fresh_cx));
}

#[std_test]
fn table_cell_wrap_cache_reuses_shaped_lines_and_updates_offset_maps() {
    let table = "| Header A | Header B |\n| --- | --- |\n| alpha beta gamma delta epsilon zeta eta theta iota kappa | short |\n";
    let (view, mut cx) = harness(table, ViewMode::Wysiwyg);
    let cell_shaped_ptrs =
        |view: &Entity<LiteMarkApp>, cx: &VisualTestContext, line_idx: u32| -> Vec<usize> {
            view.read_with(cx, |app, _| {
                let paint = app.editor_paint.borrow();
                let row = paint
                    .rows
                    .iter()
                    .find(|r| !r.cells.is_empty() && r.line_index == line_idx)
                    .expect("body table row");
                row.cells[0]
                    .lines
                    .iter()
                    .map(|l| std::rc::Rc::as_ptr(&l.shaped) as usize)
                    .collect()
            })
        };
    let initial_ptrs = cell_shaped_ptrs(&view, &cx, 2);
    assert!(!initial_ptrs.is_empty());

    // Idle redraw reuses the exact same Rc<ShapedLine>s.
    redraw(&mut cx);
    assert_eq!(cell_shaped_ptrs(&view, &cx, 2), initial_ptrs);

    // Inserting a paragraph above the table shifts byte offsets, yet the cached
    // Rc<ShapedLine>s are reused while CellLine::offset_map updates to the new offsets.
    let prefix = "Inserted paragraph above table.\n\n";
    view.update(&mut cx, |app, _cx| {
        app.active_tab_mut()
            .unwrap()
            .insert_text_in_mode(0..0, prefix, ViewMode::Wysiwyg);
    });
    redraw(&mut cx);
    assert_eq!(cell_shaped_ptrs(&view, &cx, 4), initial_ptrs);

    let (alpha_start, alpha_end) = view.read_with(&cx, |app, _| {
        let paint = app.editor_paint.borrow();
        let row = paint
            .rows
            .iter()
            .find(|r| !r.cells.is_empty() && r.line_index == 4)
            .expect("shifted body row");
        let first_line = &row.cells[0].lines[0];
        let last_line = row.cells[0].lines.last().unwrap();
        (
            first_line.offset_map.line_start,
            last_line.offset_map.line_end,
        )
    });
    let expected_start = view.read_with(&cx, |app, _| {
        app.active_tab().unwrap().content.find("alpha").unwrap()
    });
    assert_eq!(alpha_start, expected_start);
    assert!(alpha_end > alpha_start);
}

#[std_test]
fn table_cell_wrap_cache_invalidates_on_theme_zoom_width_and_bypasses_marked() {
    let table = "| Header A | Header B |\n| --- | --- |\n| alpha beta gamma delta epsilon zeta eta theta iota kappa | short |\n";
    let (view, mut cx) = harness(table, ViewMode::Wysiwyg);
    // Every shaped line seen stays alive to the end of the test, so a freed
    // line's address can't be reused by its replacement (macOS's allocator
    // hands it straight back, which made the address checks flaky there).
    let kept = std::cell::RefCell::new(Vec::new());
    let body_ptrs = |view: &Entity<LiteMarkApp>, cx: &VisualTestContext| -> Vec<usize> {
        view.read_with(cx, |app, _| {
            let paint = app.editor_paint.borrow();
            let row = paint
                .rows
                .iter()
                .find(|r| !r.cells.is_empty() && r.line_index == 2)
                .expect("body row");
            row.cells[0]
                .lines
                .iter()
                .map(|l| {
                    kept.borrow_mut().push(l.shaped.clone());
                    std::rc::Rc::as_ptr(&l.shaped) as usize
                })
                .collect()
        })
    };
    let p0 = body_ptrs(&view, &cx);

    // 1. Zoom change invalidates wrapped_cells.
    view.update(&mut cx, |app, _cx| {
        app.settings.zoom_level = 125;
    });
    redraw(&mut cx);
    let p1 = body_ptrs(&view, &cx);
    assert_ne!(p1, p0, "zoom change must re-shape cached table cells");

    // 2. Theme change invalidates wrapped_cells.
    view.update(&mut cx, |app, _cx| {
        app.settings.theme = if app.settings.theme == crate::theme::ThemeMode::YaruDark {
            crate::theme::ThemeMode::YaruLight
        } else {
            crate::theme::ThemeMode::YaruDark
        };
    });
    redraw(&mut cx);
    let p2 = body_ptrs(&view, &cx);
    assert_ne!(p2, p1, "theme change must re-shape cached table cells");

    // 3. Column width change (widening column B) invalidates wrapped_cells for column A.
    view.update(&mut cx, |app, _cx| {
        let tab = app.active_tab_mut().unwrap();
        let pos = tab.content.find("short").unwrap();
        tab.insert_text_in_mode(pos..pos, "much longer column B text ", ViewMode::Wysiwyg);
    });
    redraw(&mut cx);
    let p3 = body_ptrs(&view, &cx);
    assert_ne!(
        p3, p2,
        "column width change must re-shape cached table cells"
    );

    // 4. Active IME marked_range inside column A bypasses wrapped_cells without overwriting it.
    view.update(&mut cx, |app, _cx| {
        let tab = app.active_tab_mut().unwrap();
        let pos = tab.content.find("alpha").unwrap();
        tab.marked_range = Some(pos..pos + 5);
    });
    redraw(&mut cx);
    let p_marked = body_ptrs(&view, &cx);
    assert_ne!(
        p_marked, p3,
        "IME marked range must bypass wrapped_cells cache"
    );
    view.update(&mut cx, |app, _cx| {
        app.active_tab_mut().unwrap().marked_range = None;
    });
    redraw(&mut cx);
    assert_eq!(
        body_ptrs(&view, &cx),
        p3,
        "IME frame must not overwrite non-IME cached wrapped_cells"
    );

    // 5. Switching to Raw mode clears wrapped_cells.
    view.update(&mut cx, |app, _cx| {
        app.settings.view_mode = ViewMode::Raw;
    });
    redraw(&mut cx);
    view.read_with(&cx, |app, _| {
        assert!(app
            .editor_paint
            .borrow()
            .table_width_cache
            .wrapped_cells
            .is_empty());
    });
}

#[std_test]
fn shaped_line_cache_reuses_lines_on_idle_and_single_line_edit() {
    for mode in [ViewMode::Wysiwyg, ViewMode::Raw] {
        let doc = "First **bold** line\nSecond plain line\nThird *italic* line\n";
        let (view, mut cx) = harness(doc, mode);
        let ptrs = |view: &Entity<LiteMarkApp>, cx: &VisualTestContext| -> Vec<usize> {
            view.read_with(cx, |app, _| {
                app.editor_paint
                    .borrow()
                    .rows
                    .iter()
                    .map(|r| std::rc::Rc::as_ptr(&r.shaped) as usize)
                    .collect()
            })
        };
        let p0 = ptrs(&view, &cx);
        assert!(p0.len() >= 3);

        // Idle redraw reuses every Rc<ShapedLine>.
        redraw(&mut cx);
        assert_eq!(ptrs(&view, &cx), p0, "{mode:?}: idle redraw reuses lines");

        // Editing line 1 re-shapes line 1 only; lines 0 and 2 reuse their Rc.
        view.update(&mut cx, |app, _cx| {
            let tab = app.active_tab_mut().unwrap();
            let pos = tab.content.find("Second").unwrap();
            tab.insert_text_in_mode(pos..pos, "Edited ", mode);
        });
        redraw(&mut cx);
        let p1 = ptrs(&view, &cx);
        assert_eq!(p1[0], p0[0], "{mode:?}: line 0 reused");
        assert_ne!(p1[1], p0[1], "{mode:?}: edited line 1 re-shaped");
        assert_eq!(p1[2], p0[2], "{mode:?}: line 2 reused");
    }
}

#[std_test]
fn shaped_line_cache_invalidates_on_theme_zoom_mode_and_bypasses_marked() {
    let doc = "Alpha **beta** gamma\n";
    let (view, mut cx) = harness(doc, ViewMode::Wysiwyg);
    let kept = std::cell::RefCell::new(Vec::new());
    let ptr0 = |view: &Entity<LiteMarkApp>, cx: &VisualTestContext| -> usize {
        view.read_with(cx, |app, _| {
            let s = app.editor_paint.borrow().rows[0].shaped.clone();
            let ptr = std::rc::Rc::as_ptr(&s) as usize;
            kept.borrow_mut().push(s);
            ptr
        })
    };
    let p0 = ptr0(&view, &cx);

    // 1. Zoom change invalidates cache.
    view.update(&mut cx, |app, _| app.settings.zoom_level = 125);
    redraw(&mut cx);
    let p1 = ptr0(&view, &cx);
    assert_ne!(p1, p0, "zoom change must re-shape");

    // 2. Theme change invalidates cache.
    view.update(&mut cx, |app, _| {
        app.settings.theme = if app.settings.theme == crate::theme::ThemeMode::YaruDark {
            crate::theme::ThemeMode::YaruLight
        } else {
            crate::theme::ThemeMode::YaruDark
        };
    });
    redraw(&mut cx);
    let p2 = ptr0(&view, &cx);
    assert_ne!(p2, p1, "theme change must re-shape");

    // 3. IME marked range bypasses cache without overwriting it.
    view.update(&mut cx, |app, _| {
        app.active_tab_mut().unwrap().marked_range = Some(0..5);
    });
    redraw(&mut cx);
    let p_marked = ptr0(&view, &cx);
    assert_ne!(p_marked, p2, "marked range must bypass cache");

    view.update(&mut cx, |app, _| {
        app.active_tab_mut().unwrap().marked_range = None;
    });
    redraw(&mut cx);
    let p_after_marked = ptr0(&view, &cx);
    assert_ne!(
        p_after_marked, p_marked,
        "IME frame must not poison non-IME cache"
    );

    // 4. Mode switch invalidates cache.
    view.update(&mut cx, |app, _| app.settings.view_mode = ViewMode::Raw);
    redraw(&mut cx);
    assert_ne!(
        ptr0(&view, &cx),
        p_after_marked,
        "mode switch must re-shape"
    );
}

#[std_test]
fn scrolling_past_multiple_offscreen_tables_keeps_column_widths_and_reaches_bottom() {
    // Regression test for v0.15 scroll glitch: tables outside the initial
    // viewport were missing from `layout_cache.table_keys`, colliding on
    // `table_key = 0` and stealing each other's column counts/widths as tables
    // entered and left the shaping band.
    let mut content = String::from(
        "# Top\n\n\
         | ID | Task | Owner | Status |\n\
         | --- | --- | --- | --- |\n\
         | P0-1 | Initial setup | Alice | Done |\n\n",
    );
    // Enough prose to push the remaining tables well past the initial shaping band.
    for i in 0..80 {
        content.push_str(&format!("Spacer paragraph line {i} with ordinary text.\n"));
    }
    content.push_str(
        "\n## 4. Key numbers\n\n\
         | Metric | Now | Target (Demo Day) |\n\
         | --- | --- | --- |\n\
         | Facilities in app | 7 (sample) | All ODC rows fill in after DATA-01 |\n\
         | Automated tests | 14 | 100% pass |\n\n\
         ## 5. Risks\n\n\
         | Risk | Impact | Likelihood | Mitigation |\n\
         | --- | --- | --- | --- |\n\
         | Event rules ban pre-event code | High | Unknown | Confirm rules early |\n\
         | Prakas details (injury deadlines, exclusions) unclear | High | Medium | Verify with lawyer |\n\
         | Internet fails on Demo Day | High | Low | Cache offline bundle |\n\n\
         ## 6. Open questions\n\n\
         | # | Question | Asked to |\n\
         | --- | --- | --- |\n\
         | Q1 | What are the judging criteria? | TSA |\n\
         | Q2 | Can we bring an existing prototype, and must code be written in build week? | TSA |\n\
         | Q3 | Is 1286 still the NSSF hotline? | NSSF |\n\
         | Q4 | Which facilities accept HEF? | MoH / PCA |\n\
         | Q5 | Can we combine ODC (CC BY-SA) with OSM data? | ODC |\n\n\
         ## 7. Weekly log\n\n\
         - Done: demo audit and bug fixes.\n\
         - Next: commit the fixes and verify.\n\
         - Blocked: nothing.\n",
    );

    let risks_start = content.find("| Risk | Impact |").unwrap();
    let questions_start = content.find("| # | Question |").unwrap();
    let weekly_start = content.find("## 7. Weekly log").unwrap();
    let end_marker = content.find("- Blocked: nothing.").unwrap();

    let (view, mut cx) = harness(&content, ViewMode::Wysiwyg);
    redraw(&mut cx);

    let pos = view.read_with(&cx, |app, _| app.editor_scroll.bounds().center());
    let mut prev_y = view.read_with(&cx, |app, _| f32::from(app.editor_scroll.offset().y));
    let mut saw_risks = false;
    let mut saw_questions = false;
    let mut questions_col1_widths: Vec<f32> = Vec::new();

    // Scroll down in Windows-like line wheel notches until reaching the bottom.
    for step in 0..80 {
        cx.simulate_event(ScrollWheelEvent {
            position: pos,
            delta: ScrollDelta::Lines(point(0.0, -3.0)),
            modifiers: Modifiers::default(),
            touch_phase: TouchPhase::Moved,
        });
        redraw(&mut cx);

        let cur_y = view.read_with(&cx, |app, _| f32::from(app.editor_scroll.offset().y));
        assert!(
            cur_y <= prev_y + 0.5,
            "step {step}: scrolling down bounced upward ({prev_y} -> {cur_y})"
        );
        prev_y = cur_y;

        view.read_with(&cx, |app, _| {
            let paint = app.editor_paint.borrow();
            for tbl in &paint.tables {
                if tbl.source.start == risks_start {
                    saw_risks = true;
                    assert_eq!(
                        tbl.columns.len(),
                        4,
                        "step {step}: Risks table must keep all 4 columns"
                    );
                } else if tbl.source.start == questions_start {
                    saw_questions = true;
                    assert_eq!(
                        tbl.columns.len(),
                        3,
                        "step {step}: Open questions table must keep 3 columns"
                    );
                    let q_col_w = f32::from(tbl.columns[1].header_bounds.size.width);
                    let id_col_w = f32::from(tbl.columns[0].header_bounds.size.width);
                    assert!(
                        q_col_w > id_col_w,
                        "step {step}: Question column ({q_col_w}) must be wider than # column ({id_col_w})"
                    );
                    questions_col1_widths.push(q_col_w);
                }
            }
        });
    }

    assert!(saw_risks, "should have scrolled past Risks table");
    assert!(
        saw_questions,
        "should have scrolled past Open questions table"
    );
    let first_w = questions_col1_widths[0];
    for (i, w) in questions_col1_widths.iter().enumerate() {
        assert!(
            (*w - first_w).abs() < 0.5,
            "Question column width must stay stable across scroll steps (step {i}: {w} vs {first_w})"
        );
    }

    // Confirm the bottom of the document (section 7) is visible in the viewport.
    view.read_with(&cx, |app, _| {
        let paint = app.editor_paint.borrow();
        let view_bounds = app.editor_scroll.bounds();
        let weekly_row = paint
            .rows
            .iter()
            .find(|r| r.byte_start() <= weekly_start && r.byte_end() >= weekly_start)
            .expect("Weekly log heading row should be materialized");
        let last_row = paint
            .rows
            .iter()
            .find(|r| r.byte_start() <= end_marker && r.byte_end() >= end_marker)
            .expect("final line should be materialized");
        assert!(
            weekly_row.origin.y >= view_bounds.top()
                && last_row.origin.y + last_row.line_height <= view_bounds.bottom() + px(1.0),
            "bottom content must scroll cleanly into view (last bottom={:?}, view bottom={:?})",
            last_row.origin.y + last_row.line_height,
            view_bounds.bottom()
        );
    });

    // Scroll back up to the top and back down again: cached table widths must
    // survive passing through the prose-only middle section and stay identical.
    for _ in 0..80 {
        cx.simulate_event(ScrollWheelEvent {
            position: pos,
            delta: ScrollDelta::Lines(point(0.0, 3.0)),
            modifiers: Modifiers::default(),
            touch_phase: TouchPhase::Moved,
        });
        redraw(&mut cx);
    }
    let mut prev_y = view.read_with(&cx, |app, _| f32::from(app.editor_scroll.offset().y));
    for step in 0..80 {
        cx.simulate_event(ScrollWheelEvent {
            position: pos,
            delta: ScrollDelta::Lines(point(0.0, -3.0)),
            modifiers: Modifiers::default(),
            touch_phase: TouchPhase::Moved,
        });
        redraw(&mut cx);
        let cur_y = view.read_with(&cx, |app, _| f32::from(app.editor_scroll.offset().y));
        assert!(
            cur_y <= prev_y + 0.5,
            "second pass step {step}: scrolling down bounced upward ({prev_y} -> {cur_y})"
        );
        prev_y = cur_y;
        view.read_with(&cx, |app, _| {
            let paint = app.editor_paint.borrow();
            for tbl in &paint.tables {
                if tbl.source.start == questions_start {
                    assert_eq!(tbl.columns.len(), 3);
                    let w = f32::from(tbl.columns[1].header_bounds.size.width);
                    assert!(
                        (w - first_w).abs() < 0.5,
                        "second pass step {step}: Question column width drifted ({w} vs {first_w})"
                    );
                }
            }
        });
    }
}

#[std_test]
fn scrolling_up_past_unmeasured_tables_anchors_visible_content() {
    let mut doc = String::from("# Top\n\n");
    for i in 0..45 {
        doc.push_str(&format!("Top paragraph line {i} with filler prose.\n"));
    }
    // Two multi-line wrapped tables in the middle, initially far below the viewport.
    for t in 0..2 {
        doc.push_str(&format!(
            "\n## Middle Table {t}\n\n| Col A | Col B | Detailed Description Column |\n| --- | --- | --- |\n"
        ));
        for r in 0..8 {
            doc.push_str(&format!(
                "| Item {t}-{r} | Owner {r} | A deliberately long table cell description that wraps onto several visual lines inside a narrow column to expand row height when measured. |\n"
            ));
        }
    }
    doc.push_str("\n## Bottom Section\n\n");
    for i in 0..60 {
        doc.push_str(&format!("Bottom paragraph line {i} with steady prose.\n"));
    }

    let (view, mut cx) = harness(&doc, ViewMode::Wysiwyg);
    let bottom_marker = doc.find("Bottom paragraph line 30").expect("bottom marker");

    // Jump directly to the bottom without ever scrolling down through the middle tables.
    view.update(&mut cx, |app, _cx| {
        let max = app.editor_scroll.max_offset().height;
        app.editor_scroll.set_offset(point(px(0.0), -max));
    });
    redraw(&mut cx);
    redraw(&mut cx);

    let pos = view.read_with(&cx, |app, _| app.editor_scroll.bounds().center());

    // Scroll UP step by step so the unmeasured middle tables enter ShapingBand above us.
    const WHEEL_STEP_PX: f32 = 78.0;
    let mut max_single_step_shift = 0.0f32;
    let mut saw_anchor = false;
    for step in 0..35 {
        let before_y = view.read_with(&cx, |app, _| {
            let paint = app.editor_paint.borrow();
            let view_bounds = app.editor_scroll.bounds();
            paint
                .rows
                .iter()
                .find(|r| r.origin.y >= view_bounds.top() && r.origin.y < view_bounds.bottom())
                .map(|r| (r.byte_start(), f32::from(r.origin.y)))
        });

        cx.simulate_event(ScrollWheelEvent {
            position: pos,
            delta: ScrollDelta::Lines(point(0.0, 3.0)),
            modifiers: Modifiers::default(),
            touch_phase: TouchPhase::Moved,
        });
        redraw(&mut cx);

        if let Some((anchor_byte, y_before)) = before_y {
            let after_y = view.read_with(&cx, |app, _| {
                app.editor_paint
                    .borrow()
                    .rows
                    .iter()
                    .find(|r| r.byte_start() == anchor_byte)
                    .map(|r| f32::from(r.origin.y))
            });
            if let Some(y_after) = after_y {
                saw_anchor = true;
                let shift = y_after - y_before;
                max_single_step_shift = max_single_step_shift.max(shift);
                // One wheel step of three lines moves content by 78 px; allow
                // a little slack, but not enough to hide a bounce.
                assert!(
                    (-1.0..WHEEL_STEP_PX + 2.0).contains(&shift),
                    "step {step}: scrolling up caused abrupt jump ({y_before} -> {y_after}, delta={shift})"
                );
            }
        }
    }
    assert!(saw_anchor, "should have tracked visible anchor rows");
    // Ensure the bottom marker existed in our test doc.
    assert!(bottom_marker > 0);
}

#[std_test]
fn offscreen_thumbnail_image_rows_prepopulate_height_estimate() {
    let mut doc = String::from("# Intro\n\n");
    for i in 0..80 {
        doc.push_str(&format!(
            "Prose line {i} pushing thumbnail rows off-screen.\n"
        ));
    }
    let thumb_offset = doc.len();
    doc.push_str("![a](a.png) ![b](b.png) ![c](c.png) ![d](d.png)\n\n");
    doc.push_str("![e](e.png) ![f](f.png) ![g](g.png) ![h](h.png)\n\n");
    for i in 0..30 {
        doc.push_str(&format!("Trailing prose line {i}.\n"));
    }

    let (view, mut cx) = harness(&doc, ViewMode::Wysiwyg);
    // Let the initial height-sync redraw complete while still at scroll y = 0.
    redraw(&mut cx);

    let (initial_total, thumb_slot_h) = view.update(&mut cx, |app, _cx| {
        let parsed = app.active_tab_mut().expect("tab").parsed();
        let mut paint = app.editor_paint.borrow_mut();
        let (_, h) = paint
            .row_y_for_offset(thumb_offset, Some(&parsed))
            .expect("thumbnail row estimate");
        (f32::from(paint.total_height), f32::from(h))
    });
    assert!(
        thumb_slot_h > 100.0,
        "off-screen thumbnail row should be pre-measured to thumbnail height, got {thumb_slot_h}"
    );

    // Scroll down to the thumbnail rows and verify total_height stayed stable.
    view.update(&mut cx, |app, _cx| {
        let max = app.editor_scroll.max_offset().height;
        app.editor_scroll.set_offset(point(px(0.0), -max));
    });
    redraw(&mut cx);
    redraw(&mut cx);

    let after_total = view.read_with(&cx, |app, _| {
        f32::from(app.editor_paint.borrow().total_height)
    });
    assert!(
        (after_total - initial_total).abs() < 4.0,
        "total height should not jump when off-screen thumbnail rows enter viewport ({initial_total} -> {after_total})"
    );
}

#[std_test]
fn outline_jump_below_unmeasured_wrapped_tables_lands_at_top() {
    let mut doc = String::from("# Start\n\n");
    for i in 0..45 {
        doc.push_str(&format!("Opening line {i} pushing tables off-screen.\n"));
    }
    for t in 0..3 {
        doc.push_str(&format!(
            "\n## Table {t}\n\n| Key | Value | Long Notes |\n| --- | --- | --- |\n"
        ));
        for r in 0..6 {
            doc.push_str(&format!(
                "| K{t}-{r} | V{r} | Long wrapped text inside table cell {t}-{r} so its real height is several times larger than a single-line estimate. |\n"
            ));
        }
    }
    doc.push('\n');
    let target_offset = doc.len();
    doc.push_str("## Target Heading\n\n");
    for i in 0..50 {
        doc.push_str(&format!(
            "Post-target line {i} so heading can sit at top.\n"
        ));
    }

    let (view, mut cx) = harness(&doc, ViewMode::Wysiwyg);
    // Jump directly to Target Heading while all 3 tables above it are still unmeasured.
    view.update(&mut cx, |app, cx| {
        app.jump_to_heading(target_offset);
        cx.notify();
    });
    redraw(&mut cx);
    redraw(&mut cx);

    view.read_with(&cx, |app, _| {
        let paint = app.editor_paint.borrow();
        let view_bounds = app.editor_scroll.bounds();
        let target_row = paint
            .rows
            .iter()
            .find(|r| r.byte_start() <= target_offset && r.byte_end() >= target_offset)
            .expect("Target Heading should be materialized after outline jump");
        let dist_from_top = f32::from(target_row.origin.y - view_bounds.top());
        let expected_margin = f32::from(paint.line_height) * 1.5;
        assert!(
            (dist_from_top - expected_margin).abs() <= f32::from(paint.line_height) * 2.0,
            "Target Heading should land near top of viewport (dist={dist_from_top}, expected~{expected_margin})"
        );
    });
}

#[std_test]
fn wysiwyg_hit_test_outside_materialized_band_does_not_snap_to_bof_or_eof() {
    let doc: String = (0..400).map(|i| format!("Paragraph line {i}\n")).collect();
    let (view, mut cx) = harness(&doc, ViewMode::Wysiwyg);

    // Move caret to the middle and scroll to the middle so both top and bottom
    // of the doc are outside paint.rows.
    let mid_offset = doc.find("Paragraph line 200\n").expect("mid line");
    place_caret(&view, &mut cx, mid_offset);
    view.update(&mut cx, |app, _cx| {
        let max = app.editor_scroll.max_offset().height;
        app.editor_scroll.set_offset(point(px(0.0), -max * 0.5));
    });
    redraw(&mut cx);

    view.update(&mut cx, |app, _cx| {
        let parsed = app.active_tab_mut().expect("tab").parsed();
        let paint = app.editor_paint.borrow();
        let bounds = paint.bounds.expect("editor bounds");
        let first = paint.rows.first().expect("first materialized row");
        let last = paint.rows.last().expect("last materialized row");
        assert!(first.byte_start() > 0, "top of doc should be culled");
        assert!(
            last.byte_end() < paint.last_row_byte_end,
            "bottom of doc should be culled"
        );

        let mid_x = bounds.center().x;
        // Pick a Y strictly between content_top (line 0) and first materialized row.
        let above_y = paint.content_top + (first.origin.y - paint.content_top) * 0.5;
        let off_above = index_for_position_in(&paint, Some(&parsed), point(mid_x, above_y))
            .expect("offset above band");
        assert!(
            off_above > 0 && off_above < first.byte_start(),
            "above-band hit test should map proportionally instead of snapping to 0 (got {off_above}, first={})",
            first.byte_start()
        );
        // Without parsed doc, it should clamp to first row rather than jumping to 0.
        let clamp_above =
            index_for_position(&paint, point(mid_x, above_y)).expect("clamped offset above");
        assert_eq!(clamp_above, first.byte_start());

        // Pick a Y strictly between last materialized row and bounds.bottom().
        let last_bottom = last.origin.y + last.line_height;
        let below_y = last_bottom + (bounds.bottom() - last_bottom) * 0.5;
        let off_below = index_for_position_in(&paint, Some(&parsed), point(mid_x, below_y))
            .expect("offset below band");
        assert!(
            off_below > last.byte_end() && off_below < paint.last_row_byte_end,
            "below-band hit test should map proportionally instead of snapping to EOF (got {off_below}, last={}, eof={})",
            last.byte_end(),
            paint.last_row_byte_end
        );
        // Without parsed doc, it should clamp to last row rather than jumping to EOF.
        let clamp_below =
            index_for_position(&paint, point(mid_x, below_y)).expect("clamped offset below");
        assert_eq!(clamp_below, last.byte_end());
    });

    // Now put the caret back at byte 0 while scrolled to the middle, creating
    // an unmaterialized gap between the caret anchor interval and the viewport.
    view.update(&mut cx, |app, _cx| {
        app.active_tab_mut().expect("tab").place(0, false);
        let max = app.editor_scroll.max_offset().height;
        app.editor_scroll.set_offset(point(px(0.0), -max * 0.5));
    });
    redraw(&mut cx);
    // Keep scroll in the middle after caret_followed runs.
    view.update(&mut cx, |app, _cx| {
        let max = app.editor_scroll.max_offset().height;
        app.editor_scroll.set_offset(point(px(0.0), -max * 0.5));
    });
    redraw(&mut cx);

    view.update(&mut cx, |app, _cx| {
        let parsed = app.active_tab_mut().expect("tab").parsed();
        let paint = app.editor_paint.borrow();
        let bounds = paint.bounds.expect("editor bounds");
        let gap = paint
            .rows
            .windows(2)
            .find(|w| w[1].line_index > w[0].line_index + 1)
            .expect("should have a gap between caret anchor and viewport band");
        let gap_top = gap[0].origin.y + gap[0].line_height;
        let gap_mid_y = gap_top + (gap[1].origin.y - gap_top) * 0.5;
        let off_gap =
            index_for_position_in(&paint, Some(&parsed), point(bounds.center().x, gap_mid_y))
                .expect("offset in gap");
        assert!(
            off_gap > gap[0].byte_end() && off_gap < gap[1].byte_start(),
            "gap hit test should map proportionally inside the gap (got {off_gap}, gap={}..{})",
            gap[0].byte_end(),
            gap[1].byte_start()
        );
    });
}

/// Manual benchmark: cost of a redraw (idle and after an edit) on a ~5 MB,
/// 100k-line note. Run with
/// `cargo test --release bench_large_note_redraw -- --ignored --nocapture`.
#[ignore]
#[std_test]
fn bench_large_note_redraw() {
    use std::time::Instant;
    for mode in [ViewMode::Wysiwyg, ViewMode::Raw] {
        for lines in [3_000usize, 100_000] {
            let doc: String = (0..lines)
                .map(|i| {
                    format!(
                        "line {i} of a reasonably long plain paragraph of text, with **bold** bits
"
                    )
                })
                .collect();
            let (view, mut cx) = harness(&doc, mode);
            let idle = (0..10)
                .map(|_| {
                    let t = Instant::now();
                    redraw(&mut cx);
                    t.elapsed()
                })
                .min()
                .unwrap();
            let edit = (0..10)
                .map(|k| {
                    view.update(&mut cx, |app, _cx| {
                        let tab = app.active_tab_mut().expect("tab");
                        tab.place(10 + k, false);
                        tab.insert_text_in_mode(10 + k..10 + k, "x", mode);
                    });
                    let t = Instant::now();
                    redraw(&mut cx);
                    t.elapsed()
                })
                .min()
                .unwrap();
            eprintln!(
                "{mode:?} {lines:>7} lines: idle redraw {idle:?}, redraw after edit {edit:?}"
            );
        }
    }
}
