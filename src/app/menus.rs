//! Keyboard menu navigation, prompt modals and the close-window hook.

use super::*;

impl LiteMarkApp {
    /// Indices of selectable items (non-divider, non-heading, enabled) in
    /// a list of menu items.
    pub(super) fn selectable_menu_ixs(items: &[MenuItem]) -> Vec<usize> {
        items
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                !item.disabled
                    && !matches!(
                        item.kind,
                        MenuItemKind::Divider | MenuItemKind::Heading
                    )
            })
            .map(|(ix, _)| ix)
            .collect()
    }

    pub(super) fn menu_selectable_ixs(&self) -> Vec<usize> {
        Self::selectable_menu_ixs(&self.menu_items_cache)
    }

    /// Move the menu selection by `delta` items (wrapping), skipping
    /// dividers, headings and disabled entries. With a submenu open,
    /// navigation happens inside the submenu instead.
    pub(super) fn menu_move_selection(&mut self, delta: i32) {
        if self.open_menu.is_none() || self.open_menu == Some(MenuId::Table) {
            return;
        }
        if let Some(sub_ix) = self.menu_open_submenu_ix {
            let sub_items = match self
                .menu_items_cache
                .get(sub_ix)
                .map(|item| item.kind.clone())
            {
                Some(MenuItemKind::Submenu(items)) => items,
                _ => {
                    // The submenu no longer exists — close it and fall back.
                    self.menu_open_submenu_ix = None;
                    self.submenu_selected_ix = None;
                    return;
                }
            };
            let selectable = Self::selectable_menu_ixs(&sub_items);
            if selectable.is_empty() {
                return;
            }
            self.submenu_selected_ix = Some(match self.submenu_selected_ix {
                None => {
                    if delta < 0 {
                        selectable[selectable.len() - 1]
                    } else {
                        selectable[0]
                    }
                }
                Some(current) => {
                    let pos = selectable.iter().position(|&ix| ix == current).unwrap_or(0);
                    let len = selectable.len() as i32;
                    selectable[((pos as i32 + delta).rem_euclid(len)) as usize]
                }
            });
            return;
        }
        let selectable = self.menu_selectable_ixs();
        if selectable.is_empty() {
            return;
        }
        let next = match self.menu_selected_ix {
            None => {
                if delta < 0 {
                    selectable[selectable.len() - 1]
                } else {
                    selectable[0]
                }
            }
            Some(current) => {
                let pos = selectable.iter().position(|&ix| ix == current).unwrap_or(0);
                let len = selectable.len() as i32;
                let new_pos = (pos as i32 + delta).rem_euclid(len) as usize;
                selectable[new_pos]
            }
        };
        self.menu_selected_ix = Some(next);
    }

    /// Close the menu and return the action to run for the selected item,
    /// if any. The caller must run the action *outside* the view update —
    /// item actions themselves update this view, so calling them from
    /// inside a `view.update` would re-entrantly borrow it and panic.
    pub(super) fn menu_take_confirm_action(&mut self, cx: &mut Context<Self>) -> Option<MenuAction> {
        if self.open_menu.is_none() || self.open_menu == Some(MenuId::Table) {
            return None;
        }
        // Confirming with a submenu open activates the selected submenu item.
        if let Some(sub_ix) = self.menu_open_submenu_ix {
            let sub_items = match self
                .menu_items_cache
                .get(sub_ix)
                .map(|item| item.kind.clone())
            {
                Some(MenuItemKind::Submenu(items)) => items,
                _ => {
                    self.menu_open_submenu_ix = None;
                    self.submenu_selected_ix = None;
                    cx.notify();
                    return None;
                }
            };
            let sel = self.submenu_selected_ix?;
            let item = sub_items.get(sel)?;
            if item.disabled
                || !matches!(
                    item.kind,
                    MenuItemKind::Action | MenuItemKind::Check(_) | MenuItemKind::Radio(_)
                )
            {
                return None;
            }
            let action = item.action.clone();
            self.open_menu = None;
            self.menu_selected_ix = None;
            self.menu_open_submenu_ix = None;
            self.submenu_selected_ix = None;
            cx.notify();
            return Some(action);
        }
        let ix = self.menu_selected_ix?;
        let item = self.menu_items_cache.get(ix)?;
        if item.disabled {
            return None;
        }
        if matches!(item.kind, MenuItemKind::Submenu(_)) {
            self.menu_open_submenu_ix = if self.menu_open_submenu_ix == Some(ix) {
                None
            } else {
                Some(ix)
            };
            // Keyboard-opened submenus preselect their first selectable item.
            self.submenu_selected_ix = match (&self.menu_open_submenu_ix, &item.kind) {
                (Some(_), MenuItemKind::Submenu(sub_items)) => {
                    Self::selectable_menu_ixs(sub_items).first().copied()
                }
                _ => None,
            };
            cx.notify();
            return None;
        }
        if matches!(
            item.kind,
            MenuItemKind::Divider | MenuItemKind::Heading
        ) {
            return None;
        }
        let action = item.action.clone();
        self.open_menu = None;
        self.menu_selected_ix = None;
        self.menu_open_submenu_ix = None;
        self.submenu_selected_ix = None;
        cx.notify();
        Some(action)
    }

    /// Open a prompt and preselect its default choice so Enter confirms it.
    pub(super) fn show_prompt(&mut self, prompt: Prompt) {
        let (_, _, choices) = self.prompt_modal_data(&prompt);
        self.prompt_selected_ix = default_prompt_choice_index(&choices);
        self.prompt = Some(prompt);
    }

    /// Title, message and button choices for a prompt. The bool marks the
    /// default choice.
    pub(super) fn prompt_modal_data(
        &self,
        prompt: &Prompt,
    ) -> (&'static str, String, Vec<(&'static str, bool)>) {
        match prompt {
            Prompt::CloseTab { id } => {
                let name = self
                    .tabs
                    .iter()
                    .find(|tab| &tab.id == id)
                    .map(|tab| tab.title.clone())
                    .unwrap_or_else(|| "this note".into());
                (
                    "Unsaved changes",
                    format!("Save changes to {name} before closing?"),
                    vec![("Cancel", false), ("Discard", false), ("Save", true)],
                )
            }
            Prompt::CloseWindow => (
                "Unsaved changes",
                "Save your changes before closing the window?".into(),
                vec![("Cancel", false), ("Discard", false), ("Save", true)],
            ),
            Prompt::DeleteFile { id } => {
                let name = self
                    .workspace
                    .files
                    .iter()
                    .find(|file| &file.id == id)
                    .map(|file| file.name.clone())
                    .unwrap_or_else(|| "this note".into());
                (
                    "Move to Trash",
                    format!("Move {name} to the Trash?"),
                    vec![("Cancel", false), ("Trash", true)],
                )
            }
            Prompt::DeleteFolder { id } => {
                let name = self
                    .workspace
                    .folders
                    .iter()
                    .find(|folder| &folder.id == id)
                    .map(|folder| folder.name.clone())
                    .unwrap_or_else(|| "this folder".into());
                (
                    "Move to Trash",
                    format!("Move {name} and its contents to the Trash?"),
                    vec![("Cancel", false), ("Trash", true)],
                )
            }
            Prompt::TrashDirtyFile { id } => {
                let name = self
                    .workspace
                    .files
                    .iter()
                    .find(|file| &file.id == id)
                    .map(|file| file.name.clone())
                    .unwrap_or_else(|| "this note".into());
                (
                    "Unsaved changes",
                    format!("Save changes to {name} before moving it to the Trash?"),
                    vec![("Cancel", false), ("Discard", false), ("Save", true)],
                )
            }
            Prompt::TrashDirtyFolder { id } => {
                let name = self
                    .workspace
                    .folders
                    .iter()
                    .find(|folder| &folder.id == id)
                    .map(|folder| folder.name.clone())
                    .unwrap_or_else(|| "this folder".into());
                (
                    "Unsaved changes",
                    format!("Save changes in {name} before moving it to the Trash?"),
                    vec![("Cancel", false), ("Discard", false), ("Save", true)],
                )
            }
            Prompt::DeleteMany { targets } => (
                "Move to Trash",
                format!("Move {} items to the Trash?", targets.len()),
                vec![("Cancel", false), ("Trash", true)],
            ),
            Prompt::SwitchWorkspace { new_dir } => {
                let name = self
                    .active_tab()
                    .map(|tab| tab.title.clone())
                    .unwrap_or_else(|| "your current note".into());
                let dest = new_dir
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("the new folder");
                (
                    "Switch folder",
                    format!(
                        "Copy \u{201c}{name}\u{201d} into \u{201c}{dest}\u{201d} before switching? The original stays in the current folder either way."
                    ),
                    vec![("Not now", false), ("Copy note", true)],
                )
            }
        }
    }

    pub(super) fn answer_prompt(&mut self, choice: &'static str, cx: &mut Context<Self>) {
        let Some(prompt) = self.prompt.take() else {
            return;
        };
        match prompt {
            Prompt::CloseTab { id } => match choice {
                "Save" => {
                    // Untitled quick notes get a name before the close continues.
                    if self.tabs.iter().any(|t| t.id == id && t.file_path.is_none()) {
                        self.begin_save_naming(&id, cx);
                        self.pending_after_save = Some(PendingAfterSave::CloseTab(id));
                    } else {
                        match self.save_tab(&id) {
                            Ok(title) => {
                                self.show_toast(format!("Saved {title}"), cx);
                                self.finish_close_saved(&id);
                            }
                            Err(err) => {
                                self.show_error(err);
                                self.show_prompt(Prompt::CloseTab { id });
                            }
                        }
                    }
                }
                "Discard" => self.discard_tab(&id),
                _ => {}
            },
            Prompt::CloseWindow => match choice {
                "Save" => {
                    if let Some(id) = self.first_unnamed_dirty_tab() {
                        self.pending_after_save = Some(PendingAfterSave::CloseWindow);
                        self.begin_save_naming(&id, cx);
                    } else {
                        match self.save_all_dirty() {
                            Ok(()) => self.allow_window_close = true,
                            Err(err) => {
                                self.show_error(err);
                                self.show_prompt(Prompt::CloseWindow);
                            }
                        }
                    }
                }
                "Discard" => self.allow_window_close = true,
                _ => {}
            },
            Prompt::DeleteFile { id } => {
                if choice == "Trash" {
                    self.trash_file(&id, cx);
                }
            }
            Prompt::DeleteFolder { id } => {
                if choice == "Trash" {
                    self.trash_folder(&id, cx);
                }
            }
            Prompt::TrashDirtyFile { id } => match trash_action(match choice {
                "Save" => TrashChoice::Save,
                "Discard" => TrashChoice::Discard,
                _ => TrashChoice::Cancel,
            }) {
                TrashAction::Abort => {}
                TrashAction::DiscardThenTrash => self.trash_file(&id, cx),
                TrashAction::SaveThenTrash => {
                    let path = self
                        .workspace
                        .files
                        .iter()
                        .find(|file| file.id == id)
                        .map(|file| file.path.clone());
                    let Some(path) = path else {
                        self.show_error("That note is no longer in the folder.");
                        return;
                    };
                    let ids = self.dirty_tab_ids_for_path(&path);
                    if let Err(err) = self.save_ids(&ids) {
                        self.show_error(err);
                        self.show_prompt(Prompt::TrashDirtyFile { id });
                        return;
                    }
                    self.trash_file(&id, cx);
                }
            },
            Prompt::TrashDirtyFolder { id } => match trash_action(match choice {
                "Save" => TrashChoice::Save,
                "Discard" => TrashChoice::Discard,
                _ => TrashChoice::Cancel,
            }) {
                TrashAction::Abort => {}
                TrashAction::DiscardThenTrash => self.trash_folder(&id, cx),
                TrashAction::SaveThenTrash => {
                    let prefix = self
                        .workspace
                        .folders
                        .iter()
                        .find(|folder| folder.id == id)
                        .map(|folder| folder.path.clone());
                    let Some(prefix) = prefix else {
                        self.show_error("That folder is no longer available.");
                        return;
                    };
                    let ids = self.dirty_tab_ids_under(&prefix);
                    if let Err(err) = self.save_ids(&ids) {
                        self.show_error(err);
                        self.show_prompt(Prompt::TrashDirtyFolder { id });
                        return;
                    }
                    self.trash_folder(&id, cx);
                }
            },
            Prompt::DeleteMany { targets } => {
                if choice == "Trash" {
                    self.multi_selection = None;
                    let report: BulkReport = Rc::new(BulkTrash {
                        report: RefCell::new(BulkTrashReport::new(targets.len())),
                        prev_selected: self.selected_target.clone(),
                    });
                    for target in targets {
                        match target {
                            SidebarTarget::File(id) => {
                                self.trash_file_in(&id, cx, Some(report.clone()))
                            }
                            SidebarTarget::Folder(id) => {
                                self.trash_folder_in(&id, cx, Some(report.clone()))
                            }
                        }
                    }
                }
            }
            Prompt::SwitchWorkspace { new_dir } => match choice {
                "Copy note" => {
                    let current = self.migratable_note_path();
                    self.apply_workspace_switch(new_dir, current);
                }
                "Not now" => self.apply_workspace_switch(new_dir, None),
                _ => {}
            },
        }
        cx.notify();
    }

    pub(super) fn request_close_window(&mut self) {
        if self.tabs.iter().any(|tab| tab.is_modified) {
            self.show_prompt(Prompt::CloseWindow);
        } else {
            self.allow_window_close = true;
        }
    }

    pub(super) fn install_close_hook(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.close_hook_installed {
            return;
        }
        self.close_hook_installed = true;
        if self.bounds_subscription.is_none() {
            self.bounds_subscription =
                Some(cx.observe_window_bounds(window, |_this, window, cx| {
                    window.refresh();
                    cx.notify();
                }));
        }
        if self.activation_subscription.is_none() {
            self.activation_subscription =
                Some(cx.observe_window_activation(window, |this, window, cx| {
                    if !window.is_window_active() {
                        this.handle_window_deactivated();
                        cx.notify();
                    }
                }));
        }
        let view = cx.weak_entity();
        window.on_window_should_close(cx, move |_window, cx| {
            view.update(cx, |this, cx| {
                if this.allow_window_close {
                    return true;
                }
                if this.tabs.iter().any(|tab| tab.is_modified) {
                    this.show_prompt(Prompt::CloseWindow);
                    cx.notify();
                    false
                } else {
                    true
                }
            })
            .unwrap_or(true)
        });
    }
}
