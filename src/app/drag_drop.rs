//! Dropping files and folders onto the sidebar: move or copy.

use super::*;

impl LiteMarkApp {
    /// Handle a note or folder dragged and dropped onto a sidebar folder,
    /// or onto the notes list itself (`folder_id: None` = move to the
    /// workspace root). A plain drop moves; holding Ctrl drops a copy.
    /// Drop of one row or a whole multi-selection. Rows inside a selected
    /// folder travel with it, and a folder dropped on itself is skipped.
    pub(super) fn handle_drop_items(
        &mut self,
        targets: Vec<SidebarTarget>,
        folder_id: Option<String>,
        copy: bool,
        cx: &mut Context<Self>,
    ) {
        if targets.len() <= 1 {
            if let Some(target) = targets.into_iter().next() {
                self.handle_drop_item(target, folder_id, copy, cx);
            }
            return;
        }
        let path_of = |t: &SidebarTarget| match t {
            SidebarTarget::Folder(id) => self
                .workspace
                .folders
                .iter()
                .find(|f| &f.id == id)
                .map(|f| f.path.clone()),
            SidebarTarget::File(id) => self
                .workspace
                .files
                .iter()
                .find(|f| &f.id == id)
                .map(|f| f.path.clone()),
        };
        let rows: Vec<(SidebarTarget, PathBuf, bool)> = targets
            .into_iter()
            .filter_map(|t| {
                let path = path_of(&t)?;
                let is_folder = matches!(t, SidebarTarget::Folder(_));
                Some((t, path, is_folder))
            })
            .collect();
        // A folder with a skipped row showing stays put; only its selected rows move.
        let visible: Vec<PathBuf> = self
            .sidebar_visible_targets()
            .iter()
            .filter_map(&path_of)
            .collect();
        let rows = without_partial_folders(&rows, &visible);
        let dest_is_target = |t: &SidebarTarget| {
            matches!((t, &folder_id), (SidebarTarget::Folder(id), Some(dest)) if id == dest)
        };
        let moving: Vec<SidebarTarget> = top_level_targets(&rows)
            .into_iter()
            .filter(|t| !dest_is_target(t))
            .collect();
        if moving.is_empty() {
            return;
        }
        let dest_name = match &folder_id {
            Some(id) => self
                .workspace
                .folders
                .iter()
                .find(|f| &f.id == id)
                .map(|f| f.name.clone()),
            None => self
                .workspace
                .folder_path
                .file_name()
                .and_then(|n| n.to_str())
                .map(str::to_string),
        }
        .unwrap_or_else(|| "the workspace root".into());
        self.error_message = None;
        self.multi_selection = None;
        let (mut done, mut failed) = (0usize, 0usize);
        let mut last_error = None;
        for target in moving {
            match self.drop_one(target, folder_id.clone(), copy, false) {
                DropOutcome::Done(_) => done += 1,
                DropOutcome::Skipped => {}
                DropOutcome::Failed(err) => {
                    failed += 1;
                    last_error = Some(err);
                }
            }
        }
        let verb = if copy { "Copied" } else { "Moved" };
        match last_error {
            None if done > 0 => {
                let noun = if done == 1 { "item" } else { "items" };
                self.show_toast(format!("{verb} {done} {noun} to {dest_name}"), cx)
            }
            None => {}
            Some(err) => self.show_error(format!("{verb} {done}, {failed} failed: {err}")),
        }
    }

    pub(super) fn handle_drop_item(
        &mut self,
        target: SidebarTarget,
        folder_id: Option<String>,
        copy: bool,
        cx: &mut Context<Self>,
    ) {
        match self.drop_one(target, folder_id, copy, true) {
            DropOutcome::Done(message) => self.show_toast(message, cx),
            DropOutcome::Skipped => {}
            DropOutcome::Failed(err) => self.show_error(err),
        }
    }

    pub(super) fn drop_one(
        &mut self,
        target: SidebarTarget,
        folder_id: Option<String>,
        copy: bool,
        open_copy: bool,
    ) -> DropOutcome {
        match target {
            SidebarTarget::File(id) => self.move_or_copy_note(&id, folder_id, copy, open_copy),
            SidebarTarget::Folder(id) => self.move_or_copy_folder(&id, folder_id, copy),
        }
    }

    pub(super) fn move_or_copy_note(
        &mut self,
        file_id: &str,
        folder_id: Option<String>,
        copy: bool,
        open_copy: bool,
    ) -> DropOutcome {
        let (dest_name, dest_path) = if let Some(dest_id) = folder_id {
            let Some((name, path)) = self
                .workspace
                .folders
                .iter()
                .find(|f| f.id == dest_id)
                .map(|f| (f.name.clone(), f.path.clone()))
            else {
                return DropOutcome::Failed("That folder is no longer available.".into());
            };
            (name, path)
        } else {
            let root = self.workspace.folder_path.clone();
            let name = root
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("the workspace root")
                .to_string();
            (name, root)
        };
        let Some(file) = self
            .workspace
            .files
            .iter()
            .find(|f| f.id == file_id)
            .cloned()
        else {
            return DropOutcome::Failed("That note is no longer in the folder.".into());
        };
        if file.path.parent() == Some(dest_path.as_path()) {
            return DropOutcome::Skipped;
        }
        if copy {
            let result = read_note(&file.path).and_then(|content| {
                copy_note_into_workspace(&file.path, &dest_path, &content)
                    .map(|new_path| (new_path, content))
                    .map_err(|err| err.to_string())
            });
            match result {
                Ok((new_path, copied)) => {
                    self.rescan_workspace();
                    if open_copy {
                        let new_file_id = self
                            .workspace
                            .files
                            .iter()
                            .find(|f| f.path == new_path)
                            .map(|f| f.id.clone());
                        let display = new_path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or(file.name.as_str())
                            .to_string();
                        self.open_loaded_tab(display, copied, Some(new_path), new_file_id);
                    }
                    DropOutcome::Done(format!("Copied {} to {dest_name}", file.name))
                }
                Err(err) => DropOutcome::Failed(format!("Couldn't copy note: {err}")),
            }
        } else {
            match move_note(&file.path, &dest_path) {
                Ok(new_path) if new_path != file.path => {
                    self.retarget_open_tabs(&file.path, &new_path);
                    self.rescan_workspace();
                    DropOutcome::Done(format!("Moved {} to {dest_name}", file.name))
                }
                Ok(_) => DropOutcome::Skipped,
                Err(err) => DropOutcome::Failed(format!("Couldn't move note: {err}")),
            }
        }
    }

    pub(super) fn move_or_copy_folder(
        &mut self,
        folder_id: &str,
        dest_folder_id: Option<String>,
        copy: bool,
    ) -> DropOutcome {
        let Some((name, src)) = self
            .workspace
            .folders
            .iter()
            .find(|f| f.id == folder_id)
            .map(|f| (f.name.clone(), f.path.clone()))
        else {
            return DropOutcome::Failed("That folder is no longer available.".into());
        };
        let (dest_name, dest_path) = if let Some(dest_id) = dest_folder_id {
            if dest_id == folder_id {
                return DropOutcome::Skipped;
            }
            let Some((dname, dpath)) = self
                .workspace
                .folders
                .iter()
                .find(|f| f.id == dest_id)
                .map(|f| (f.name.clone(), f.path.clone()))
            else {
                return DropOutcome::Failed("That folder is no longer available.".into());
            };
            (dname, dpath)
        } else {
            let root = self.workspace.folder_path.clone();
            let dname = root
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("the workspace root")
                .to_string();
            (dname, root)
        };
        // Never move/copy a folder into its own descendant (or itself) —
        // that would create a cycle on disk.
        if dest_path.starts_with(&src) {
            return DropOutcome::Failed("Can't move a folder into itself.".into());
        }
        if src.parent() == Some(dest_path.as_path()) {
            return DropOutcome::Skipped;
        }
        if copy {
            match copy_folder(&src, &dest_path) {
                Ok(_) => {
                    self.rescan_workspace();
                    DropOutcome::Done(format!("Copied {name} to {dest_name}"))
                }
                Err(err) => DropOutcome::Failed(format!("Couldn't copy folder: {err}")),
            }
        } else {
            match move_folder(&src, &dest_path) {
                Ok(new_path) if new_path != src => {
                    self.retarget_open_tabs(&src, &new_path);
                    self.rescan_workspace();
                    DropOutcome::Done(format!("Moved {name} to {dest_name}"))
                }
                Ok(_) => DropOutcome::Skipped,
                Err(err) => DropOutcome::Failed(format!("Couldn't move folder: {err}")),
            }
        }
    }
}
