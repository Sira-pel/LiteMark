//! Sidebar tree: selection, expand and collapse, new notes and folders.

use super::*;

impl LiteMarkApp {
    pub(super) fn folder_pairs(&self) -> Vec<(String, PathBuf)> {
        self.workspace
            .folders
            .iter()
            .map(|folder| (folder.id.clone(), folder.path.clone()))
            .collect()
    }

    pub fn toggle_folder(&mut self, folder_id: &str) {
        if let Some(folder) = self.workspace.folders.iter_mut().find(|f| f.id == folder_id) {
            folder.is_open = !folder.is_open;
        }
        // Clicking a folder selects it (Delete-key target) and keeps
        // keyboard navigation in the sidebar tree.
        self.multi_selection = None;
        self.selected_target = Some(SidebarTarget::Folder(folder_id.to_string()));
        self.sidebar_keyboard = true;
    }

    /// Point the sidebar selection at the active tab's note (tab-bar clicks,
    /// tab closes) and hand keyboard focus back to the editor.
    pub(super) fn sync_selection_to_active_tab(&mut self) {
        self.selected_target = self
            .active_tab()
            .and_then(|tab| tab.workspace_file_id.clone())
            .map(SidebarTarget::File);
        self.sidebar_keyboard = false;
    }

    /// Flatten the visible sidebar tree into selection targets, in row order.
    pub(super) fn sidebar_visible_targets(&self) -> Vec<SidebarTarget> {
        if !self.search_query.trim().is_empty() {
            let folders =
                sidebar_folder_search_matches(&self.workspace.folders, &self.search_query)
                    .into_iter()
                    .map(|folder| SidebarTarget::Folder(folder.id.clone()));
            let files = sidebar_search_matches(&self.workspace.files, &self.search_query)
                .into_iter()
                .map(|file| SidebarTarget::File(file.id.clone()));
            return folders.chain(files).collect();
        }
        build_sidebar_tree(
            &self.workspace.folders,
            &self.workspace.files,
            self.create_draft.as_ref(),
        )
        .iter()
        .filter_map(|item| match item {
            SidebarTreeItem::Folder { folder, .. } => {
                Some(SidebarTarget::Folder(folder.id.clone()))
            }
            SidebarTreeItem::File { file, .. } => Some(SidebarTarget::File(file.id.clone())),
            SidebarTreeItem::CreateInput { .. } => None,
        })
        .collect()
    }

    /// The Shift-selected rows, if the selection still has several.
    pub(super) fn active_multi(&self) -> Option<&MultiSelection> {
        self.multi_selection
            .as_ref()
            .filter(|m| self.selected_target.as_ref() == Some(&m.cursor) && m.items.len() > 1)
    }

    /// Extend the selection from its anchor to `to` over the visible rows.
    pub(super) fn extend_selection_to(&mut self, to: SidebarTarget) {
        let anchor = self
            .active_multi()
            .map(|m| m.anchor.clone())
            .or_else(|| self.selected_target.clone())
            .unwrap_or_else(|| to.clone());
        let (anchor, items) = extend_selection(&self.sidebar_visible_targets(), &anchor, &to);
        self.multi_selection = (items.len() > 1).then(|| MultiSelection {
            anchor,
            cursor: to.clone(),
            items,
        });
        self.selected_target = Some(to);
        self.sidebar_keyboard = true;
    }

    /// Ctrl/Cmd+click: add `target` to the selection, or drop it if already in.
    /// With nothing selected the open note counts as the starting selection.
    pub(super) fn toggle_selection(&mut self, target: SidebarTarget) {
        let multi = self.active_multi().cloned();
        let items = match &multi {
            Some(m) => m.items.clone(),
            None => self
                .selected_target
                .clone()
                .or_else(|| {
                    self.active_tab()
                        .and_then(|tab| tab.workspace_file_id.clone())
                        .map(SidebarTarget::File)
                })
                .into_iter()
                .collect(),
        };
        let toggled = toggle_row(items, multi.map(|m| m.anchor), target);
        self.selected_target = toggled.as_ref().map(|(_, cursor, _)| cursor.clone());
        self.multi_selection = toggled.and_then(|(anchor, cursor, items)| {
            (items.len() > 1).then_some(MultiSelection { anchor, cursor, items })
        });
        self.sidebar_keyboard = true;
    }

    /// Ctrl+A in the sidebar: select every visible row.
    pub(super) fn sidebar_select_all(&mut self) {
        let targets = self.sidebar_visible_targets();
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
    }

    /// Shift+Up/Down: grow or shrink the range by one visible row.
    pub(super) fn sidebar_extend_selection(&mut self, delta: i32) {
        let targets = self.sidebar_visible_targets();
        if let Some(next) = step_cursor(&targets, self.selected_target.as_ref(), delta) {
            self.extend_selection_to(targets[next].clone());
        }
    }

    pub(super) fn target_path(&self, target: &SidebarTarget) -> Option<PathBuf> {
        match target {
            SidebarTarget::Folder(id) => {
                self.workspace.folders.iter().find(|f| &f.id == id).map(|f| f.path.clone())
            }
            SidebarTarget::File(id) => {
                self.workspace.files.iter().find(|f| &f.id == id).map(|f| f.path.clone())
            }
        }
    }

    /// Ask before trashing every Shift-selected row. Rows inside a selected
    /// folder go with it; modified notes must be saved or closed first.
    pub(super) fn request_trash_many(&mut self, items: Vec<SidebarTarget>) {
        let rows: Vec<(SidebarTarget, PathBuf, bool)> = items
            .into_iter()
            .filter_map(|t| {
                let (path, is_folder) = match &t {
                    SidebarTarget::Folder(id) => {
                        (self.workspace.folders.iter().find(|f| &f.id == id)?.path.clone(), true)
                    }
                    SidebarTarget::File(id) => {
                        (self.workspace.files.iter().find(|f| &f.id == id)?.path.clone(), false)
                    }
                };
                Some((t, path, is_folder))
            })
            .collect();
        // A partly selected folder stays; only its selected rows go.
        let visible: Vec<PathBuf> = self
            .sidebar_visible_targets()
            .iter()
            .filter_map(|t| self.target_path(t))
            .collect();
        let rows = without_partial_folders(&rows, &visible);
        let modified: Vec<PathBuf> = self
            .tabs
            .iter()
            .filter(|tab| tab.is_modified)
            .filter_map(|tab| tab.file_path.clone())
            .collect();
        match plan_bulk_trash(&rows, &modified) {
            BulkTrashPlan::Nothing => {}
            BulkTrashPlan::ModifiedOpen => self.show_error(
                "Save or close modified notes before moving several items to the Trash.",
            ),
            BulkTrashPlan::Trash(mut targets) => match targets.len() {
                1 => match targets.remove(0) {
                    SidebarTarget::File(id) => self.request_trash_file(id),
                    SidebarTarget::Folder(id) => self.request_trash_folder(id),
                },
                _ => self.show_prompt(Prompt::DeleteMany { targets }),
            },
        }
    }

    /// Move the sidebar selection through the visible tree (wrapping).
    pub(super) fn sidebar_move_selection(&mut self, delta: i32) {
        let targets = self.sidebar_visible_targets();
        if targets.is_empty() {
            return;
        }
        let next = match &self.selected_target {
            None => {
                if delta < 0 {
                    targets[targets.len() - 1].clone()
                } else {
                    targets[0].clone()
                }
            }
            Some(current) => {
                let pos = targets
                    .iter()
                    .position(|t| t == current)
                    .map(|p| p as i32)
                    .unwrap_or(0);
                let len = targets.len() as i32;
                let new_pos = (pos + delta).rem_euclid(len) as usize;
                targets[new_pos].clone()
            }
        };
        self.multi_selection = None;
        self.selected_target = Some(next);
        self.sidebar_keyboard = true;
    }

    /// Enter in the sidebar: open the selected note (hands focus to the
    /// editor) or toggle the selected folder.
    pub(super) fn sidebar_confirm(&mut self, cx: &mut Context<Self>) {
        let Some(target) = self.selected_target.clone() else {
            return;
        };
        match target {
            SidebarTarget::Folder(id) => {
                self.toggle_folder(&id);
                cx.notify();
            }
            SidebarTarget::File(id) => {
                let Some(file) = self
                    .workspace
                    .files
                    .iter()
                    .find(|f| f.id == id)
                    .cloned()
                else {
                    return;
                };
                match read_note(&file.path) {
                    Ok(content) => {
                        self.open_loaded_tab(
                            file.name.clone(),
                            content,
                            Some(file.path.clone()),
                            Some(file.id.clone()),
                        );
                    }
                    Err(err) => self.show_error(err),
                }
                cx.notify();
            }
        }
    }

    /// Left arrow: collapse the selected folder, or move a note's selection
    /// up to its parent folder.
    pub(super) fn sidebar_collapse(&mut self, cx: &mut Context<Self>) {
        let Some(target) = self.selected_target.clone() else {
            return;
        };
        match target {
            SidebarTarget::Folder(id) => {
                if let Some(folder) = self
                    .workspace
                    .folders
                    .iter_mut()
                    .find(|f| f.id == id)
                {
                    folder.is_open = false;
                }
                self.sidebar_keyboard = true;
            }
            SidebarTarget::File(id) => {
                let parent = self
                    .workspace
                    .files
                    .iter()
                    .find(|f| f.id == id)
                    .and_then(|f| f.folder_id.clone());
                if let Some(fid) = parent {
                    self.selected_target = Some(SidebarTarget::Folder(fid));
                }
                self.sidebar_keyboard = true;
            }
        }
        cx.notify();
    }

    /// Right arrow: expand the selected folder, or open the selected note.
    pub(super) fn sidebar_expand(&mut self, cx: &mut Context<Self>) {
        let Some(target) = self.selected_target.clone() else {
            return;
        };
        match target {
            SidebarTarget::Folder(id) => {
                if let Some(folder) = self.workspace.folders.iter_mut().find(|f| f.id == id) {
                    folder.is_open = true;
                }
                self.sidebar_keyboard = true;
                cx.notify();
            }
            SidebarTarget::File(_) => self.sidebar_confirm(cx),
        }
    }

    pub fn rescan_workspace(&mut self) {
        let open_map: std::collections::HashMap<PathBuf, bool> = self
            .workspace
            .folders
            .iter()
            .map(|f| (f.path.clone(), f.is_open))
            .collect();
        let mut new_workspace =
            scan_workspace_dir(&self.workspace.folder_path, self.settings.show_code_files);
        for folder in &mut new_workspace.folders {
            if let Some(&open) = open_map.get(&folder.path) {
                folder.is_open = open;
            }
        }
        self.workspace = new_workspace;
        self.refresh_workspace_ids();
    }

    pub(super) fn create_note(&mut self, folder_id: Option<String>, desired: String) -> Result<(), String> {
        let folders = self.folder_pairs();
        let dir = child_directory(&self.workspace.folder_path, &folders, folder_id.as_deref());
        let name = next_available_filename(&dir, &desired);
        match safe_create_file(&dir, &name, "") {
            Ok(path) => {
                if let Some(fid) = &folder_id {
                    if let Some(folder) = self.workspace.folders.iter_mut().find(|f| &f.id == fid) {
                        folder.is_open = true;
                    }
                }
                self.rescan_workspace();
                let file_id = self
                    .workspace
                    .files
                    .iter()
                    .find(|file| file.path == path)
                    .map(|file| file.id.clone());
                let display = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or(&name)
                    .to_string();
                self.open_loaded_tab(display, String::new(), Some(path), file_id);
                Ok(())
            }
            Err(err) => Err(format!("Couldn't create note: {err}")),
        }
    }

    pub(super) fn create_folder(&mut self, parent_id: Option<String>, desired: String) -> Result<(), String> {
        let folders = self.folder_pairs();
        let dir = child_directory(&self.workspace.folder_path, &folders, parent_id.as_deref());
        let name = unique_name(&sanitize_name(&desired), &|candidate| {
            dir.join(candidate).exists()
        });
        match safe_create_folder(&dir, &name) {
            Ok(path) => {
                if let Some(pid) = &parent_id {
                    if let Some(folder) = self.workspace.folders.iter_mut().find(|f| &f.id == pid) {
                        folder.is_open = true;
                    }
                }
                self.rescan_workspace();
                // Keep the fresh folder visible in the tree.
                if let Some(folder) = self
                    .workspace
                    .folders
                    .iter_mut()
                    .find(|f| f.path == path)
                {
                    folder.is_open = true;
                }
                Ok(())
            }
            Err(err) => Err(format!("Couldn't create folder: {err}")),
        }
    }

    pub(super) fn refresh_workspace_ids(&mut self) {
        for tab in &mut self.tabs {
            tab.workspace_file_id = tab.file_path.as_ref().and_then(|path| {
                self.workspace
                    .files
                    .iter()
                    .find(|file| &file.path == path)
                    .map(|file| file.id.clone())
            });
        }
    }

    /// Number of keyboard-selectable actions in the currently open modal
    /// popup (0 = no modal keyboard focus).
    pub(super) fn modal_choice_count(&self) -> usize {
        if let Some(prompt) = &self.prompt {
            self.prompt_modal_data(prompt).2.len()
        } else if self.show_pdf_modal || self.sidebar_menu.is_some() {
            2
        } else if self.show_about_modal || self.show_shortcuts_modal {
            1
        } else {
            0
        }
    }
}
