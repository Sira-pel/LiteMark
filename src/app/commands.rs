//! Menu and shortcut command handlers: tabs, files, view settings, clipboard.

use super::*;

impl LiteMarkApp {
    pub(super) fn clear_table_drag_press(&mut self) {
        self.table_drag_press_held = false;
        self.table_pressed = false;
    }

    pub(super) fn handle_window_deactivated(&mut self) {
        self.table_drag = None;
        self.clear_table_drag_press();
    }

    pub(super) fn handle_mouse_down_out(&mut self) {
        self.table_drag = None;
        self.clear_table_drag_press();
    }

    /// Pointer left the window. Only a no-button exit ends the press:
    /// - Wayland: the implicit grab delivers MouseUp (`on_mouse_up_out`)
    ///   before Leave, so this event usually has no button.
    /// - X11: no grab, so XI_Leave arrives mid-press (`pressed_button` Some).
    ///   Keep the latch until the press really ends (no-button move, next
    ///   down, focus loss, Escape/CloseModal, right-click).
    /// - Windows: SetCapture, so MouseUp still arrives outside.
    ///
    /// A held exit keeps `table_drag` so an X11 re-entry can continue the
    /// drop preview. The latch stays set either way until the press ends.
    pub(super) fn handle_mouse_exit(&mut self, event: &MouseExitEvent) {
        if event.pressed_button.is_some() {
            return;
        }
        self.table_drag = None;
        self.table_hot = None;
        self.clear_table_drag_press();
    }

    pub(super) fn handle_close_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.clear_table_drag_press();
        if self.table_drag.take().is_some() {
            cx.notify();
            return;
        }
        if self.editor_context_menu.is_some() {
            self.dismiss_editor_context_menu();
        } else if self.workspace_menu_open {
            self.workspace_menu_open = false;
        } else if self.sidebar_menu.is_some() {
            self.sidebar_menu = None;
        } else if self.rename_target.is_some() || self.save_as_tab.is_some() {
            self.cancel_rename(cx);
        } else if self.create_draft.is_some() {
            self.cancel_create(cx);
        } else if self.prompt.is_some() {
            self.prompt = None;
        } else if self.error_message.is_some()
            && self.open_menu.is_none()
            && !self.show_about_modal
            && !self.show_shortcuts_modal
            && !self.show_pdf_modal
            && !self.show_find_replace
            && !self.zen_mode
        {
            self.error_message = None;
        } else if self.open_menu.is_some() {
            self.open_menu = None;
            self.menu_selected_ix = None;
            self.menu_open_submenu_ix = None;
        } else if self.show_about_modal {
            self.show_about_modal = false;
        } else if self.show_shortcuts_modal {
            self.show_shortcuts_modal = false;
        } else if self.show_pdf_modal {
            self.show_pdf_modal = false;
        } else if self.show_find_replace {
            self.show_find_replace = false;
            self.refresh_find();
        } else if self.field_focus == TextInput::Search || !self.search_query.is_empty() {
            self.clear_search(cx);
        } else if self.active_multi().is_some() || self.selected_target.is_some() {
            self.multi_selection = None;
            self.selected_target = None;
            self.sidebar_keyboard = false;
        } else if self.show_sidebar
            && !self.zen_mode
            && window.viewport_size().width < px(600.0)
        {
            self.show_sidebar = false;
        } else if self.zen_mode {
            self.handle_exit_zen();
        }
        cx.notify();
    }

    /// Look for a newer release in the background, at most once a day.
    /// Offline or failed checks are silent and retried on the next launch.
    pub(super) fn start_update_check(&mut self, cx: &mut Context<Self>) {
        self.run_update_check(false, cx);
    }

    /// `manual` checks ignore the auto-check setting, the once-per-interval
    /// limit and an earlier dismissal, and always report the outcome.
    pub(super) fn run_update_check(&mut self, manual: bool, cx: &mut Context<Self>) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        if !manual
            && (!self.settings.check_for_updates
                || !check_is_due(self.settings.last_update_check, now))
        {
            return;
        }
        if self.update_check_running {
            return;
        }
        self.update_check_running = true;
        if manual {
            self.show_toast("Checking for updates…", cx);
        }
        cx.spawn(async move |this, cx| {
            let release = cx
                .background_executor()
                .spawn(async { fetch_latest_release() })
                .await;
            this.update(cx, |this, cx| {
                this.update_check_running = false;
                let Some(release) = release else {
                    if manual {
                        this.show_toast("Couldn't check for updates", cx);
                    }
                    return;
                };
                this.settings.last_update_check = Some(now);
                let newer = is_newer(current_version(), &release.version);
                if manual {
                    if newer {
                        this.settings.dismissed_update = None;
                        this.saved_toast = None;
                        this.available_update = Some(release);
                    } else {
                        this.show_toast("You're up to date", cx);
                    }
                } else {
                    let dismissed =
                        this.settings.dismissed_update.as_deref() == Some(&release.version);
                    if newer && !dismissed {
                        this.available_update = Some(release);
                    }
                }
                this.settings.save();
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub fn handle_toggle_update_checks(&mut self) {
        self.settings.check_for_updates = !self.settings.check_for_updates;
        if !self.settings.check_for_updates {
            self.available_update = None;
        }
        self.settings.save();
    }

    /// Enter in the search box: select the first match and move keyboard
    /// focus to the list so the arrows walk through the results.
    pub(super) fn search_enter(&mut self, cx: &mut Context<Self>) {
        if let Some(first) = self.sidebar_visible_targets().first().cloned() {
            self.selected_target = Some(first);
            self.multi_selection = None;
            self.field_focus = TextInput::Editor;
            self.field_owns_keys = false;
            self.sidebar_keyboard = true;
        }
        cx.notify();
    }

    /// Empty the search box and hand focus back to the editor.
    pub(super) fn clear_search(&mut self, cx: &mut Context<Self>) {
        self.search_query.clear();
        self.search_field.update(cx, |field, _cx| field.set_text(""));
        self.field_focus = TextInput::Editor;
        self.field_owns_keys = false;
        self.applied_focus = None;
    }

    pub(super) fn claim_field(&mut self, which: TextInput, cx: &mut Context<Self>) {
        self.field_focus = which;
        self.field_owns_keys = !document_accepts_keystrokes(which);
        self.sidebar_keyboard = sidebar_keyboard_after_claim(self.sidebar_keyboard, which);
        if which == TextInput::Editor {
            self.applied_focus = None;
        }
        cx.notify();
    }

    pub(super) fn line_field_for_focus(&self) -> Option<Entity<LineField>> {
        match self.field_focus {
            TextInput::Find if self.show_find_replace => Some(self.find_field.clone()),
            TextInput::Replace if self.show_find_replace => Some(self.replace_field.clone()),
            TextInput::Rename
                if self.rename_target.is_some() || self.save_as_tab.is_some() =>
            {
                Some(self.rename_field.clone())
            }
            TextInput::Create if self.create_draft.is_some() => Some(self.create_field.clone()),
            TextInput::Search => Some(self.search_field.clone()),
            _ => None,
        }
    }

    /// The platform input handler may still be the editor for a frame after a
    /// field click. Deliver the keystroke to the field that owns it.
    pub(super) fn forward_keystroke_to_field(
        &mut self,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let focused = if self.find_field.read(cx).is_focused(window) {
            Some(self.find_field.clone())
        } else if self.replace_field.read(cx).is_focused(window) {
            Some(self.replace_field.clone())
        } else if self.rename_field.read(cx).is_focused(window) {
            Some(self.rename_field.clone())
        } else if self.create_field.read(cx).is_focused(window) {
            Some(self.create_field.clone())
        } else if self.search_field.read(cx).is_focused(window) {
            Some(self.search_field.clone())
        } else {
            self.line_field_for_focus()
        };
        let Some(field) = focused else {
            return false;
        };
        let find_id = self.find_field.entity_id();
        let replace_id = self.replace_field.entity_id();
        let written = field.update(cx, |field, cx| field.push_input(text, cx));
        if field.entity_id() == find_id {
            self.find_term = written;
            self.find_index = None;
            self.refresh_find();
        } else if field.entity_id() == replace_id {
            self.replace_term = written;
        } else if field.entity_id() == self.search_field.entity_id() {
            self.search_query = written;
            self.multi_selection = None;
        }
        window.refresh();
        cx.notify();
        true
    }

    pub fn active_tab(&self) -> Option<&DocumentTab> {
        self.tabs.iter().find(|t| t.id == self.active_tab_id)
    }

    pub fn active_tab_mut(&mut self) -> Option<&mut DocumentTab> {
        let global = self.settings.view_mode;
        let tab = self.tabs.iter_mut().find(|t| t.id == self.active_tab_id)?;
        tab.set_view_mode(tab.file_type.effective_view_mode(global));
        Some(tab)
    }

    /// The active tab ignores the global view mode (code and plain text).
    pub(super) fn view_locked_raw(&self) -> bool {
        self.active_tab()
            .is_some_and(|tab| tab.file_type.forces_raw())
    }

    /// The view mode the active tab renders and edits in. Code and plain-text
    /// tabs are always raw; the global Markdown setting only applies to the rest.
    pub fn effective_view_mode(&self) -> ViewMode {
        let global = self.settings.view_mode;
        self.active_tab()
            .map_or(global, |tab| tab.file_type.effective_view_mode(global))
    }

    /// Activate the next (`delta` = 1) or previous (-1) tab, wrapping around.
    pub(super) fn cycle_tab(&mut self, delta: isize) {
        let len = self.tabs.len() as isize;
        if len < 2 {
            return;
        }
        let cur = self
            .tabs
            .iter()
            .position(|t| t.id == self.active_tab_id)
            .unwrap_or(0) as isize;
        let next = (cur + delta).rem_euclid(len) as usize;
        self.active_tab_id = self.tabs[next].id.clone();
        self.sync_selection_to_active_tab();
        self.refresh_find();
    }

    pub fn handle_new_tab(&mut self) {
        let new_id = self.tab_ids.alloc();
        let number = new_id.trim_start_matches("tab-");
        let title = format!("Untitled-{number}.md");
        let tab = DocumentTab::new(new_id.clone(), title, String::new(), None, None);
        self.tabs.push(tab);
        self.active_tab_id = new_id;
    }

    pub fn handle_close_tab(&mut self, id: String) {
        let snaps: Vec<TabSnap<'_>> = self
            .tabs
            .iter()
            .map(|tab| TabSnap {
                id: tab.id.as_str(),
                modified: tab.is_modified,
            })
            .collect();
        let plan = plan_close(&snaps, &id);
        match plan {
            ClosePlan::KeepLast => {}
            ClosePlan::Prompt => self.show_prompt(Prompt::CloseTab { id }),
            ClosePlan::Close { next_active } => self.remove_tab(&id, next_active),
        }
    }

    pub fn handle_save(&mut self, cx: &mut Context<Self>) {
        let id = self.active_tab_id.clone();
        // Untitled quick notes (Ctrl+T) are named at save time.
        if self.tabs.iter().any(|t| t.id == id && t.file_path.is_none()) {
            self.begin_save_naming(&id, cx);
            return;
        }
        match self.save_tab(&id) {
            Ok(title) => self.show_toast(format!("Saved {title}"), cx),
            Err(err) => self.show_file_error(err),
        }
    }

    pub fn handle_open_file(&mut self) {
        if let Some(message) = file_dialog_error() {
            self.show_error(message);
            return;
        }
        let Some(path) = pick_open_file() else {
            return;
        };
        let name = path
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or("Untitled.md")
            .to_string();
        if is_pdf_path(&path) {
            let paths: Vec<Option<&Path>> = self
                .tabs
                .iter()
                .map(|tab| tab.file_path.as_deref())
                .collect();
            if let Some(index) = tab_index_for_path(&paths, &path) {
                self.active_tab_id = self.tabs[index].id.clone();
                self.show_pdf_modal = true;
                self.prompt_selected_ix = 0;
                return;
            }
            match extract_text_from_pdf(&path) {
                Ok((extracted_md, pages)) => {
                    let new_id = self.tab_ids.alloc();
                    let mut tab =
                        DocumentTab::new(new_id.clone(), name, extracted_md, Some(path), None);
                    tab.file_type = FileType::Pdf;
                    tab.pdf_page_count = Some(pages);
                    self.tabs.push(tab);
                    self.active_tab_id = new_id;
                    self.show_pdf_modal = true;
                    self.prompt_selected_ix = 0;
                }
                Err(err) => self.show_error(format!("Couldn't open PDF: {err}")),
            }
        } else {
            match read_note(&path) {
                Ok(content) => self.open_loaded_tab(name, content, Some(path), None),
                Err(err) => self.show_error(err),
            }
        }
    }

    pub fn handle_export(&mut self, format: &'static str, cx: &mut Context<Self>) {
        if let Some(message) = file_dialog_error() {
            self.show_error(message);
            return;
        }
        let Some(tab) = self.active_tab() else {
            return;
        };
        let body = match render_export(format, &tab.title, &tab.content) {
            Ok(body) => body,
            Err(err) => {
                self.show_error(err);
                return;
            }
        };
        let stem = export_stem(&tab.title);
        let ext = match format {
            "markdown" => "md",
            "text" => "txt",
            other => other,
        };
        let default_name = format!("{stem}.{ext}");
        let Some(dest) = pick_save_file(&default_name, ext) else {
            return;
        };
        match atomic_save(&dest, &body) {
            Ok(()) => self.show_toast(
                format!(
                    "Exported to {}",
                    dest.file_name().unwrap_or_default().to_string_lossy()
                ),
                cx,
            ),
            Err(err) => self.show_file_error(format!("Couldn't export: {err}")),
        }
    }

    pub fn handle_format_cmd(&mut self, cmd: &'static str, _cx: &mut App) {
        self.dispatch_editor_input(EditorInput::Format(cmd));
    }

    pub fn handle_insert_table(&mut self, _cx: &mut App) {
        if self.open_menu == Some(MenuId::Table) {
            self.open_menu = None;
            self.menu_selected_ix = None;
            self.menu_open_submenu_ix = None;
        } else {
            self.open_menu = Some(MenuId::Table);
            self.menu_selected_ix = None;
            self.menu_open_submenu_ix = None;
        }
    }

    pub fn handle_insert_table_size(&mut self, cols: usize, rows: usize, _cx: &mut App) {
        if self.block_table_drag_edit() {
            return;
        }
        if let Some(tab) = self.active_tab_mut() {
            tab.insert_table_with_size(cols, rows);
        }
        self.refresh_find();
    }

    pub fn handle_table_edit_action(&mut self, action: &'static str, _cx: &mut App) {
        if self.block_table_drag_edit() {
            return;
        }
        let anchor = self.table_action_anchor.take();
        if let Some(tab) = self.active_tab_mut() {
            match action {
                "row_above" => {
                    if let Some(pos) = anchor {
                        let _ = tab.apply_table_op_at(pos, TableOp::InsertRowAbove);
                    } else {
                        tab.table_add_row_above();
                    }
                }
                "row_below" => {
                    if let Some(pos) = anchor {
                        let _ = tab.apply_table_op_at(pos, TableOp::InsertRowBelow);
                    } else {
                        tab.table_add_row_below();
                    }
                }
                "col_left" => {
                    if let Some(pos) = anchor {
                        let _ = tab.apply_table_op_at(pos, TableOp::InsertColLeft);
                    } else {
                        tab.table_add_col_left();
                    }
                }
                "col_right" => {
                    if let Some(pos) = anchor {
                        let _ = tab.apply_table_op_at(pos, TableOp::InsertColRight);
                    } else {
                        tab.table_add_col_right();
                    }
                }
                "align_left" => {
                    if let Some(pos) = anchor {
                        let _ =
                            tab.apply_table_op_at(pos, TableOp::SetAlign(TableColumnAlign::Left));
                    } else {
                        tab.table_align_column(TableColumnAlign::Left);
                    }
                }
                "align_center" => {
                    if let Some(pos) = anchor {
                        let _ =
                            tab.apply_table_op_at(pos, TableOp::SetAlign(TableColumnAlign::Center));
                    } else {
                        tab.table_align_column(TableColumnAlign::Center);
                    }
                }
                "align_right" => {
                    if let Some(pos) = anchor {
                        let _ =
                            tab.apply_table_op_at(pos, TableOp::SetAlign(TableColumnAlign::Right));
                    } else {
                        tab.table_align_column(TableColumnAlign::Right);
                    }
                }
                "delete_row" => {
                    if let Some(pos) = anchor {
                        let _ = tab.apply_table_op_at(pos, TableOp::DeleteRow);
                    } else {
                        tab.table_delete_row();
                    }
                }
                "delete_col" => {
                    if let Some(pos) = anchor {
                        let _ = tab.apply_table_op_at(pos, TableOp::DeleteCol);
                    } else {
                        tab.table_delete_col();
                    }
                }
                "delete_table" => {
                    if let Some(pos) = anchor {
                        tab.table_delete_table_at(pos);
                    } else {
                        tab.table_delete_table();
                    }
                }
                _ => {}
            }
        }
        self.retarget_table_hover();
        self.refresh_find();
    }

    pub(super) fn retarget_table_hover(&mut self) {
        // The geometry under the pointer changes with the edit; the next move
        // relights whichever control it lands on.
        self.table_hot = None;
        let Some(hover) = self.table_hover.clone() else {
            return;
        };
        // The cached parse is the one the next frame draws, so this costs no
        // extra parse per keystroke.
        let next = self
            .tabs
            .iter_mut()
            .find(|t| t.id == self.active_tab_id)
            .and_then(|tab| retarget_hover(&tab.parsed(), &tab.content, &hover));
        self.table_hover = next;
    }

    pub fn handle_insert_link(&mut self, cx: &mut App) {
        self.handle_format_cmd("link", cx);
    }

    pub fn handle_toggle_show_code_files(&mut self) {
        self.settings.show_code_files = !self.settings.show_code_files;
        self.settings.save();
        self.rescan_workspace();
    }

    pub fn handle_toggle_sidebar(&mut self) {
        self.show_sidebar = !self.show_sidebar;
        if !self.show_sidebar {
            // Hand input back to the editor; the sidebar can no longer take it.
            self.sidebar_keyboard = false;
            if self.field_focus == TextInput::Search {
                self.field_focus = TextInput::Editor;
                self.field_owns_keys = false;
            }
            self.applied_focus = None;
        }
    }

    #[allow(dead_code)]
    pub fn handle_toggle_theme(&mut self) {
        self.settings.theme = self.settings.theme.toggle();
        self.settings.save();
    }

    pub fn handle_toggle_zen(&mut self) {
        self.zen_mode = !self.zen_mode;
        if self.zen_mode {
            self.zen_notice = true;
        }
    }

    pub fn handle_exit_zen(&mut self) {
        self.zen_mode = false;
        self.zen_notice = false;
    }

    pub fn handle_zoom_in(&mut self, cx: &mut Context<Self>) {
        self.settings.zoom_level = (self.settings.zoom_level + 10).min(200);
        self.settings.save();
        self.flash_zoom(cx);
    }

    pub fn handle_zoom_out(&mut self, cx: &mut Context<Self>) {
        self.settings.zoom_level = self.settings.zoom_level.saturating_sub(10).max(50);
        self.settings.save();
        self.flash_zoom(cx);
    }

    pub fn handle_reset_zoom(&mut self, cx: &mut Context<Self>) {
        self.settings.zoom_level = 100;
        self.settings.save();
        self.flash_zoom(cx);
    }

    pub fn handle_toggle_mode(&mut self) {
        if self.view_locked_raw() {
            return;
        }
        self.table_hover = None;
        self.table_drag = None;
        self.table_drag_press_held = false;
        self.dismiss_editor_context_menu();
        self.settings.view_mode = self.settings.view_mode.toggle();
        self.active_tab_mut();
        if self.settings.view_mode == ViewMode::Wysiwyg {
            if let Some(tab) = self.active_tab_mut() {
                tab.snap_wysiwyg_table_delimiter();
            }
        }
        self.settings.save();
    }

    pub fn handle_save_as(&mut self, cx: &mut Context<Self>) {
        if let Some(message) = file_dialog_error() {
            self.show_error(message);
            return;
        }
        let suggested_name = if let Some(tab) = self.active_tab() {
            if tab.title.contains('.') {
                tab.title.clone()
            } else {
                format!("{}.md", tab.title)
            }
        } else {
            "Untitled.md".to_string()
        };
        let Some(path) = pick_save_file(&suggested_name, "md") else {
            return;
        };
        let paths: Vec<Option<&Path>> = self
            .tabs
            .iter()
            .map(|tab| tab.file_path.as_deref())
            .collect();
        let active = self
            .tabs
            .iter()
            .position(|tab| tab.id == self.active_tab_id)
            .unwrap_or(0);
        if path_owned_by_other_tab(&paths, active, &path) {
            self.show_error("That path is already open in another tab.");
            return;
        }
        let content = self
            .active_tab()
            .map(|tab| tab.content.clone())
            .unwrap_or_default();
        match atomic_save(&path, &content) {
            Ok(()) => {
                let mut type_changed = false;
                if let Some(tab) = self.active_tab_mut() {
                    tab.file_path = Some(path.clone());
                    if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
                        tab.title = name.to_string();
                    }
                    type_changed = tab.set_file_type(FileType::from_filename(&tab.title));
                    tab.is_modified = false;
                    let title = tab.title.clone();
                    self.show_toast(format!("Saved {title}"), cx);
                }
                if type_changed {
                    self.refresh_find();
                }
                self.rescan_workspace();
            }
            Err(err) => self.show_file_error(format!("Couldn't save {}: {err}", path.display())),
        }
    }

    /// New folder goes inside the selected folder, else at the workspace root.
    pub fn handle_new_folder(&mut self, cx: &mut Context<Self>) {
        let parent = match &self.selected_target {
            Some(SidebarTarget::Folder(id)) => Some(id.clone()),
            _ => None,
        };
        self.begin_create(parent, true, cx);
    }

    /// Start inline creation of a note or folder in the sidebar tree.
    /// Nothing is written to disk until the name is committed with Enter.
    pub fn begin_create(
        &mut self,
        parent: Option<String>,
        is_folder: bool,
        cx: &mut Context<Self>,
    ) {
        // The inline editor lives in the sidebar tree — make sure it's visible
        // when creation is triggered from the menu bar or Ctrl+N.
        if !self.show_sidebar {
            self.show_sidebar = true;
        }
        if let Some(pid) = &parent {
            if let Some(folder) = self
                .workspace
                .folders
                .iter_mut()
                .find(|f| &f.id == pid)
            {
                folder.is_open = true;
            }
        }
        self.sidebar_menu = None;
        self.create_draft = Some(SidebarCreate { parent, is_folder });
        self.pending_create_focus = true;
        let preset = if is_folder { "New Folder" } else { "Untitled.md" };
        self.create_field.update(cx, |field, _cx| {
            field.set_text(preset);
        });
        cx.notify();
    }

    pub fn cancel_create(&mut self, cx: &mut Context<Self>) {
        self.create_draft = None;
        self.field_owns_keys = false;
        self.applied_focus = None;
        cx.notify();
    }

    /// Commit the inline create editor. On failure the editor stays open so
    /// the name can be fixed; an empty name cancels creation.
    ///
    /// `raw` is the text captured by the field before submit — it must not
    /// be re-read from the field here, which would re-entrantly borrow it.
    pub(super) fn commit_create(&mut self, raw: String, cx: &mut Context<Self>) {
        let Some(draft) = self.create_draft.take() else {
            return;
        };
        let raw = raw.trim().to_string();
        if raw.is_empty() {
            self.cancel_create(cx);
            return;
        }
        let result = if draft.is_folder {
            self.create_folder(draft.parent.clone(), raw)
        } else {
            self.create_note(draft.parent.clone(), raw)
        };
        if let Err(err) = result {
            self.create_draft = Some(draft);
            self.show_file_error(err);
        }
        cx.notify();
    }

    pub fn handle_toggle_word_wrap(&mut self) {
        self.settings.word_wrap = !self.settings.word_wrap;
        self.settings.save();
    }

    pub fn handle_set_content_alignment(&mut self, alignment: ContentAlignment) {
        if self.view_locked_raw() {
            return;
        }
        self.settings.content_alignment = alignment;
        self.settings.save();
    }

    pub fn handle_toggle_line_numbers(&mut self) {
        if self.view_locked_raw() {
            return;
        }
        self.settings.show_line_numbers = !self.settings.show_line_numbers;
        self.settings.save();
    }

    pub fn handle_set_view_mode(&mut self, mode: ViewMode) {
        if self.view_locked_raw() {
            return;
        }
        self.settings.view_mode = mode;
        self.active_tab_mut();
        if mode == ViewMode::Wysiwyg {
            if let Some(tab) = self.active_tab_mut() {
                tab.snap_wysiwyg_table_delimiter();
            }
        }
        self.settings.save();
    }

    pub fn handle_set_theme(&mut self, theme: ThemeMode) {
        self.settings.theme = theme;
        self.settings.save();
    }

    pub fn handle_set_font_size(&mut self, size: f32) {
        self.settings.base_font_size = size;
        self.settings.save();
    }

    /// Clipboard item for the active selection. Other apps get the text as
    /// the WYSIWYG view shows it; the Markdown source rides along as metadata
    /// so a paste back into a Markdown tab keeps the formatting.
    pub(super) fn selection_clipboard_item(&mut self) -> Option<ClipboardItem> {
        let tab = self.active_tab_mut()?;
        let markdown = tab.selected_text();
        if markdown.is_empty() {
            return None;
        }
        let visible = tab.selected_visible_text();
        Some(clipboard_item(visible, markdown))
    }

    pub fn handle_cut(&mut self, cx: &mut App) {
        if self.block_table_drag_edit() {
            return;
        }
        let Some(item) = self.selection_clipboard_item() else {
            return;
        };
        cx.write_to_clipboard(item);
        if self.active_tab().is_some_and(|tab| tab.is_read_only()) {
            self.show_error("PDF files are read-only.");
            return;
        }
        if let Some(tab) = self.active_tab_mut() {
            let range = tab.selection_range();
            tab.edit(range, "");
        }
        self.refresh_find();
    }

    pub fn handle_copy(&mut self, cx: &mut App) {
        if let Some(item) = self.selection_clipboard_item() {
            cx.write_to_clipboard(item);
        }
    }

    pub fn handle_paste(&mut self, cx: &mut App) {
        if self.block_table_drag_edit() {
            return;
        }
        if let Some(item) = cx.read_from_clipboard() {
            // A copy from a Markdown tab carries its source as metadata.
            let into_markdown = self
                .active_tab()
                .is_some_and(|tab| tab.file_type == FileType::Markdown);
            if let Some(text) = paste_text(&item, into_markdown) {
                if self.active_tab().is_some_and(|tab| tab.is_read_only()) {
                    self.show_error("PDF files are read-only.");
                    return;
                }
                if let Some(tab) = self.active_tab_mut() {
                    let range = tab.selection_range();
                    tab.edit(range, &text);
                }
                self.refresh_find();
            }
        }
    }

    pub fn handle_select_all(&mut self) {
        if self.input_blocked() {
            return;
        }
        if let Some(tab) = self.active_tab_mut() {
            tab.select_all();
        }
    }
}
