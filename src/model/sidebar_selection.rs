//! Shift-selection and bulk-trash planning for the sidebar tree.
//!
//! Generic over the row id so the rules can be tested without a window.

use std::path::{Path, PathBuf};

use super::workspace::{WorkspaceFile, WorkspaceFolder};

/// Rows from `anchor` to `to` inclusive, in tree order. Falls back to just
/// `to` when either row is not visible.
pub fn extend_selection<T: Clone + PartialEq>(rows: &[T], anchor: &T, to: &T) -> (T, Vec<T>) {
    let pos = |t: &T| rows.iter().position(|x| x == t);
    match (pos(anchor), pos(to)) {
        (Some(a), Some(b)) => (anchor.clone(), rows[a.min(b)..=a.max(b)].to_vec()),
        _ => (to.clone(), vec![to.clone()]),
    }
}

/// Ctrl+click: add `target` to `items`, or drop it if present. Returns
/// `(anchor, cursor, items)`, or `None` once nothing is left. The cursor is the
/// clicked row, or the last remaining one after a removal; the anchor stays
/// unless it was the row removed.
pub fn toggle_row<T: Clone + PartialEq>(
    mut items: Vec<T>,
    anchor: Option<T>,
    target: T,
) -> Option<(T, T, Vec<T>)> {
    match items.iter().position(|t| *t == target) {
        Some(i) => {
            items.remove(i);
        }
        None => items.push(target.clone()),
    }
    let cursor = if items.contains(&target) { target } else { items.last()?.clone() };
    let anchor = anchor
        .filter(|a| items.contains(a))
        .unwrap_or_else(|| cursor.clone());
    Some((anchor, cursor, items))
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

/// Notes whose name contains `query` (case-insensitive), in workspace order.
pub fn sidebar_search_matches<'a>(
    files: &'a [WorkspaceFile],
    query: &str,
) -> Vec<&'a WorkspaceFile> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Vec::new();
    }
    files
        .iter()
        .filter(|f| {
            let stem = f.path.file_stem().and_then(|s| s.to_str());
            stem.unwrap_or(&f.name).to_lowercase().contains(&query)
        })
        .collect()
}

/// Folders whose name contains `query` (case-insensitive), in workspace order.
pub fn sidebar_folder_search_matches<'a>(
    folders: &'a [WorkspaceFolder],
    query: &str,
) -> Vec<&'a WorkspaceFolder> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Vec::new();
    }
    folders
        .iter()
        .filter(|f| f.name.to_lowercase().contains(&query))
        .collect()
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

/// Rows not inside another selected folder; a folder carries its children.
fn top_level_rows<T>(items: &[(T, PathBuf, bool)]) -> Vec<&(T, PathBuf, bool)> {
    let folder_paths: Vec<&Path> = items
        .iter()
        .filter(|(_, _, is_folder)| *is_folder)
        .map(|(_, path, _)| path.as_path())
        .collect();
    items
        .iter()
        .filter(|(_, path, _)| {
            !folder_paths
                .iter()
                .any(|folder| path.as_path() != *folder && path.starts_with(folder))
        })
        .collect()
}

/// Drops selected folders that were opened up and only partly selected: some
/// row inside is selected, but a visible one is not. Moving or trashing the
/// folder would take the skipped row too, so the folder stays and only its
/// selected rows go. A folder with nothing selected inside is taken whole.
pub fn without_partial_folders<T: Clone>(
    items: &[(T, PathBuf, bool)],
    visible: &[PathBuf],
) -> Vec<(T, PathBuf, bool)> {
    let selected = |path: &PathBuf| items.iter().any(|(_, p, _)| p == path);
    items
        .iter()
        .filter(|(_, folder, is_folder)| {
            if !*is_folder {
                return true;
            }
            let inside = |row: &&PathBuf| *row != folder && row.starts_with(folder);
            let any_selected = items.iter().any(|(_, p, _)| p != folder && p.starts_with(folder));
            let any_skipped = visible.iter().filter(inside).any(|row| !selected(row));
            !(any_selected && any_skipped)
        })
        .cloned()
        .collect()
}

/// `top_level_rows` for callers (such as moving) that only need the rows.
pub fn top_level_targets<T: Clone>(items: &[(T, PathBuf, bool)]) -> Vec<T> {
    top_level_rows(items)
        .into_iter()
        .map(|(t, _, _)| t.clone())
        .collect()
}

/// `items` are `(row, path)` for rows still in the workspace; `folders`
/// marks which of them are folders. `modified_paths` are open notes with
/// unsaved changes.
pub fn plan_bulk_trash<T: Clone>(
    items: &[(T, PathBuf, bool)],
    modified_paths: &[PathBuf],
) -> BulkTrashPlan<T> {
    let kept = top_level_rows(items);
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
    fn partial_folder_is_dropped_but_its_selected_rows_stay() {
        let p = |s: &str| PathBuf::from(s);
        let items = vec![
            (1, p("/w/dir"), true),
            (2, p("/w/dir/a.md"), false),
        ];
        let visible = vec![p("/w/dir"), p("/w/dir/a.md"), p("/w/dir/b.md")];
        let got = without_partial_folders(&items, &visible);
        assert_eq!(got, vec![(2, p("/w/dir/a.md"), false)]);
    }

    #[test]
    fn fully_selected_or_collapsed_folder_stays_whole() {
        let p = |s: &str| PathBuf::from(s);
        let items = vec![(1, p("/w/dir"), true), (2, p("/w/dir/a.md"), false)];
        let all = vec![p("/w/dir"), p("/w/dir/a.md")];
        assert_eq!(without_partial_folders(&items, &all), items);
        let collapsed = vec![p("/w/dir")];
        let only_folder = vec![(1, p("/w/dir"), true)];
        assert_eq!(without_partial_folders(&only_folder, &collapsed), only_folder);
        let expanded = vec![p("/w/dir"), p("/w/dir/a.md")];
        assert_eq!(without_partial_folders(&only_folder, &expanded), only_folder);
    }

    #[test]
    fn toggle_adds_a_row_and_moves_the_cursor_to_it() {
        let got = toggle_row(vec!["a"], Some("a"), "c");
        assert_eq!(got, Some(("a", "c", vec!["a", "c"])));
    }

    #[test]
    fn toggle_removes_a_middle_row() {
        let got = toggle_row(vec!["a", "b", "c"], Some("a"), "b");
        assert_eq!(got, Some(("a", "c", vec!["a", "c"])));
    }

    #[test]
    fn toggle_down_to_one_row_keeps_that_row() {
        let got = toggle_row(vec!["a", "b"], Some("a"), "b");
        assert_eq!(got, Some(("a", "a", vec!["a"])));
    }

    #[test]
    fn toggle_removing_the_anchor_re_anchors_on_the_cursor() {
        let got = toggle_row(vec!["a", "b", "c"], Some("a"), "a");
        assert_eq!(got, Some(("c", "c", vec!["b", "c"])));
    }

    #[test]
    fn toggle_removing_the_last_row_clears_the_selection() {
        assert_eq!(toggle_row(vec!["a"], None, "a"), None);
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
    fn top_level_targets_drop_rows_inside_a_selected_folder() {
        let items = [
            item("dir", "/w/dir", true),
            item("in", "/w/dir/in.md", false),
            item("out", "/w/out.md", false),
        ];
        assert_eq!(top_level_targets(&items), vec!["dir", "out"]);
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

    fn note(name: &str) -> WorkspaceFile {
        WorkspaceFile {
            id: format!("file-{name}"),
            name: name.into(),
            folder_id: None,
            path: PathBuf::from(name),
            file_type: super::super::document::FileType::Markdown,
            updated_at: 0,
        }
    }

    #[test]
    fn search_matches_names_case_insensitively_in_order() {
        let files = [
            note("Welcome.md"),
            note("Untitled.md"),
            note("Untitled 2.md"),
        ];
        let hits: Vec<&str> = sidebar_search_matches(&files, "UNTIT")
            .into_iter()
            .map(|f| f.name.as_str())
            .collect();
        assert_eq!(hits, ["Untitled.md", "Untitled 2.md"]);
    }

    #[test]
    fn search_ignores_the_file_extension() {
        let files = [note("Welcome.md")];
        assert!(sidebar_search_matches(&files, "md").is_empty());
        assert_eq!(sidebar_search_matches(&files, "come").len(), 1);
    }

    #[test]
    fn blank_or_unmatched_search_returns_nothing() {
        let files = [note("Welcome.md")];
        assert!(sidebar_search_matches(&files, "").is_empty());
        assert!(sidebar_search_matches(&files, "   ").is_empty());
        assert!(sidebar_search_matches(&files, "zzz").is_empty());
    }

    #[test]
    fn search_trims_surrounding_spaces() {
        let files = [note("Welcome.md")];
        assert_eq!(sidebar_search_matches(&files, "  welcome ").len(), 1);
    }

    #[test]
    fn folder_search_matches_names_case_insensitively() {
        let folder = |name: &str| WorkspaceFolder {
            id: name.into(),
            name: name.into(),
            parent_id: None,
            path: PathBuf::from(name),
            is_open: false,
        };
        let folders = [folder("Projects"), folder("Journal")];
        let hits: Vec<&str> = sidebar_folder_search_matches(&folders, " PROJ ")
            .into_iter()
            .map(|f| f.name.as_str())
            .collect();
        assert_eq!(hits, ["Projects"]);
        assert!(sidebar_folder_search_matches(&folders, "").is_empty());
    }
}
