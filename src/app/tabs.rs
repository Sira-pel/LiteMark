//! Saving, opening and closing document tabs.

use super::*;

/// How long after a tab switch the tab list is written to settings.json.
const TAB_SETTINGS_SAVE_DELAY_MS: u64 = 1000;

impl LiteMarkApp {
    /// Save a tab. Refuses, with an error, when another program changed or
    /// deleted the file since the tab last read or wrote it; `handle_save`
    /// checks first and asks instead.
    pub(super) fn save_tab(&mut self, id: &str) -> Result<String, String> {
        self.save_tab_with(id, false)
    }

    /// Save a tab even if the file changed on disk (the user chose to keep
    /// their version).
    pub(super) fn overwrite_tab(&mut self, id: &str) -> Result<String, String> {
        self.save_tab_with(id, true)
    }

    /// How the file behind tab `id` compares with what the tab last saw.
    pub(super) fn disk_change_for(&self, id: &str) -> DiskChange {
        let Some(tab) = self.tabs.iter().find(|tab| tab.id == id) else {
            return DiskChange::Unchanged;
        };
        match (&tab.file_path, &tab.disk_version) {
            (Some(path), Some(seen)) => disk_change(path, seen),
            _ => DiskChange::Unchanged,
        }
    }

    /// Remember the file as it is now, holding the tab's current text.
    pub(super) fn record_disk_version(&mut self, id: &str) {
        if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == id) {
            tab.disk_version = tab
                .file_path
                .as_deref()
                .and_then(|path| disk_version(path, &tab.file_text_ref()));
        }
    }

    /// Replace the tab's text with the file on disk (the user chose to drop
    /// their version).
    pub(super) fn reload_tab(&mut self, id: &str) -> Result<(), String> {
        let path = self
            .tabs
            .iter()
            .find(|tab| tab.id == id)
            .and_then(|tab| tab.file_path.clone())
            .ok_or_else(|| "This note has no file to reload.".to_string())?;
        let content = read_note(&path)?;
        if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == id) {
            tab.reload(content);
        }
        self.record_disk_version(id);
        self.refresh_find();
        Ok(())
    }

    fn save_tab_with(&mut self, id: &str, overwrite: bool) -> Result<String, String> {
        if !overwrite {
            let change = self.disk_change_for(id);
            if change != DiskChange::Unchanged {
                let name = self
                    .tabs
                    .iter()
                    .find(|tab| tab.id == id)
                    .map(|tab| tab.title.clone())
                    .unwrap_or_default();
                let what = if change == DiskChange::Deleted {
                    "deleted"
                } else {
                    "changed"
                };
                return Err(format!(
                    "Didn't save {name}: another program {what} it on disk. \
                     Open the note and save it to choose which version to keep."
                ));
            }
        }
        let tab = self
            .tabs
            .iter()
            .find(|tab| tab.id == id)
            .ok_or_else(|| "No document to save.".to_string())?;
        let folder = tab
            .file_path
            .as_ref()
            .and_then(|path| path.parent().map(|parent| parent.to_path_buf()))
            .unwrap_or_else(|| self.workspace.folder_path.clone());
        let planned = plan_save(
            tab.file_type,
            tab.file_path.as_deref(),
            &tab.title,
            &folder,
            |name| folder.join(name).exists(),
        )?;
        let content = tab.file_text();
        let previous_path = tab.file_path.clone();
        let saved_path = if planned.path.exists() {
            atomic_save(&planned.path, &content)
                .map_err(|err| format!("Couldn't save {}: {err}", planned.path.display()))?;
            planned.path
        } else {
            let parent = planned
                .path
                .parent()
                .map(|parent| parent.to_path_buf())
                .unwrap_or_else(|| folder.clone());
            let name = planned
                .path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| planned.title.clone());
            atomic_save_new(&parent, &name, &content)
                .map_err(|err| format!("Couldn't save {}/{name}: {err}", parent.display()))?
        };
        let saved_title = saved_path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or(planned.title);
        if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == id) {
            tab.file_path = Some(saved_path.clone());
            tab.title = saved_title;
            let type_changed = tab.set_file_type(FileType::from_filename(&tab.title));
            tab.is_modified = false;
            if type_changed {
                self.refresh_find();
            }
        }
        self.record_disk_version(id);
        // Re-saving a file the sidebar already lists changes nothing in the
        // tree, so update its timestamp instead of rescanning the whole
        // workspace on the UI thread on every Ctrl+S.
        let listed = (previous_path.as_ref() == Some(&saved_path))
            .then(|| {
                self.workspace
                    .files
                    .iter_mut()
                    .find(|file| file.path == saved_path)
            })
            .flatten();
        if let Some(file) = listed {
            file.updated_at = modified_secs(&saved_path).unwrap_or(file.updated_at);
        } else {
            self.rescan_workspace();
        }
        if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == id) {
            let path = tab.file_path.clone();
            tab.workspace_file_id = path.and_then(|path| {
                self.workspace
                    .files
                    .iter()
                    .find(|file| file.path == path)
                    .map(|file| file.id.clone())
            });
            return Ok(tab.title.clone());
        }
        Ok(planned_title_fallback())
    }

    pub(super) fn open_loaded_tab(
        &mut self,
        name: String,
        content: String,
        path: Option<PathBuf>,
        workspace_file_id: Option<String>,
    ) {
        // Opening a note (sidebar click, creation, file open) selects it in
        // the sidebar and hands keyboard focus back to the editor.
        self.multi_selection = None;
        self.selected_target = workspace_file_id.clone().map(SidebarTarget::File);
        self.sidebar_keyboard = false;
        if let Some(path) = path.as_ref() {
            let paths: Vec<Option<&Path>> = self
                .tabs
                .iter()
                .map(|tab| tab.file_path.as_deref())
                .collect();
            if let Some(index) = tab_index_for_path(&paths, path) {
                let id = self.tabs[index].id.clone();
                self.activate_tab(id.clone());
                // `content` was just read from disk. A tab without edits
                // shows it if it is newer; a tab with edits keeps them, and
                // its next save asks because the file changed.
                let tab = &mut self.tabs[index];
                if !tab.is_modified {
                    // `content` is the raw file, BOM included.
                    if tab.file_text_ref() != content.as_str() {
                        tab.reload(content);
                    }
                    self.record_disk_version(&id);
                }
                self.refresh_find();
                return;
            }
        }
        let id = self.tab_ids.alloc();
        self.tabs.push(DocumentTab::new(
            id.clone(),
            name,
            content,
            path,
            workspace_file_id,
        ));
        self.record_disk_version(&id);
        self.activate_tab(id);
        self.refresh_find();
    }

    pub(super) fn remove_tab(&mut self, id: &str, next_active: Option<String>) {
        let pos = self.tabs.iter().position(|tab| tab.id == id);
        let removing_active = self.active_tab_id == id;
        if removing_active {
            self.reset_editor_transient_state();
        }
        self.tabs.retain(|tab| tab.id != id);
        let target = next_active
            .filter(|next| self.tabs.iter().any(|tab| &tab.id == next))
            .or_else(|| {
                pos.and_then(|pos| self.tabs.get(pos).or_else(|| self.tabs.last()))
                    .map(|tab| tab.id.clone())
            });
        if let Some(next) = target {
            if removing_active {
                self.active_tab_id = next;
                if self.effective_view_mode() == ViewMode::Wysiwyg {
                    if let Some(tab) = self.active_tab_mut() {
                        tab.snap_wysiwyg_table_delimiter();
                    }
                }
            } else {
                self.activate_tab(next);
            }
        }
        self.sync_selection_to_active_tab();
        self.refresh_find();
    }

    pub(super) fn discard_tab(&mut self, id: &str) {
        let only = self.tabs.len() <= 1 && self.tabs.iter().any(|tab| tab.id == id);
        if only {
            self.reset_editor_transient_state();
            let new_id = self.tab_ids.alloc();
            let number = new_id.trim_start_matches("tab-");
            let title = format!("Untitled-{number}.md");
            self.tabs.clear();
            self.tabs.push(DocumentTab::new(
                new_id.clone(),
                title,
                String::new(),
                None,
                None,
            ));
            self.active_tab_id = new_id;
            self.sync_selection_to_active_tab();
            self.refresh_find();
            return;
        }
        let pos = self.tabs.iter().position(|tab| tab.id == id);
        let next = pos.and_then(|pos| {
            if pos + 1 < self.tabs.len() {
                Some(self.tabs[pos + 1].id.clone())
            } else if pos > 0 {
                Some(self.tabs[pos - 1].id.clone())
            } else {
                None
            }
        });
        self.remove_tab(id, next);
    }

    pub(super) fn finish_close_saved(&mut self, id: &str) {
        let snaps: Vec<TabSnap<'_>> = self
            .tabs
            .iter()
            .map(|tab| TabSnap {
                id: tab.id.as_str(),
                modified: tab.is_modified,
            })
            .collect();
        if let ClosePlan::Close { next_active } = plan_close(&snaps, id) {
            self.remove_tab(id, next_active);
        }
    }

    /// Keep the saved tab list and the recent notes in step with the tabs.
    /// Called every frame. A change (open files, active one, recent notes;
    /// never an edit) only marks the settings dirty: the file is written by
    /// [`Self::schedule_tab_settings_save`] or [`Self::flush_tab_settings`].
    /// Returns whether anything changed.
    pub(super) fn sync_tab_settings(&mut self) -> bool {
        let open: Vec<PathBuf> = self
            .tabs
            .iter()
            .filter(|tab| tab.file_type != FileType::Pdf)
            .filter_map(|tab| tab.file_path.clone())
            .collect();
        let active_path = self.active_tab().and_then(|tab| tab.file_path.clone());
        let active = active_path.clone().filter(|path| open.contains(path));
        let mut changed = false;
        if open != self.settings.open_tabs {
            self.settings.open_tabs = open;
            changed = true;
        }
        if active != self.settings.active_tab {
            self.settings.active_tab = active;
            changed = true;
        }
        if active_path != self.recent_seen {
            if let Some(path) = &active_path {
                if push_recent(&mut self.settings.recent_notes, path) {
                    self.recent_menu_cache = None;
                    changed = true;
                }
            }
            self.recent_seen = active_path;
        }
        if changed {
            self.tab_settings_dirty = true;
        }
        changed
    }

    /// Write dirty tab settings about a second from now, once, however many
    /// tab switches happen in between.
    pub(super) fn schedule_tab_settings_save(&mut self, cx: &mut Context<Self>) {
        if !self.tab_settings_dirty || self.tab_settings_save_pending {
            return;
        }
        self.tab_settings_save_pending = true;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(TAB_SETTINGS_SAVE_DELAY_MS))
                .await;
            this.update(cx, |this, _cx| {
                this.tab_settings_save_pending = false;
                this.flush_tab_settings();
            })
            .ok();
        })
        .detach();
    }

    /// Write dirty tab settings now (the window is closing).
    pub(super) fn flush_tab_settings(&mut self) {
        self.sync_tab_settings();
        if self.tab_settings_dirty {
            self.save_settings();
        }
    }

    /// Reopen the notes that were open when the app last closed, skipping
    /// any that are gone or unreadable. With `keep_current` (a file passed on
    /// the command line) the current tab stays last and active; otherwise the
    /// welcome tab is replaced and the saved active note comes back active.
    /// Returns how many notes were reopened; with none, nothing changes.
    pub(super) fn restore_saved_tabs(&mut self, keep_current: bool) -> usize {
        if !self.settings.reopen_tabs_on_startup {
            return 0;
        }
        let mut restored: Vec<DocumentTab> = Vec::new();
        for path in self.settings.open_tabs.clone() {
            let already = |tabs: &[DocumentTab]| {
                tabs.iter()
                    .any(|tab| tab.file_path.as_deref() == Some(path.as_path()))
            };
            if is_pdf_path(&path) || already(&restored) || (keep_current && already(&self.tabs)) {
                continue;
            }
            let Ok(content) = read_note(&path) else {
                continue;
            };
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Untitled.md".into());
            let mut tab = DocumentTab::new(self.tab_ids.alloc(), name, content, None, None);
            tab.disk_version = disk_version(&path, &tab.file_text_ref());
            tab.file_path = Some(path);
            restored.push(tab);
        }
        let count = restored.len();
        if count == 0 {
            return 0;
        }
        if keep_current {
            restored.append(&mut self.tabs);
        } else {
            let active = self.settings.active_tab.as_deref();
            self.active_tab_id = restored
                .iter()
                .find(|tab| active.is_some() && tab.file_path.as_deref() == active)
                .unwrap_or(&restored[0])
                .id
                .clone();
        }
        self.tabs = restored;
        self.refresh_workspace_ids();
        self.multi_selection = None;
        self.sync_selection_to_active_tab();
        self.refresh_find();
        count
    }

    /// Open a note from the recent list. A note that is gone is dropped
    /// from the list.
    pub(super) fn open_recent(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if !path.is_file() {
            if retain_existing(&mut self.settings.recent_notes, |p| p != path) {
                self.save_settings();
            }
            self.show_error(format!("{} no longer exists.", path.display()));
            return;
        }
        if is_pdf_path(&path) {
            self.open_path(path, cx);
            return;
        }
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled.md".into());
        match read_note(&path) {
            Ok(content) => {
                let file_id = self
                    .workspace
                    .files
                    .iter()
                    .find(|file| file.path == path)
                    .map(|file| file.id.clone());
                self.open_loaded_tab(name, content, Some(path), file_id);
            }
            Err(err) => self.show_error(err),
        }
    }

    /// Recent notes that still exist, for the File menu. Missing ones are
    /// dropped from the saved list too. Checked once per menu opening.
    pub(super) fn recent_notes_for_menu(&mut self) -> Vec<PathBuf> {
        if let Some(cached) = &self.recent_menu_cache {
            return cached.clone();
        }
        if retain_existing(&mut self.settings.recent_notes, |p| p.is_file()) {
            self.save_settings();
        }
        let list = self.settings.recent_notes.clone();
        self.recent_menu_cache = Some(list.clone());
        list
    }

    pub(super) fn clear_recent_notes(&mut self) {
        self.settings.recent_notes.clear();
        self.recent_menu_cache = None;
        self.save_settings();
    }

    pub(super) fn handle_toggle_reopen_tabs(&mut self) {
        self.settings.reopen_tabs_on_startup = !self.settings.reopen_tabs_on_startup;
        self.save_settings();
    }
}
