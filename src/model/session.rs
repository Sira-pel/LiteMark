//! Tab identity, close/save planning, and file-dialog availability.
//!
//! These decisions are pure so the data-loss bugs can be regression-tested
//! without opening a window.

use std::path::{Path, PathBuf};

use super::document::FileType;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabIds {
    next: u64,
}

impl Default for TabIds {
    fn default() -> Self {
        Self::new()
    }
}

impl TabIds {
    pub fn new() -> Self {
        Self { next: 0 }
    }

    /// Ids only increase. Closing a tab never recycles an earlier id.
    pub fn alloc(&mut self) -> String {
        self.next += 1;
        format!("tab-{}", self.next)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TabSnap<'a> {
    pub id: &'a str,
    pub modified: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClosePlan {
    /// The only tab is clean. Leave its buffer alone.
    KeepLast,
    Prompt,
    Close {
        next_active: Option<String>,
    },
}

pub fn plan_close(tabs: &[TabSnap<'_>], closing_id: &str) -> ClosePlan {
    let Some(pos) = tabs.iter().position(|tab| tab.id == closing_id) else {
        return ClosePlan::KeepLast;
    };
    if tabs[pos].modified {
        return ClosePlan::Prompt;
    }
    if tabs.len() <= 1 {
        return ClosePlan::KeepLast;
    }
    let next = if pos + 1 < tabs.len() {
        Some(tabs[pos + 1].id.to_string())
    } else if pos > 0 {
        Some(tabs[pos - 1].id.to_string())
    } else {
        None
    };
    ClosePlan::Close { next_active: next }
}

/// Index of an already-open tab for `path`, if any.
pub fn tab_index_for_path(paths: &[Option<&Path>], path: &Path) -> Option<usize> {
    paths.iter().position(|existing| *existing == Some(path))
}

/// True when `dest` is the path of some tab other than `active`.
pub fn path_owned_by_other_tab(paths: &[Option<&Path>], active: usize, dest: &Path) -> bool {
    paths
        .iter()
        .enumerate()
        .any(|(index, existing)| index != active && *existing == Some(dest))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrashPlan {
    /// Every affected tab is clean. Deleting will not drop unsaved edits.
    Proceed,
    /// At least one affected tab has unsaved edits. Ask before deleting.
    Prompt,
}

pub fn plan_trash(affected_modified: impl IntoIterator<Item = bool>) -> TrashPlan {
    if affected_modified.into_iter().any(|modified| modified) {
        TrashPlan::Prompt
    } else {
        TrashPlan::Proceed
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrashChoice {
    Save,
    Discard,
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrashAction {
    Abort,
    SaveThenTrash,
    DiscardThenTrash,
}

pub fn trash_action(choice: TrashChoice) -> TrashAction {
    match choice {
        TrashChoice::Save => TrashAction::SaveThenTrash,
        TrashChoice::Discard => TrashAction::DiscardThenTrash,
        TrashChoice::Cancel => TrashAction::Abort,
    }
}

/// Step the selected modal choice forwards or backwards with wrapping.
pub fn step_prompt_selection(current: usize, count: usize, step_back: bool) -> usize {
    if count == 0 {
        return 0;
    }
    if step_back {
        (current + count - 1) % count
    } else {
        (current + 1) % count
    }
}

/// Index of the default choice for a prompt modal, or 0 if none is marked default.
pub fn default_prompt_choice_index(choices: &[(&'static str, bool)]) -> usize {
    choices.iter().position(|(_, default)| *default).unwrap_or(0)
}

/// Clicking a tab id activates that tab, including a newly created untitled tab.
pub fn active_after_click(tab_ids: &[&str], clicked: &str, current: &str) -> String {
    if tab_ids.contains(&clicked) {
        clicked.to_string()
    } else {
        current.to_string()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedWrite {
    pub path: PathBuf,
    pub title: String,
}

pub fn is_pdf_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("pdf"))
}

pub fn plan_save(
    file_type: FileType,
    file_path: Option<&Path>,
    title: &str,
    folder: &Path,
    name_taken: impl Fn(&str) -> bool,
) -> Result<PlannedWrite, String> {
    if file_type == FileType::Pdf || file_path.is_some_and(is_pdf_path) {
        return Err("PDF files are read-only. Use Save As to write a copy.".into());
    }
    if let Some(path) = file_path {
        let title = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| title.to_string());
        return Ok(PlannedWrite {
            path: path.to_path_buf(),
            title,
        });
    }
    let desired = if title.contains('.') {
        title.to_string()
    } else {
        format!("{title}.md")
    };
    let free = unique_name(&desired, &name_taken);
    Ok(PlannedWrite {
        path: folder.join(&free),
        title: free,
    })
}

/// `New Note.md`, then `New Note 2.md`, `New Note 3.md`, ...
pub fn unique_name(desired: &str, taken: &impl Fn(&str) -> bool) -> String {
    let desired = if desired.trim().is_empty() {
        "Untitled.md".to_string()
    } else {
        desired.to_string()
    };
    if !taken(&desired) {
        return desired;
    }
    let (stem, ext) = split_stem_ext(&desired);
    let mut number = 2u32;
    loop {
        let candidate = format!("{stem} {number}{ext}");
        if !taken(&candidate) {
            return candidate;
        }
        number += 1;
        if number > 10_000 {
            return format!("{stem} {number}{ext}");
        }
    }
}

fn split_stem_ext(name: &str) -> (String, String) {
    if let Some((stem, ext)) = name.rsplit_once('.') {
        if !stem.is_empty() && !ext.is_empty() {
            return (stem.to_string(), format!(".{ext}"));
        }
    }
    (name.to_string(), String::new())
}

pub fn child_directory(
    root: &Path,
    folders: &[(String, PathBuf)],
    folder_id: Option<&str>,
) -> PathBuf {
    if let Some(id) = folder_id {
        if let Some((_, path)) = folders.iter().find(|(folder, _)| folder == id) {
            return path.clone();
        }
    }
    root.to_path_buf()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusSlot {
    Editor,
    Menu,
    Prompt,
    Sidebar,
}

/// Which surface should receive typed text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextInput {
    Editor,
    Find,
    Replace,
    Rename,
    Create,
    Search,
}

/// Printable keys and editing actions reach the document only while the editor owns input.
pub fn document_accepts_keystrokes(owner: TextInput) -> bool {
    owner == TextInput::Editor
}

/// Sidebar keyboard mode ends once a text field other than the editor is claimed,
/// otherwise typing and Select All would keep going to the sidebar.
pub fn sidebar_keyboard_after_claim(was_on: bool, claimed: TextInput) -> bool {
    was_on && claimed == TextInput::Editor
}

/// Undo and redo always edit the document, including while Replace is focused.
/// Other format commands stay with the editor so the field keeps its own keys.
pub fn command_edits_document(cmd: &str, owner: TextInput, field_owns_keys: bool) -> bool {
    if cmd == "undo" || cmd == "redo" {
        return true;
    }
    document_accepts_keystrokes(owner) && !field_owns_keys
}

/// Focus is applied only when the desired slot changes.
pub fn focus_if_changed(previous: Option<FocusSlot>, next: FocusSlot) -> bool {
    previous != Some(next)
}

/// Only Linux probes the desktop portal; other systems always report `Ready`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub enum PortalProbe {
    Ready,
    NotOwned,
    NoSessionBus,
    HelperMissing { portal_installed: bool },
}

pub fn file_dialog_block_reason(probe: PortalProbe) -> Option<&'static str> {
    match probe {
        PortalProbe::Ready => None,
        PortalProbe::NotOwned => Some(
            "The file dialog could not be opened because xdg-desktop-portal is not running.",
        ),
        PortalProbe::NoSessionBus => Some(
            "The file dialog could not be opened because there is no D-Bus session bus for xdg-desktop-portal.",
        ),
        PortalProbe::HelperMissing { portal_installed: true } => Some(
            "The file dialog could not be opened because the desktop portal could not be queried.",
        ),
        PortalProbe::HelperMissing { portal_installed: false } => Some(
            "The file dialog could not be opened because xdg-desktop-portal is not installed.",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claiming_a_text_field_ends_sidebar_keyboard_mode() {
        for field in [
            TextInput::Search,
            TextInput::Find,
            TextInput::Replace,
            TextInput::Rename,
            TextInput::Create,
        ] {
            assert!(!sidebar_keyboard_after_claim(true, field));
        }
        assert!(sidebar_keyboard_after_claim(true, TextInput::Editor));
        assert!(!sidebar_keyboard_after_claim(false, TextInput::Search));
    }

    #[test]
    fn tab_ids_stay_unique_after_a_close_and_click_activates_the_new_tab() {
        let mut ids = TabIds::new();
        let first = ids.alloc();
        let second = ids.alloc();
        assert_eq!(first, "tab-1");
        assert_eq!(second, "tab-2");
        // Closing `second` must not make the next tab reuse tab-2.
        let third = ids.alloc();
        assert_eq!(third, "tab-3");
        assert_ne!(second, third);
        let open = [first.as_str(), third.as_str()];
        assert_eq!(active_after_click(&open, &third, &first), third);
    }

    #[test]
    fn closing_the_last_clean_tab_does_not_clear_it() {
        let tabs = [TabSnap {
            id: "tab-1",
            modified: false,
        }];
        assert_eq!(plan_close(&tabs, "tab-1"), ClosePlan::KeepLast);
    }

    #[test]
    fn closing_a_dirty_tab_prompts_and_a_clean_extra_tab_closes() {
        let tabs = [
            TabSnap {
                id: "tab-1",
                modified: false,
            },
            TabSnap {
                id: "tab-3",
                modified: true,
            },
        ];
        assert_eq!(plan_close(&tabs, "tab-3"), ClosePlan::Prompt);
        assert_eq!(
            plan_close(&tabs, "tab-1"),
            ClosePlan::Close {
                next_active: Some("tab-3".into())
            }
        );
    }

    #[test]
    fn save_refuses_pdf_and_picks_a_free_untitled_name() {
        let folder = Path::new("/notes");
        let err = plan_save(
            FileType::Pdf,
            Some(Path::new("/a/file.pdf")),
            "file.pdf",
            folder,
            |_| false,
        );
        assert!(err.unwrap_err().contains("read-only"));

        let taken = |name: &str| name == "Untitled-1.md";
        let plan = plan_save(FileType::Markdown, None, "Untitled-1.md", folder, taken).unwrap();
        assert_eq!(plan.title, "Untitled-1 2.md");
        assert_eq!(plan.path, folder.join("Untitled-1 2.md"));

        let existing = plan_save(
            FileType::Markdown,
            Some(Path::new("/notes/Welcome.md")),
            "Welcome.md",
            folder,
            |_| true,
        )
        .unwrap();
        assert_eq!(existing.path, Path::new("/notes/Welcome.md"));
    }

    #[test]
    fn new_note_names_and_folder_placement() {
        let mut used = vec!["New Note.md".to_string()];
        let taken = |name: &str| used.iter().any(|have| have == name);
        let next = unique_name("New Note.md", &taken);
        assert_eq!(next, "New Note 2.md");
        used.push(next);
        assert_eq!(
            unique_name("New Note.md", &|name| used.iter().any(|have| have == name)),
            "New Note 3.md"
        );

        let root = PathBuf::from("/notes");
        let folders = [("folder-/notes/Work".into(), PathBuf::from("/notes/Work"))];
        assert_eq!(
            child_directory(&root, &folders, Some("folder-/notes/Work")),
            PathBuf::from("/notes/Work")
        );
        assert_eq!(child_directory(&root, &folders, None), root);
    }

    #[test]
    fn opening_the_same_path_reuses_the_tab() {
        let first = Path::new("/notes/New Note.md");
        let second = Path::new("/notes/New Note 2.md");
        let open = [Some(first), Some(second)];
        assert_eq!(tab_index_for_path(&open, first), Some(0));
        assert_eq!(
            tab_index_for_path(&open, Path::new("/notes/other.md")),
            None
        );
    }

    #[test]
    fn keystrokes_follow_the_focused_field() {
        assert!(document_accepts_keystrokes(TextInput::Editor));
        assert!(!document_accepts_keystrokes(TextInput::Find));
        assert!(!document_accepts_keystrokes(TextInput::Replace));
        assert!(!document_accepts_keystrokes(TextInput::Rename));
        assert!(command_edits_document("undo", TextInput::Replace, true));
        assert!(command_edits_document("redo", TextInput::Find, true));
        assert!(!command_edits_document(
            "bold",
            TextInput::Replace,
            true
        ));
        assert!(command_edits_document(
            "bold",
            TextInput::Editor,
            false
        ));
    }

    #[test]
    fn focus_is_not_reapplied_every_time() {
        assert!(focus_if_changed(None, FocusSlot::Editor));
        assert!(!focus_if_changed(
            Some(FocusSlot::Editor),
            FocusSlot::Editor
        ));
        assert!(focus_if_changed(Some(FocusSlot::Editor), FocusSlot::Menu));
    }

    #[test]
    fn missing_portal_is_an_error_and_a_ready_portal_is_not() {
        assert!(file_dialog_block_reason(PortalProbe::Ready).is_none());
        let reason = file_dialog_block_reason(PortalProbe::NotOwned).unwrap();
        assert!(reason.contains("xdg-desktop-portal"));
        assert!(file_dialog_block_reason(PortalProbe::HelperMissing {
            portal_installed: false
        })
        .unwrap()
        .contains("not installed"));
    }

    #[test]
    fn save_as_file_type_follows_the_new_name() {
        assert_eq!(FileType::from_filename("Notes.md"), FileType::Markdown);
        assert_eq!(
            FileType::from_filename("Notes.markdown"),
            FileType::Markdown
        );
        assert_eq!(FileType::from_filename("notes.TXT"), FileType::PlainText);
        assert_eq!(FileType::from_filename("image.png"), FileType::PlainText);
        assert_eq!(FileType::from_filename("scan.PDF"), FileType::Pdf);
        assert_ne!(FileType::from_filename("image.png"), FileType::Markdown);
    }

    #[test]
    fn trash_of_a_dirty_tab_prompts_before_discarding() {
        assert_eq!(plan_trash([false, false]), TrashPlan::Proceed);
        assert_eq!(plan_trash([false, true]), TrashPlan::Prompt);
        assert_eq!(trash_action(TrashChoice::Cancel), TrashAction::Abort);
        assert_eq!(trash_action(TrashChoice::Save), TrashAction::SaveThenTrash);
        assert_eq!(
            trash_action(TrashChoice::Discard),
            TrashAction::DiscardThenTrash
        );
    }

    #[test]
    fn save_as_refuses_a_path_another_tab_already_has() {
        let notes = Path::new("/notes/a.md");
        let other = Path::new("/notes/b.md");
        let open = [Some(notes), Some(other)];
        assert!(path_owned_by_other_tab(&open, 0, other));
        assert!(!path_owned_by_other_tab(&open, 0, notes));
        assert!(!path_owned_by_other_tab(&open, 1, other));
        assert_eq!(
            tab_index_for_path(&open, Path::new("/notes/scan.pdf")),
            None
        );
        let with_pdf = [Some(notes), Some(Path::new("/notes/scan.pdf"))];
        assert_eq!(
            tab_index_for_path(&with_pdf, Path::new("/notes/scan.pdf")),
            Some(1)
        );
    }

    #[test]
    fn delete_prompt_arrow_navigation_and_cancel_selection() {
        let choices: Vec<(&'static str, bool)> = vec![("Cancel", false), ("Trash", true)];
        let count = choices.len();

        // Modal opens with default selected ("Trash" at index 1).
        let initial_ix = default_prompt_choice_index(&choices);
        assert_eq!(initial_ix, 1);
        assert_eq!(choices[initial_ix].0, "Trash");

        // Pressing Left arrow (step_back: true) moves to "Cancel" at index 0.
        let after_left = step_prompt_selection(initial_ix, count, true);
        assert_eq!(after_left, 0);
        assert_eq!(choices[after_left].0, "Cancel");

        // Pressing Left again wraps back to "Trash" at index 1.
        let wrap_left = step_prompt_selection(after_left, count, true);
        assert_eq!(wrap_left, 1);
        assert_eq!(choices[wrap_left].0, "Trash");

        // Pressing Right from "Cancel" (0) moves to "Trash" (1).
        let after_right = step_prompt_selection(0, count, false);
        assert_eq!(after_right, 1);
        assert_eq!(choices[after_right].0, "Trash");

        // Pressing Right from "Trash" (1) wraps to "Cancel" (0).
        let wrap_right = step_prompt_selection(1, count, false);
        assert_eq!(wrap_right, 0);
        assert_eq!(choices[wrap_right].0, "Cancel");
    }

    #[test]
    fn prompt_selection_with_zero_count_does_not_panic() {
        assert_eq!(step_prompt_selection(0, 0, true), 0);
        assert_eq!(step_prompt_selection(0, 0, false), 0);
    }
}
