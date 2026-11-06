//! Workspace switching, rename, trash and the save-before-close flow.

use super::*;

impl LiteMarkApp {
    /// Route a picked workspace directory: when the current note lives on
    /// disk inside the current workspace, ask whether to copy it into the
    /// new one before switching; otherwise switch right away.
    pub(super) fn prepare_workspace_switch(&mut self, new_dir: PathBuf) {
        if new_dir == self.workspace.folder_path {
            self.rescan_workspace();
            return;
        }
        if self.migratable_note_path().is_some() {
            self.show_prompt(Prompt::SwitchWorkspace { new_dir });
        } else {
            self.apply_workspace_switch(new_dir, None);
        }
    }

    /// Path of the active note if it is a text note stored inside the
    /// current workspace (the only kind that can be copied to a new one).
    pub(super) fn migratable_note_path(&self) -> Option<PathBuf> {
        let tab = self.active_tab()?;
        if !matches!(
            tab.file_type,
            FileType::Markdown | FileType::PlainText | FileType::Code
        ) {
            return None;
        }
        tab.file_path
            .clone()
            .filter(|path| path.starts_with(&self.workspace.folder_path))
    }

    /// Switch the workspace to `new_dir`, optionally copying the current
    /// note (including unsaved edits) into it before the switch. The
    /// original file on disk is never removed.
    pub(super) fn apply_workspace_switch(&mut self, new_dir: PathBuf, migrate_from: Option<PathBuf>) {
        let mut migrated = None;
        if let Some(old_path) = migrate_from {
            let content = self
                .active_tab()
                .map(|tab| tab.content.clone())
                .unwrap_or_default();
            match copy_note_into_workspace(&old_path, &new_dir, &content) {
                Ok(new_path) => migrated = Some(new_path),
                Err(err) => self.show_error(format!("Couldn't copy note: {err}")),
            }
        }
        let left = self.workspace.folder_path.clone();
        self.workspace = scan_workspace_dir(&new_dir, self.settings.show_code_files);
        self.settings.remember_folders(&left, &new_dir);
        self.settings.save();
        self.workspace_menu_open = false;
        if let Some(new_path) = migrated {
            if let Some(tab) = self.active_tab_mut() {
                if let Some(name) = new_path.file_name().and_then(|n| n.to_str()) {
                    tab.title = name.to_string();
                }
                tab.file_path = Some(new_path);
                tab.is_modified = false;
            }
        }
        // Selection ids are workspace-scoped and don't survive a switch.
        self.selected_target = None;
        self.sidebar_keyboard = false;
        self.refresh_workspace_ids();
    }

    pub(super) fn retarget_open_tabs(&mut self, old: &Path, new_path: &Path) {
        let mut type_changed = false;
        for tab in &mut self.tabs {
            let Some(path) = tab.file_path.clone() else {
                continue;
            };
            if path == old {
                tab.file_path = Some(new_path.to_path_buf());
                if let Some(name) = new_path.file_name().and_then(|name| name.to_str()) {
                    tab.title = name.to_string();
                    type_changed |= tab.set_file_type(FileType::from_filename(&tab.title));
                }
            } else if let Ok(rest) = path.strip_prefix(old) {
                tab.file_path = Some(new_path.join(rest));
            }
        }
        if type_changed {
            self.refresh_find();
        }
    }

    /// Trash the sidebar selection (or the active note), asking first.
    pub(super) fn delete_selected(&mut self, cx: &mut Context<Self>) {
        // Only act in a normal window state — never while a menu,
        // modal or naming editor is open.
        if self.open_menu.is_some()
            || self.modal_choice_count() > 0
            || self.rename_target.is_some()
            || self.create_draft.is_some()
        {
            return;
        }
        if let Some(items) = self
            .active_multi()
            .filter(|_| self.sidebar_keyboard)
            .map(|m| m.items.clone())
        {
            self.request_trash_many(items);
            cx.notify();
            return;
        }
        // Target whatever is selected in the sidebar (folder or
        // note); fall back to the active tab's note.
        let target = self.selected_target.clone().or_else(|| {
            self.active_tab()
                .and_then(|tab| tab.workspace_file_id.clone())
                .map(SidebarTarget::File)
        });
        match target {
            Some(SidebarTarget::Folder(id)) => {
                self.request_trash_folder(id);
                cx.notify();
            }
            Some(SidebarTarget::File(id)) => {
                self.request_trash_file(id);
                cx.notify();
            }
            None => {
                let Some(tab) = self.active_tab() else {
                    return;
                };
                if tab.file_path.is_none() {
                    self.show_error(
                        "This note isn't saved yet — press Ctrl+S to name and save it first.",
                    );
                } else {
                    self.show_error(
                        "This file isn't in the workspace folder, so it can't be trashed here.",
                    );
                }
            }
        }
    }

    pub(super) fn request_trash_file(&mut self, id: String) {
        let Some(file) = self
            .workspace
            .files
            .iter()
            .find(|file| file.id == id)
            .cloned()
        else {
            self.show_error("That note is no longer in the folder.");
            return;
        };
        let modified = self
            .tabs
            .iter()
            .filter(|tab| tab.file_path.as_ref() == Some(&file.path))
            .map(|tab| tab.is_modified);
        self.show_prompt(match plan_trash(modified) {
            TrashPlan::Prompt => Prompt::TrashDirtyFile { id },
            TrashPlan::Proceed => Prompt::DeleteFile { id },
        });
    }

    pub(super) fn request_trash_folder(&mut self, id: String) {
        let Some(folder) = self
            .workspace
            .folders
            .iter()
            .find(|folder| folder.id == id)
            .cloned()
        else {
            self.show_error("That folder is no longer available.");
            return;
        };
        let prefix = folder.path.clone();
        let modified = self
            .tabs
            .iter()
            .filter(|tab| {
                tab.file_path
                    .as_ref()
                    .is_some_and(|path| path.starts_with(&prefix))
            })
            .map(|tab| tab.is_modified);
        self.show_prompt(match plan_trash(modified) {
            TrashPlan::Prompt => Prompt::TrashDirtyFolder { id },
            TrashPlan::Proceed => Prompt::DeleteFolder { id },
        });
    }

    pub(super) fn dirty_tab_ids_for_path(&self, path: &Path) -> Vec<String> {
        self.tabs
            .iter()
            .filter(|tab| tab.is_modified && tab.file_path.as_deref() == Some(path))
            .map(|tab| tab.id.clone())
            .collect()
    }

    pub(super) fn dirty_tab_ids_under(&self, prefix: &Path) -> Vec<String> {
        self.tabs
            .iter()
            .filter(|tab| {
                tab.is_modified
                    && tab
                        .file_path
                        .as_ref()
                        .is_some_and(|path| path.starts_with(prefix))
            })
            .map(|tab| tab.id.clone())
            .collect()
    }

    pub(super) fn save_ids(&mut self, ids: &[String]) -> Result<(), String> {
        for id in ids {
            self.save_tab(id)?;
        }
        Ok(())
    }

    pub(super) fn begin_rename(&mut self, target: SidebarTarget, cx: &mut Context<Self>) {
        let name = match &target {
            SidebarTarget::File(id) => self
                .workspace
                .files
                .iter()
                .find(|file| &file.id == id)
                .map(|file| file.name.clone()),
            SidebarTarget::Folder(id) => self
                .workspace
                .folders
                .iter()
                .find(|folder| &folder.id == id)
                .map(|folder| folder.name.clone()),
        };
        let Some(name) = name else {
            self.show_error("That item is no longer in the folder.");
            return;
        };
        self.sidebar_menu = None;
        self.rename_target = Some(target);
        self.rename_value = name.clone();
        self.pending_rename_focus = true;
        self.rename_field.update(cx, |field, _cx| {
            field.set_text(name);
        });
    }

    pub(super) fn cancel_rename(&mut self, cx: &mut Context<Self>) {
        self.rename_target = None;
        self.save_as_tab = None;
        self.pending_after_save = None;
        self.rename_value.clear();
        self.field_owns_keys = false;
        self.applied_focus = None;
        cx.notify();
    }

    /// Open the naming modal for saving an untitled tab (quick note).
    /// Saving happens only after the user confirms a name with Enter.
    pub(super) fn begin_save_naming(&mut self, tab_id: &str, cx: &mut Context<Self>) {
        let Some(title) = self
            .tabs
            .iter()
            .find(|t| t.id == tab_id)
            .map(|t| t.title.clone())
        else {
            return;
        };
        self.sidebar_menu = None;
        self.save_as_tab = Some(tab_id.to_string());
        self.pending_rename_focus = true;
        self.rename_value = title.clone();
        self.rename_field.update(cx, |field, _cx| {
            field.set_text(title);
        });
        cx.notify();
    }

    /// Commit the save-naming modal: set the chosen title on the untitled
    /// tab and save it into the workspace root (plan_save appends `.md`
    /// and picks a free name). Re-opens on error; resumes a pending close
    /// flow on success.
    pub(super) fn commit_save_naming(
        &mut self,
        name_override: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let Some(tab_id) = self.save_as_tab.clone() else {
            return;
        };
        let raw = name_override
            .unwrap_or_else(|| self.rename_value.clone())
            .trim()
            .to_string();
        if raw.is_empty() {
            self.show_error("Enter a name.");
            return;
        }
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
            tab.title = raw;
        }
        match self.save_tab(&tab_id) {
            Ok(title) => {
                self.save_as_tab = None;
                self.rename_value.clear();
                self.field_owns_keys = false;
                self.applied_focus = None;
                self.show_toast(format!("Saved {title}"), cx);
                match self.pending_after_save.take() {
                    Some(PendingAfterSave::CloseTab(id)) => self.finish_close_saved(&id),
                    Some(PendingAfterSave::CloseWindow) => self.resume_close_window(cx),
                    None => {}
                }
            }
            Err(err) => self.show_error(err),
        }
        cx.notify();
    }

    pub(super) fn first_unnamed_dirty_tab(&self) -> Option<String> {
        self.tabs
            .iter()
            .find(|tab| tab.is_modified && tab.file_path.is_none())
            .map(|tab| tab.id.clone())
    }

    /// Continue the close-window flow after a tab was saved with a name:
    /// name the next untitled dirty tab, or finish the save-all.
    pub(super) fn resume_close_window(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.first_unnamed_dirty_tab() {
            self.pending_after_save = Some(PendingAfterSave::CloseWindow);
            self.begin_save_naming(&id, cx);
            return;
        }
        match self.save_all_dirty() {
            Ok(()) => self.allow_window_close = true,
            Err(err) => {
                self.show_error(err);
                self.show_prompt(Prompt::CloseWindow);
            }
        }
    }

    pub(super) fn commit_rename(&mut self, name_override: Option<String>, _cx: &mut Context<Self>) {
        let Some(target) = self.rename_target.take() else {
            return;
        };
        let name = name_override
            .unwrap_or_else(|| self.rename_value.clone())
            .trim()
            .to_string();
        if name.is_empty() {
            self.show_error("Enter a name.");
            self.rename_target = Some(target);
            return;
        }
        let renamed = match &target {
            SidebarTarget::File(id) => self
                .workspace
                .files
                .iter()
                .find(|file| &file.id == id)
                .map(|file| file.path.clone()),
            SidebarTarget::Folder(id) => self
                .workspace
                .folders
                .iter()
                .find(|folder| &folder.id == id)
                .map(|folder| folder.path.clone()),
        };
        let Some(old) = renamed else {
            self.show_error("That item is no longer in the folder.");
            return;
        };
        match safe_rename(&old, &name) {
            Ok(new_path) => {
                self.rename_value.clear();
                self.retarget_open_tabs(&old, &new_path);
                self.rescan_workspace();
                // Item ids are path-based: remap the sidebar selection so the
                // renamed item stays highlighted.
                let (kind, old_id) = match &target {
                    SidebarTarget::File(_) => ("file", format!("file-{}", old.display())),
                    SidebarTarget::Folder(_) => {
                        ("folder", format!("folder-{}", old.display()))
                    }
                };
                let new_id = format!("{kind}-{}", new_path.display());
                match &self.selected_target {
                    Some(SidebarTarget::File(id)) if *id == old_id => {
                        self.selected_target = Some(SidebarTarget::File(new_id));
                    }
                    Some(SidebarTarget::Folder(id)) if *id == old_id => {
                        self.selected_target = Some(SidebarTarget::Folder(new_id));
                    }
                    _ => {}
                }
            }
            Err(err) => {
                self.show_error(format!("Couldn't rename: {err}"));
                self.rename_target = Some(target);
            }
        }
    }

    /// Optimistically close the tabs whose file path matches `pred` and fix
    /// up the active-tab selection. Returns enough state to restore them if
    /// the operation that motivated the close fails.
    pub(super) fn optimistic_close_tabs(&mut self, pred: impl Fn(&Path) -> bool) -> OptimisticClose {
        let prev_active = self.active_tab_id.clone();
        let mut removed: Vec<(usize, DocumentTab)> = Vec::new();
        let mut original_ix = 0usize;
        self.tabs.retain(|tab| {
            let keep = !tab
                .file_path
                .as_ref()
                .is_some_and(|path| pred(path));
            if !keep {
                removed.push((original_ix, tab.clone()));
            }
            original_ix += 1;
            keep
        });

        // Keep a valid selection: the tab that took the first closed tab's
        // position, the last remaining tab, or a fresh placeholder when the
        // last tab was closed (same as discard_tab's only-tab behavior).
        let placeholder_id = if !self.tabs.iter().any(|t| t.id == self.active_tab_id) {
            if self.tabs.is_empty() {
                let new_id = self.tab_ids.alloc();
                let number = new_id.trim_start_matches("tab-");
                let title = format!("Untitled-{number}.md");
                self.tabs.push(DocumentTab::new(
                    new_id.clone(),
                    title,
                    String::new(),
                    None,
                    None,
                ));
                Some(new_id)
            } else {
                let pos = removed.first().map(|(ix, _)| *ix).unwrap_or(0);
                if let Some(tab) = self.tabs.get(pos.min(self.tabs.len() - 1)) {
                    self.active_tab_id = tab.id.clone();
                }
                None
            }
        } else {
            None
        };
        self.refresh_find();
        OptimisticClose {
            tabs: removed,
            prev_active,
            placeholder_id,
            prev_selected: self.selected_target.clone(),
        }
    }

    /// Undo an optimistic tab closure (the trash failed): put the tabs back
    /// in their original positions and restore the previous selection.
    pub(super) fn restore_optimistic_close(
        &mut self,
        closed: OptimisticClose,
        restore_selection: bool,
        cx: &mut Context<Self>,
    ) {
        let OptimisticClose {
            tabs,
            prev_active,
            placeholder_id,
            prev_selected,
        } = closed;
        if let Some(placeholder_id) = placeholder_id {
            // Only drop the placeholder while the user hasn't typed into it
            // in the meantime; a used placeholder is real work, keep it open.
            self.tabs
                .retain(|tab| !(tab.id == placeholder_id && tab.content.is_empty() && !tab.is_modified));
        }
        for (ix, tab) in tabs {
            let at = ix.min(self.tabs.len());
            self.tabs.insert(at, tab);
        }
        self.active_tab_id = prev_active;
        if restore_selection {
            self.selected_target = prev_selected;
        }
        self.refresh_find();
        cx.notify();
    }

    pub(super) fn trash_file(&mut self, id: &str, cx: &mut Context<Self>) {
        self.trash_file_in(id, cx, None);
    }

    pub(super) fn trash_file_in(&mut self, id: &str, cx: &mut Context<Self>, bulk: Option<BulkReport>) {
        let Some(file) = self
            .workspace
            .files
            .iter()
            .find(|file| file.id == id)
            .cloned()
        else {
            self.report_missing("That note is no longer in the folder.", bulk);
            return;
        };
        // Optimistic removal: the note vanishes and its tabs close the same
        // frame, while the (slow, portal-backed) trash runs in the
        // background. On failure the closed tabs are restored and the
        // rescan brings the note back — trashing is a rename, so a failed
        // trash loses nothing.
        self.workspace.files.retain(|f| f.path != file.path);
        let closed = self.optimistic_close_tabs(|path| path == file.path.as_path());
        // The selection pointed at the removed note — clear it (restored on
        // failure together with the tabs).
        if matches!(&self.selected_target, Some(SidebarTarget::File(fid)) if self.workspace.files.iter().all(|f| f.id != *fid))
        {
            self.selected_target = None;
        }
        cx.notify();
        let path = file.path.clone();
        let task = cx
            .background_executor()
            .spawn(async move { move_to_trash(&path) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| this.finish_trash(result, closed, bulk, cx));
        })
        .detach();
    }

    pub(super) fn trash_folder(&mut self, id: &str, cx: &mut Context<Self>) {
        self.trash_folder_in(id, cx, None);
    }

    /// Settle one background trash: rescan, undo the optimistic close on
    /// failure, and report. A bulk trash shows a single message at the end.
    pub(super) fn finish_trash(
        &mut self,
        result: Result<(), String>,
        closed: OptimisticClose,
        bulk: Option<BulkReport>,
        cx: &mut Context<Self>,
    ) {
        let failed = result.clone().err();
        if failed.is_some() {
            // In a bulk trash the selection is put back once, below, because
            // the rows finish in no set order.
            self.restore_optimistic_close(closed, bulk.is_none(), cx);
        }
        self.rescan_workspace();
        match bulk {
            Some(bulk) => self.record_bulk(&bulk, result, cx),
            None => {
                if let Some(err) = failed {
                    self.show_error(err);
                }
            }
        }
    }

    /// Count one row of a bulk trash. After the last row, show one message
    /// and, if anything failed, restore the pre-trash selection (when that
    /// row is still in the tree).
    pub(super) fn record_bulk(&mut self, bulk: &BulkTrash, result: Result<(), String>, cx: &mut Context<Self>) {
        let message = bulk.report.borrow_mut().record(result);
        let (finished, failed) = {
            let report = bulk.report.borrow();
            (report.is_finished(), report.has_failures())
        };
        if finished && failed {
            let still_there = match &bulk.prev_selected {
                Some(SidebarTarget::File(id)) => self.workspace.files.iter().any(|f| &f.id == id),
                Some(SidebarTarget::Folder(id)) => {
                    self.workspace.folders.iter().any(|f| &f.id == id)
                }
                None => false,
            };
            if still_there {
                self.selected_target = bulk.prev_selected.clone();
            }
            cx.notify();
        }
        if let Some(message) = message {
            self.show_error(message);
        }
    }

    /// A row vanished (say, deleted outside the app) before it could be
    /// trashed. In a bulk trash it still counts toward the total, so the
    /// final message is not lost.
    pub(super) fn report_missing(&mut self, message: &str, bulk: Option<BulkReport>) {
        match bulk {
            Some(bulk) => {
                if let Some(message) = bulk.report.borrow_mut().record(Err(message.into())) {
                    self.show_error(message);
                }
            }
            None => self.show_error(message),
        }
    }

    pub(super) fn trash_folder_in(&mut self, id: &str, cx: &mut Context<Self>, bulk: Option<BulkReport>) {
        let Some(folder) = self
            .workspace
            .folders
            .iter()
            .find(|folder| folder.id == id)
            .cloned()
        else {
            self.report_missing("That folder is no longer available.", bulk);
            return;
        };
        // Same optimistic removal as trash_file, for the whole subtree.
        let prefix = folder.path.clone();
        self.workspace
            .folders
            .retain(|f| !f.path.starts_with(&prefix));
        self.workspace
            .files
            .retain(|f| !f.path.starts_with(&prefix));
        let closed = self.optimistic_close_tabs(|path| path.starts_with(&prefix));
        // Clear the selection if it pointed into the removed subtree.
        let selection_gone = match &self.selected_target {
            Some(SidebarTarget::File(id)) => self.workspace.files.iter().all(|f| f.id != *id),
            Some(SidebarTarget::Folder(id)) => self.workspace.folders.iter().all(|f| f.id != *id),
            None => false,
        };
        if selection_gone {
            self.selected_target = None;
        }
        cx.notify();
        let task = cx
            .background_executor()
            .spawn(async move { move_to_trash(&prefix) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| this.finish_trash(result, closed, bulk, cx));
        })
        .detach();
    }

    pub(super) fn save_all_dirty(&mut self) -> Result<(), String> {
        let ids: Vec<String> = self
            .tabs
            .iter()
            .filter(|tab| tab.is_modified)
            .map(|tab| tab.id.clone())
            .collect();
        for id in ids {
            self.save_tab(&id)?;
        }
        Ok(())
    }
}
