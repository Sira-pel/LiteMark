//! Saving, opening and closing document tabs.

use super::*;

impl LiteMarkApp {
    pub(super) fn save_tab(&mut self, id: &str) -> Result<String, String> {
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
        let content = tab.content.clone();
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
            tab.file_path = Some(saved_path);
            tab.title = saved_title;
            let type_changed = tab.set_file_type(FileType::from_filename(&tab.title));
            tab.is_modified = false;
            if type_changed {
                self.refresh_find();
            }
        }
        self.rescan_workspace();
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
                self.active_tab_id = self.tabs[index].id.clone();
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
        self.active_tab_id = id;
        self.refresh_find();
    }

    pub(super) fn remove_tab(&mut self, id: &str, next_active: Option<String>) {
        let pos = self.tabs.iter().position(|tab| tab.id == id);
        self.tabs.retain(|tab| tab.id != id);
        if let Some(next) = next_active.filter(|next| self.tabs.iter().any(|tab| &tab.id == next)) {
            self.active_tab_id = next;
        } else if let Some(pos) = pos {
            if let Some(tab) = self.tabs.get(pos).or_else(|| self.tabs.last()) {
                self.active_tab_id = tab.id.clone();
            }
        }
        self.sync_selection_to_active_tab();
        self.refresh_find();
    }

    pub(super) fn discard_tab(&mut self, id: &str) {
        let only = self.tabs.len() <= 1 && self.tabs.iter().any(|tab| tab.id == id);
        if only {
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
}
