//! Sidebar tree: selection, expand and collapse, new notes and folders.

use super::*;

impl LiteMarkApp {
    pub(super) fn folder_pairs(&self) -> Vec<(String, PathBuf)> {
        self.workspace
            .folders
            .iter()
            .map(|folder| (folder.id.to_string(), folder.path.clone()))
            .collect()
    }

    pub fn toggle_folder(&mut self, folder_id: &str) {
        if let Some(folder) = self
            .workspace
            .folders
            .iter_mut()
            .find(|f| f.id.as_ref() == folder_id)
        {
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

    /// The sidebar's folder tree, rebuilt only when the workspace or the
    /// pending create row changed since the last call.
    pub(super) fn sidebar_tree(&self) -> std::rc::Rc<Vec<SidebarTreeItem>> {
        let generation = self.workspace.generation();
        let mut cache = self.sidebar_tree.borrow_mut();
        if let Some(hit) = cache
            .as_ref()
            .filter(|c| c.generation == generation && c.creating == self.create_draft)
        {
            return hit.items.clone();
        }
        let items = std::rc::Rc::new(build_sidebar_tree(
            &self.workspace.folders,
            &self.workspace.files,
            self.create_draft.as_ref(),
        ));
        *cache = Some(SidebarTreeCache {
            generation,
            creating: self.create_draft.clone(),
            items: items.clone(),
        });
        items
    }

    /// What the sidebar lists: the folder tree, or the search matches when a
    /// query is typed. Search results are cached per workspace generation
    /// and query, so a redraw doesn't filter every note again.
    pub(super) fn sidebar_rows(&self) -> std::rc::Rc<Vec<SidebarTreeItem>> {
        let query = self.search_query.trim();
        if query.is_empty() {
            return self.sidebar_tree();
        }
        let generation = self.workspace.generation();
        let mut cache = self.sidebar_search.borrow_mut();
        if let Some((cached_gen, cached_query, items)) = cache.as_ref() {
            if *cached_gen == generation && cached_query == &self.search_query {
                return items.clone();
            }
        }
        // Same matching the sidebar used to do on every frame, as row indices.
        let query = query.to_lowercase();
        let folders = self
            .workspace
            .folders
            .iter()
            .enumerate()
            .filter(|(_, f)| f.name.to_lowercase().contains(&query))
            .map(|(index, _)| SidebarTreeItem::Folder {
                index: index as u32,
                depth: 0,
            });
        let files = self
            .workspace
            .files
            .iter()
            .enumerate()
            .filter(|(_, f)| {
                let stem = f.path.file_stem().and_then(|s| s.to_str());
                stem.unwrap_or(&f.name).to_lowercase().contains(&query)
            })
            .map(|(index, _)| SidebarTreeItem::File {
                index: index as u32,
                depth: 0,
            });
        let items = std::rc::Rc::new(folders.chain(files).collect::<Vec<_>>());
        *cache = Some((generation, self.search_query.clone(), items.clone()));
        items
    }

    /// Flatten the visible sidebar tree into selection targets, in row order.
    pub(super) fn sidebar_visible_targets(&self) -> Vec<SidebarTarget> {
        if !self.search_query.trim().is_empty() {
            let folders =
                sidebar_folder_search_matches(&self.workspace.folders, &self.search_query)
                    .into_iter()
                    .map(|folder| SidebarTarget::Folder(folder.id.to_string()));
            let files = sidebar_search_matches(&self.workspace.files, &self.search_query)
                .into_iter()
                .map(|file| SidebarTarget::File(file.id.clone()));
            return folders.chain(files).collect();
        }
        self.sidebar_tree()
            .iter()
            .filter_map(|item| match *item {
                SidebarTreeItem::Folder { index, .. } => self
                    .workspace
                    .folders
                    .get(index as usize)
                    .map(|folder| SidebarTarget::Folder(folder.id.to_string())),
                SidebarTreeItem::File { index, .. } => self
                    .workspace
                    .files
                    .get(index as usize)
                    .map(|file| SidebarTarget::File(file.id.clone())),
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
            (items.len() > 1).then_some(MultiSelection {
                anchor,
                cursor,
                items,
            })
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
            self.sidebar_scroll_to_selected = Some(sidebar_scroll_strategy(delta));
        }
    }

    pub(super) fn target_path(&self, target: &SidebarTarget) -> Option<PathBuf> {
        match target {
            SidebarTarget::Folder(id) => self
                .workspace
                .folders
                .iter()
                .find(|f| f.id.as_ref() == id.as_str())
                .map(|f| f.path.clone()),
            SidebarTarget::File(id) => self
                .workspace
                .files
                .iter()
                .find(|f| &f.id == id)
                .map(|f| f.path.clone()),
        }
    }

    /// Ask before trashing every Shift-selected row. Rows inside a selected
    /// folder go with it; modified notes must be saved or closed first.
    pub(super) fn request_trash_many(&mut self, items: Vec<SidebarTarget>) {
        let rows: Vec<(SidebarTarget, PathBuf, bool)> = items
            .into_iter()
            .filter_map(|t| {
                let (path, is_folder) = match &t {
                    SidebarTarget::Folder(id) => (
                        self.workspace
                            .folders
                            .iter()
                            .find(|f| f.id.as_ref() == id.as_str())?
                            .path
                            .clone(),
                        true,
                    ),
                    SidebarTarget::File(id) => (
                        self.workspace
                            .files
                            .iter()
                            .find(|f| &f.id == id)?
                            .path
                            .clone(),
                        false,
                    ),
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
        self.sidebar_scroll_to_selected = Some(sidebar_scroll_strategy(delta));
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
                let Some(file) = self.workspace.files.iter().find(|f| f.id == id).cloned() else {
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
                    .find(|f| f.id.as_ref() == id.as_str())
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
                    .and_then(|f| f.folder_id.as_deref().map(str::to_string));
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
                if let Some(folder) = self
                    .workspace
                    .folders
                    .iter_mut()
                    .find(|f| f.id.as_ref() == id.as_str())
                {
                    folder.is_open = true;
                }
                self.sidebar_keyboard = true;
                cx.notify();
            }
            SidebarTarget::File(_) => self.sidebar_confirm(cx),
        }
    }

    pub fn rescan_workspace(&mut self) {
        // A scan still running in the background is now out of date.
        self.scan_generation += 1;
        self.rescan_requested = false;
        let scanned =
            scan_workspace_dir(&self.workspace.folder_path, self.settings.show_code_files);
        self.apply_scan(scanned);
    }

    /// Ask for a rescan that doesn't block: for the callers that don't read
    /// the new tree straight afterwards. It starts at the next frame.
    pub(super) fn request_rescan(&mut self) {
        self.rescan_requested = true;
    }

    /// Start the rescan [`Self::request_rescan`] asked for, on the background
    /// executor. Only the newest scan is applied.
    pub(super) fn start_requested_rescan(&mut self, cx: &mut Context<Self>) {
        if !std::mem::take(&mut self.rescan_requested) {
            return;
        }
        self.scan_generation += 1;
        let generation = self.scan_generation;
        let folder = self.workspace.folder_path.clone();
        let show_code_files = self.settings.show_code_files;
        cx.spawn(async move |this, cx| {
            let scanned = {
                let folder = folder.clone();
                cx.background_executor()
                    .spawn(async move { scan_workspace_dir(&folder, show_code_files) })
                    .await
            };
            this.update(cx, |this, cx| {
                // Skip a result that a newer scan, a workspace switch or a
                // settings change has outdated.
                if this.scan_generation == generation
                    && this.workspace.folder_path == folder
                    && this.settings.show_code_files == show_code_files
                {
                    this.apply_scan(scanned);
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// Replace the tree with `scanned`, keeping the folders the user has
    /// open or closed *now* (not when the scan started).
    pub(super) fn apply_scan(&mut self, mut scanned: WorkspaceDirectory) {
        let open_map: std::collections::HashMap<PathBuf, bool> = self
            .workspace
            .folders
            .iter()
            .map(|f| (f.path.clone(), f.is_open))
            .collect();
        for folder in &mut scanned.folders {
            if let Some(&open) = open_map.get(&folder.path) {
                folder.is_open = open;
            }
        }
        self.workspace = scanned.into();
        self.refresh_workspace_ids();
    }

    pub(super) fn create_note(
        &mut self,
        folder_id: Option<String>,
        desired: String,
    ) -> Result<(), String> {
        let folders = self.folder_pairs();
        let dir = child_directory(&self.workspace.folder_path, &folders, folder_id.as_deref());
        let name = next_available_filename(&dir, &desired);
        match safe_create_file(&dir, &name, "") {
            Ok(path) => {
                if let Some(fid) = &folder_id {
                    if let Some(folder) = self
                        .workspace
                        .folders
                        .iter_mut()
                        .find(|f| f.id.as_ref() == fid.as_str())
                    {
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

    pub(super) fn create_folder(
        &mut self,
        parent_id: Option<String>,
        desired: String,
    ) -> Result<(), String> {
        let folders = self.folder_pairs();
        let dir = child_directory(&self.workspace.folder_path, &folders, parent_id.as_deref());
        let name = unique_name(&sanitize_name(&desired), &|candidate| {
            dir.join(candidate).exists()
        });
        match safe_create_folder(&dir, &name) {
            Ok(path) => {
                if let Some(pid) = &parent_id {
                    if let Some(folder) = self
                        .workspace
                        .folders
                        .iter_mut()
                        .find(|f| f.id.as_ref() == pid.as_str())
                    {
                        folder.is_open = true;
                    }
                }
                self.rescan_workspace();
                // Keep the fresh folder visible in the tree.
                if let Some(folder) = self.workspace.folders.iter_mut().find(|f| f.path == path) {
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

/// Where a keyboard move scrolls the selected row to: moving down brings it
/// in at the bottom (one row of scroll per step), moving up at the top.
pub(super) fn sidebar_scroll_strategy(delta: i32) -> ScrollStrategy {
    if delta > 0 {
        ScrollStrategy::Bottom
    } else {
        ScrollStrategy::Top
    }
}

#[cfg(test)]
mod scroll_strategy_tests {
    use super::*;
    // `use gpui::*` brings in a `test` attribute that is not Rust's `#[test]`.
    use ::core::prelude::v1::test as std_test;

    #[std_test]
    fn moving_down_scrolls_from_the_bottom() {
        assert_eq!(sidebar_scroll_strategy(1), ScrollStrategy::Bottom);
        assert_eq!(sidebar_scroll_strategy(-1), ScrollStrategy::Top);
    }
}
