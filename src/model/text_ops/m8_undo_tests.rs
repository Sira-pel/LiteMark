//! W14b/c: span-group and delete coalescing limits in the undo history.

use super::*;
use std::time::Duration;

fn t0() -> Instant {
    Instant::now()
}

fn open_group(h: &mut UndoHistory, at: Instant) {
    // `**h**` typed at 0 with the span group open, as Ctrl+B + typing does.
    h.push_at(DiffEdit::new(0, String::new(), "**h**".into()), at);
    h.span_group_open = true;
}

#[test]
fn w14b_span_group_expires_after_burst_window() {
    let start = t0();
    let mut h = UndoHistory::default();
    open_group(&mut h, start);
    // Inside the window: absorbed.
    h.push_at(
        DiffEdit::new(3, String::new(), "i".into()),
        start + Duration::from_millis(100),
    );
    assert_eq!(h.undo.len(), 1);
    // After a pause: its own step.
    h.push_at(
        DiffEdit::new(4, String::new(), "j".into()),
        start + Duration::from_millis(100 + TYPING_BURST_MS + 50),
    );
    assert_eq!(h.undo.len(), 2);
}

#[test]
fn w14b_span_group_breaks_at_word_boundary() {
    let start = t0();
    let mut h = UndoHistory::default();
    open_group(&mut h, start);
    h.push_at(
        DiffEdit::new(3, String::new(), "ab".into()),
        start + Duration::from_millis(10),
    );
    assert_eq!(h.undo.len(), 1);
    // A space inside the span is its own step, as when typing outside one.
    h.push_at(
        DiffEdit::new(5, String::new(), " ".into()),
        start + Duration::from_millis(20),
    );
    assert_eq!(h.undo.len(), 2);
}

#[test]
fn w14b_span_group_growth_respects_undo_budget() {
    let start = t0();
    let mut h = UndoHistory::default();
    let chunk = "x".repeat(1024 * 1024);
    for i in 0..7usize {
        h.push_at(
            DiffEdit::new(100_000_000 * (i + 1), String::new(), chunk.clone()),
            start,
        );
    }
    open_group(&mut h, start);
    h.push_at(
        DiffEdit::new(3, String::new(), "y".repeat(2 * 1024 * 1024)),
        start + Duration::from_millis(5),
    );
    assert!(
        h.stored_bytes() <= UNDO_BYTE_BUDGET,
        "stored {}",
        h.stored_bytes()
    );
}

#[test]
fn w14c_backspace_run_merges_within_window_only() {
    let start = t0();
    let mut h = UndoHistory::default();
    h.push_at(DiffEdit::new(10, "o".into(), String::new()), start);
    h.push_at(
        DiffEdit::new(9, "l".into(), String::new()),
        start + Duration::from_millis(50),
    );
    assert_eq!(h.undo.len(), 1);
    let e = h.undo.back().unwrap();
    assert_eq!((e.start(), &*e.deleted, e.inserted.as_str()), (9, "lo", ""));
    // After a pause a new step starts.
    h.push_at(
        DiffEdit::new(8, "l".into(), String::new()),
        start + Duration::from_millis(50 + TYPING_BURST_MS + 10),
    );
    assert_eq!(h.undo.len(), 2);
}

#[test]
fn w14c_forward_delete_run_merges() {
    let start = t0();
    let mut h = UndoHistory::default();
    h.push_at(DiffEdit::new(5, "a".into(), String::new()), start);
    h.push_at(
        DiffEdit::new(5, "b".into(), String::new()),
        start + Duration::from_millis(20),
    );
    assert_eq!(h.undo.len(), 1);
    let e = h.undo.back().unwrap();
    assert_eq!((e.start(), &*e.deleted), (5, "ab"));
}

#[test]
fn w14c_non_adjacent_multi_char_or_space_deletes_do_not_merge() {
    let start = t0();
    let at = |ms| start + Duration::from_millis(ms);
    let mut h = UndoHistory::default();
    h.push_at(DiffEdit::new(10, "o".into(), String::new()), start);
    // Not adjacent.
    h.push_at(DiffEdit::new(3, "x".into(), String::new()), at(10));
    assert_eq!(h.undo.len(), 2);
    // More than one character at once (a selection delete).
    h.push_at(DiffEdit::new(2, "yz".into(), String::new()), at(20));
    assert_eq!(h.undo.len(), 3);
    // Whitespace stays its own step.
    let mut h = UndoHistory::default();
    h.push_at(DiffEdit::new(10, "o".into(), String::new()), start);
    h.push_at(DiffEdit::new(9, " ".into(), String::new()), at(10));
    assert_eq!(h.undo.len(), 2);
    // A replacement is never a pure delete.
    let mut h = UndoHistory::default();
    h.push_at(DiffEdit::new(10, "o".into(), String::new()), start);
    h.push_at(DiffEdit::new(9, "l".into(), "L".into()), at(10));
    assert_eq!(h.undo.len(), 2);
}
