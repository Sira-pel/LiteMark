//! The recent-notes list: newest first, no duplicates, capped.

use std::path::{Path, PathBuf};

/// Most notes the recent list keeps.
pub const MAX_RECENT_NOTES: usize = 20;
/// Most recent notes the narrow-window menu lists inline.
pub const MAX_INLINE_RECENT_NOTES: usize = 5;

/// Put `path` at the front of `list`, dropping any older copy and anything
/// past the cap. Returns whether the list changed.
pub fn push_recent(list: &mut Vec<PathBuf>, path: &Path) -> bool {
    if list.first().map(PathBuf::as_path) == Some(path) {
        return false;
    }
    list.retain(|p| p != path);
    list.insert(0, path.to_path_buf());
    list.truncate(MAX_RECENT_NOTES);
    true
}

/// Follow a rename or move of `old` (a note or a folder) to `new`. Entries
/// inside a moved folder follow it. Returns whether the list changed.
pub fn rename_recent(list: &mut Vec<PathBuf>, old: &Path, new: &Path) -> bool {
    let mut changed = false;
    for entry in list.iter_mut() {
        let moved = if entry.as_path() == old {
            Some(new.to_path_buf())
        } else {
            entry.strip_prefix(old).ok().map(|rest| new.join(rest))
        };
        if let Some(moved) = moved {
            *entry = moved;
            changed = true;
        }
    }
    if changed {
        dedupe_recent(list);
    }
    changed
}

/// Drop entries `exists` rejects (trashed or missing notes). Returns
/// whether the list changed.
pub fn retain_existing(list: &mut Vec<PathBuf>, exists: impl Fn(&Path) -> bool) -> bool {
    let before = list.len();
    list.retain(|p| exists(p));
    list.len() != before
}

/// Keep the first copy of each path and at most [`MAX_RECENT_NOTES`].
pub fn dedupe_recent(list: &mut Vec<PathBuf>) {
    let mut seen: Vec<PathBuf> = Vec::with_capacity(list.len());
    list.retain(|p| {
        if seen.contains(p) {
            false
        } else {
            seen.push(p.clone());
            true
        }
    });
    list.truncate(MAX_RECENT_NOTES);
}

/// Menu text for a recent note: its file name, and the folder it is in,
/// relative to the workspace when inside it.
pub fn recent_note_label(path: &Path, workspace: &Path) -> (String, String) {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    let parent = path.parent().unwrap_or(Path::new(""));
    let folder = match parent.strip_prefix(workspace) {
        Ok(rest) if rest.as_os_str().is_empty() => workspace
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        Ok(rest) => rest.display().to_string(),
        Err(_) => parent.display().to_string(),
    };
    (name, folder)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(list: &[&str]) -> Vec<PathBuf> {
        list.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn push_puts_newest_first_without_duplicates() {
        let mut list = Vec::new();
        assert!(push_recent(&mut list, Path::new("/a.md")));
        assert!(push_recent(&mut list, Path::new("/b.md")));
        assert!(push_recent(&mut list, Path::new("/a.md")));
        assert_eq!(list, paths(&["/a.md", "/b.md"]));
        assert!(!push_recent(&mut list, Path::new("/a.md")), "already first");
    }

    #[test]
    fn push_caps_the_list() {
        let mut list = Vec::new();
        for i in 0..30 {
            push_recent(&mut list, Path::new(&format!("/n{i}.md")));
        }
        assert_eq!(list.len(), MAX_RECENT_NOTES);
        assert_eq!(list[0], PathBuf::from("/n29.md"));
        assert_eq!(list[MAX_RECENT_NOTES - 1], PathBuf::from("/n10.md"));
    }

    #[test]
    fn rename_follows_a_note_and_a_folder() {
        let mut list = paths(&["/w/a.md", "/w/sub/b.md", "/w/other.md"]);
        assert!(rename_recent(
            &mut list,
            Path::new("/w/a.md"),
            Path::new("/w/c.md")
        ));
        assert!(rename_recent(
            &mut list,
            Path::new("/w/sub"),
            Path::new("/w/moved")
        ));
        assert_eq!(list, paths(&["/w/c.md", "/w/moved/b.md", "/w/other.md"]));
        assert!(!rename_recent(
            &mut list,
            Path::new("/w/none.md"),
            Path::new("/w/x.md")
        ));
    }

    #[test]
    fn rename_does_not_match_a_sibling_with_a_longer_name() {
        let mut list = paths(&["/w/sub2/b.md"]);
        assert!(!rename_recent(
            &mut list,
            Path::new("/w/sub"),
            Path::new("/w/x")
        ));
        assert_eq!(list, paths(&["/w/sub2/b.md"]));
    }

    #[test]
    fn rename_onto_an_existing_entry_keeps_one_copy() {
        let mut list = paths(&["/w/a.md", "/w/b.md"]);
        rename_recent(&mut list, Path::new("/w/b.md"), Path::new("/w/a.md"));
        assert_eq!(list, paths(&["/w/a.md"]));
    }

    #[test]
    fn missing_entries_are_dropped() {
        let mut list = paths(&["/a.md", "/gone.md", "/b.md"]);
        assert!(retain_existing(&mut list, |p| p != Path::new("/gone.md")));
        assert_eq!(list, paths(&["/a.md", "/b.md"]));
        assert!(!retain_existing(&mut list, |_| true));
    }

    #[test]
    fn labels_show_the_folder_relative_to_the_workspace() {
        let ws = Path::new("/home/me/Notes");
        assert_eq!(
            recent_note_label(Path::new("/home/me/Notes/a.md"), ws),
            ("a.md".to_string(), "Notes".to_string())
        );
        let (name, folder) = recent_note_label(Path::new("/home/me/Notes/work/b.md"), ws);
        assert_eq!(name, "b.md");
        assert_eq!(PathBuf::from(folder), PathBuf::from("work"));
        let (_, outside) = recent_note_label(Path::new("/tmp/c.md"), ws);
        assert_eq!(PathBuf::from(outside), PathBuf::from("/tmp"));
    }
}
