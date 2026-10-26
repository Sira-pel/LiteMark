//! Shift-selection and bulk-trash planning for the sidebar tree.
//!
//! Generic over the row id so the rules can be tested without a window.

use std::path::{Path, PathBuf};

/// Rows from `anchor` to `to` inclusive, in tree order. Falls back to just
/// `to` when either row is not visible.
pub fn extend_selection<T: Clone + PartialEq>(rows: &[T], anchor: &T, to: &T) -> (T, Vec<T>) {
    let pos = |t: &T| rows.iter().position(|x| x == t);
    match (pos(anchor), pos(to)) {
        (Some(a), Some(b)) => (anchor.clone(), rows[a.min(b)..=a.max(b)].to_vec()),
        _ => (to.clone(), vec![to.clone()]),
    }
}

/// Row index one step from `cursor` (clamped, no wrap). With no cursor,
/// Up starts at the last row and Down at the first.
pub fn step_cursor<T: PartialEq>(rows: &[T], cursor: Option<&T>, delta: i32) -> Option<usize> {
    if rows.is_empty() {
        return None;
    }
    let last = rows.len() as i32 - 1;
    Some(
        match cursor.and_then(|c| rows.iter().position(|t| t == c)) {
            Some(pos) => (pos as i32 + delta).clamp(0, last) as usize,
            None if delta < 0 => last as usize,
            None => 0,
        },
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BulkTrashPlan<T> {
    /// Nothing selected is still in the workspace.
    Nothing,
    /// A selected note has unsaved changes; trashing must wait.
    ModifiedOpen,
    /// Rows to trash. Rows inside a selected folder are dropped because the
    /// folder takes them along.
    Trash(Vec<T>),
}

/// `items` are `(row, path)` for rows still in the workspace; `folders`
/// marks which of them are folders. `modified_paths` are open notes with
/// unsaved changes.
pub fn plan_bulk_trash<T: Clone>(
    items: &[(T, PathBuf, bool)],
    modified_paths: &[PathBuf],
) -> BulkTrashPlan<T> {
    let folder_paths: Vec<&Path> = items
        .iter()
        .filter(|(_, _, is_folder)| *is_folder)
        .map(|(_, path, _)| path.as_path())
        .collect();
    let kept: Vec<&(T, PathBuf, bool)> = items
        .iter()
        .filter(|(_, path, _)| {
            !folder_paths
                .iter()
                .any(|folder| path.as_path() != *folder && path.starts_with(folder))
        })
        .collect();
    if kept.is_empty() {
        return BulkTrashPlan::Nothing;
    }
    if modified_paths
        .iter()
        .any(|m| kept.iter().any(|(_, path, _)| m.starts_with(path)))
    {
        return BulkTrashPlan::ModifiedOpen;
    }
    BulkTrashPlan::Trash(kept.into_iter().map(|(t, _, _)| t.clone()).collect())
}

/// Collects the outcome of trashing several rows so one message is shown
/// when the last one finishes.
#[derive(Debug)]
pub struct BulkTrashReport {
    total: usize,
    finished: usize,
    failures: Vec<String>,
}

impl BulkTrashReport {
    pub fn new(total: usize) -> Self {
        Self {
            total,
            finished: 0,
            failures: Vec::new(),
        }
    }

    /// True once every row has reported back.
    pub fn is_finished(&self) -> bool {
        self.finished >= self.total
    }

    /// True if any row has failed so far.
    pub fn has_failures(&self) -> bool {
        !self.failures.is_empty()
    }

    /// Record one result. Returns the message to show once every row has
    /// finished and at least one failed.
    pub fn record(&mut self, result: Result<(), String>) -> Option<String> {
        self.finished += 1;
        if let Err(err) = result {
            self.failures.push(err);
        }
        if self.finished < self.total || self.failures.is_empty() {
            return None;
        }
        Some(format!(
            "Moved {} of {} to the Trash. {}",
            self.total - self.failures.len(),
            self.total,
            self.failures[0]
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows() -> Vec<&'static str> {
        vec!["a", "b", "c", "d", "e"]
    }

    #[test]
    fn extend_selects_inclusive_range_in_either_direction() {
        let r = rows();
        assert_eq!(extend_selection(&r, &"b", &"d"), ("b", vec!["b", "c", "d"]));
        assert_eq!(extend_selection(&r, &"d", &"b"), ("d", vec!["b", "c", "d"]));
        assert_eq!(extend_selection(&r, &"c", &"c"), ("c", vec!["c"]));
    }

    #[test]
    fn extend_falls_back_to_target_when_a_row_is_hidden() {
        let r = rows();
        assert_eq!(extend_selection(&r, &"x", &"c"), ("c", vec!["c"]));
        assert_eq!(extend_selection(&r, &"a", &"x"), ("x", vec!["x"]));
    }

    #[test]
    fn step_clamps_at_both_ends_without_wrapping() {
        let r = rows();
        assert_eq!(step_cursor(&r, Some(&"c"), 1), Some(3));
        assert_eq!(step_cursor(&r, Some(&"c"), -1), Some(1));
        assert_eq!(step_cursor(&r, Some(&"e"), 1), Some(4));
        assert_eq!(step_cursor(&r, Some(&"a"), -1), Some(0));
    }

    #[test]
    fn step_without_cursor_starts_at_the_edge_for_the_direction() {
        let r = rows();
        assert_eq!(step_cursor(&r, None, 1), Some(0));
        assert_eq!(step_cursor(&r, None, -1), Some(4));
        assert_eq!(step_cursor(&r, Some(&"gone"), 1), Some(0));
        assert_eq!(step_cursor::<&str>(&[], None, 1), None);
    }

    fn item(id: &'static str, path: &str, folder: bool) -> (&'static str, PathBuf, bool) {
        (id, PathBuf::from(path), folder)
    }

    #[test]
    fn bulk_trash_skips_files_inside_a_selected_folder() {
        let items = [
            item("dir", "/w/dir", true),
            item("in", "/w/dir/in.md", false),
            item("sub", "/w/dir/sub", true),
            item("out", "/w/out.md", false),
        ];
        assert_eq!(
            plan_bulk_trash(&items, &[]),
            BulkTrashPlan::Trash(vec!["dir", "out"])
        );
    }

    #[test]
    fn bulk_trash_keeps_sibling_with_shared_name_prefix() {
        let items = [
            item("dir", "/w/dir", true),
            item("f", "/w/dir2/f.md", false),
        ];
        assert_eq!(
            plan_bulk_trash(&items, &[]),
            BulkTrashPlan::Trash(vec!["dir", "f"])
        );
    }

    #[test]
    fn bulk_trash_refuses_when_a_selected_note_is_modified() {
        let items = [item("a", "/w/a.md", false), item("b", "/w/b.md", false)];
        let modified = [PathBuf::from("/w/b.md")];
        assert_eq!(
            plan_bulk_trash(&items, &modified),
            BulkTrashPlan::ModifiedOpen
        );
    }

    #[test]
    fn bulk_trash_refuses_when_a_note_inside_a_selected_folder_is_modified() {
        let items = [
            item("dir", "/w/dir", true),
            item("in", "/w/dir/in.md", false),
        ];
        let modified = [PathBuf::from("/w/dir/in.md")];
        assert_eq!(
            plan_bulk_trash(&items, &modified),
            BulkTrashPlan::ModifiedOpen
        );
    }

    #[test]
    fn bulk_trash_ignores_modified_notes_outside_the_selection() {
        let items = [item("a", "/w/a.md", false)];
        let modified = [PathBuf::from("/w/other.md")];
        assert_eq!(
            plan_bulk_trash(&items, &modified),
            BulkTrashPlan::Trash(vec!["a"])
        );
    }

    #[test]
    fn bulk_trash_of_nothing_is_nothing() {
        assert_eq!(plan_bulk_trash::<&str>(&[], &[]), BulkTrashPlan::Nothing);
    }

    #[test]
    fn report_stays_silent_when_everything_succeeds() {
        let mut r = BulkTrashReport::new(2);
        assert_eq!(r.record(Ok(())), None);
        assert_eq!(r.record(Ok(())), None);
    }

    #[test]
    fn report_tracks_completion_and_failures() {
        let mut r = BulkTrashReport::new(2);
        assert!(!r.is_finished() && !r.has_failures());
        r.record(Err("gone".into()));
        assert!(!r.is_finished() && r.has_failures());
        r.record(Ok(()));
        assert!(r.is_finished());
    }

    #[test]
    fn report_counts_a_row_that_vanished_before_trashing() {
        let mut r = BulkTrashReport::new(3);
        assert_eq!(
            r.record(Err("That note is no longer in the folder.".into())),
            None
        );
        assert_eq!(r.record(Err("busy".into())), None);
        assert_eq!(
            r.record(Ok(())),
            Some("Moved 1 of 3 to the Trash. That note is no longer in the folder.".into())
        );
    }

    #[test]
    fn report_gives_one_message_after_the_last_row_finishes() {
        let mut r = BulkTrashReport::new(5);
        assert_eq!(r.record(Ok(())), None);
        assert_eq!(r.record(Err("permission denied".into())), None);
        assert_eq!(r.record(Ok(())), None);
        assert_eq!(r.record(Ok(())), None);
        assert_eq!(
            r.record(Err("busy".into())),
            Some("Moved 3 of 5 to the Trash. permission denied".into())
        );
    }
}
