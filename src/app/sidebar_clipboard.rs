//! Sidebar Copy, Paste and Duplicate (Ctrl+C / Ctrl+V / Ctrl+D), like the
//! file trees of other editors. Copied rows are kept by path in
//! `sidebar_clipboard`, apart from the system clipboard.

use super::*;

impl LiteMarkApp {
    /// The sidebar tree has the keyboard, so Ctrl+C / V / D act on its rows.
    /// Never while the editor, a text field, a menu or a dialog has it: there
    /// the keys keep their text meaning.
    pub(super) fn sidebar_owns_file_keys(&self) -> bool {
        self.sidebar_keyboard
            && self.open_menu.is_none()
            && self.modal_choice_count() == 0
            && self.rename_target.is_none()
            && self.create_draft.is_none()
    }

    /// Ctrl+C: remember the selected notes and folders for Ctrl+V.
    pub(super) fn sidebar_copy(&mut self) {
        let paths = self.selected_sidebar_paths();
        if !paths.is_empty() {
            self.sidebar_clipboard = paths;
        }
    }

    /// Ctrl+V: copy the remembered rows into the selected folder, the folder
    /// of the selected note, or the workspace root. A folder pasted onto
    /// itself lands beside it.
    pub(super) fn sidebar_paste(&mut self) {
        if self.sidebar_clipboard.is_empty() {
            return;
        }
        let dest = self.sidebar_paste_destination();
        let jobs = self
            .sidebar_clipboard
            .iter()
            .map(|src| {
                let dir = match src.parent() {
                    Some(parent) if *src == dest => parent.to_path_buf(),
                    _ => dest.clone(),
                };
                (src.clone(), dir)
            })
            .collect();
        self.copy_sidebar_rows(jobs, ("paste", "Pasted"));
    }

    /// Ctrl+D: copy each selected row next to the original.
    pub(super) fn sidebar_duplicate(&mut self) {
        let jobs = self
            .selected_sidebar_paths()
            .into_iter()
            .filter_map(|src| {
                let dir = src.parent()?.to_path_buf();
                Some((src, dir))
            })
            .collect();
        self.copy_sidebar_rows(jobs, ("duplicate", "Duplicated"));
    }

    /// Paths of the selected rows. Rows inside a selected folder are left
    /// out because the folder carries them, as when dragging.
    fn selected_sidebar_paths(&self) -> Vec<PathBuf> {
        let targets: Vec<SidebarTarget> = match self.active_multi() {
            Some(multi) => multi.items.clone(),
            None => self.selected_target.clone().into_iter().collect(),
        };
        let rows: Vec<(PathBuf, PathBuf, bool)> = targets
            .iter()
            .filter_map(|target| {
                let path = self.target_path(target)?;
                let is_folder = matches!(target, SidebarTarget::Folder(_));
                Some((path.clone(), path, is_folder))
            })
            .collect();
        if rows.len() < 2 || !rows.iter().any(|(_, _, is_folder)| *is_folder) {
            return rows.into_iter().map(|(path, _, _)| path).collect();
        }
        let visible: Vec<PathBuf> = self
            .sidebar_visible_targets()
            .iter()
            .filter_map(|target| self.target_path(target))
            .collect();
        top_level_targets(&without_partial_folders(&rows, &visible))
    }

    fn sidebar_paste_destination(&self) -> PathBuf {
        let root = self.workspace.folder_path.clone();
        match &self.selected_target {
            Some(target @ SidebarTarget::Folder(_)) => self.target_path(target).unwrap_or(root),
            Some(target @ SidebarTarget::File(_)) => self
                .target_path(target)
                .and_then(|path| path.parent().map(Path::to_path_buf))
                .unwrap_or(root),
            None => root,
        }
    }

    /// Copy each `(source, destination folder)`, then rescan, select the new
    /// rows and report any failure. `verb` is the action's name for errors.
    fn copy_sidebar_rows(&mut self, jobs: Vec<(PathBuf, PathBuf)>, verb: (&str, &str)) {
        if jobs.is_empty() {
            return;
        }
        self.error_message = None;
        let mut created = Vec::new();
        let mut errors = Vec::new();
        for (src, dir) in &jobs {
            let name = src
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| src.display().to_string());
            match paste_copy(src, dir) {
                Ok(path) => created.push(path),
                Err(_) if fs_entry_missing(src) => errors.push(format!("{name} no longer exists")),
                Err(err) => errors.push(format!("{name}: {err}")),
            }
        }
        if !created.is_empty() {
            self.rescan_workspace();
            self.select_sidebar_paths(&created);
        }
        let (verb, past) = verb;
        match (created.len(), errors.as_slice()) {
            (_, []) => {}
            (0, [only]) => self.show_file_error(format!("Couldn't {verb} {only}")),
            (done, errors) => self.show_file_error(format!(
                "{past} {done}, {} failed: {}",
                errors.len(),
                errors.last().map(String::as_str).unwrap_or_default()
            )),
        }
    }

    /// Open the folders above `paths` and select their rows.
    fn select_sidebar_paths(&mut self, paths: &[PathBuf]) {
        for folder in self.workspace.folders.iter_mut() {
            if !folder.is_open
                && paths
                    .iter()
                    .any(|path| *path != folder.path && path.starts_with(&folder.path))
            {
                folder.is_open = true;
            }
        }
        let targets: Vec<SidebarTarget> = paths
            .iter()
            .filter_map(|path| self.sidebar_target_for_path(path))
            .collect();
        let (Some(first), Some(last)) = (targets.first().cloned(), targets.last().cloned()) else {
            return;
        };
        self.multi_selection = (targets.len() > 1).then(|| MultiSelection {
            anchor: first,
            cursor: last.clone(),
            items: targets,
        });
        self.selected_target = Some(last);
        self.sidebar_keyboard = true;
        self.sidebar_scroll_to_selected = Some(ScrollStrategy::Bottom);
    }

    fn sidebar_target_for_path(&self, path: &Path) -> Option<SidebarTarget> {
        self.workspace
            .folders
            .iter()
            .find(|folder| folder.path == path)
            .map(|folder| SidebarTarget::Folder(folder.id.clone()))
            .or_else(|| {
                self.workspace
                    .files
                    .iter()
                    .find(|file| file.path == path)
                    .map(|file| SidebarTarget::File(file.id.clone()))
            })
    }
}

fn fs_entry_missing(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_err()
}
