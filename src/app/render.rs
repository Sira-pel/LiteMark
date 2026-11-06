//! The window render tree.

use super::*;

impl Render for LiteMarkApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::resolve(self.settings.theme, window.appearance());
        self.sync_frame_state(&theme, window, cx);

        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == self.active_tab_id) {
            tab.ensure_stats();
        }
        // Borrow the tab only for the few fields the chrome reads. Cloning it
        // here copied its whole text and undo history on every frame.
        let fallback_tab;
        let active_tab = match self.active_tab() {
            Some(tab) => tab,
            None => {
                fallback_tab = DocumentTab::new(
                    "tab-1".to_string(),
                    "Welcome.md".to_string(),
                    INITIAL_MARKDOWN.to_string(),
                    None,
                    None,
                );
                &fallback_tab
            }
        };

        let active_tab_id = active_tab.id.clone();
        let active_tab_title = active_tab.title.clone();
        let active_has_selection = !active_tab.selection_range().is_empty();
        let active_formats = active_tab.active_formats;
        let stats_line = active_tab.cursor_line;
        let stats_words = active_tab.word_count;
        let stats_chars = active_tab.char_count;
        let stats_selection = {
            let text = active_tab
                .content
                .get(active_tab.selection_range())
                .unwrap_or("");
            (!text.is_empty()).then(|| text.split_whitespace().count())
        };
        let active_tab_has_pdf = active_tab.file_type == FileType::Pdf;
        let pdf_page_count = active_tab.pdf_page_count;
        let is_max = window.is_maximized();

        let view = cx.entity().downgrade();

        // Callbacks for HeaderBar
        let on_select_tab = {
            let view = view.clone();
            move |id: String, cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    let ids: Vec<&str> = this.tabs.iter().map(|tab| tab.id.as_str()).collect();
                    this.active_tab_id = active_after_click(&ids, &id, &this.active_tab_id);
                    this.sync_selection_to_active_tab();
                    this.refresh_find();
                    cx.notify();
                });
            }
        };

        let on_close_tab = {
            let view = view.clone();
            move |id: String, cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.handle_close_tab(id);
                    cx.notify();
                });
            }
        };

        let on_new_tab = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.handle_new_tab();
                    cx.notify();
                });
            }
        };

        let on_toggle_show_code_files = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.handle_toggle_show_code_files();
                    cx.notify();
                });
            }
        };

        let on_toggle_sidebar = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.handle_toggle_sidebar();
                    cx.notify();
                });
            }
        };

        let on_save = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.handle_save(cx);
                    cx.notify();
                });
            }
        };

        let on_open = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.handle_open_file();
                    cx.notify();
                });
            }
        };

        let on_export = {
            let view = view.clone();
            move |fmt: &'static str, cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.handle_export(fmt, cx);
                    cx.notify();
                });
            }
        };

        let on_toggle_zen = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.handle_toggle_zen();
                    cx.notify();
                });
            }
        };

        let on_open_shortcuts = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.show_shortcuts_modal = true;
                    this.prompt_selected_ix = 0;
                    cx.notify();
                });
            }
        };

        let on_zoom_in = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.handle_zoom_in(cx);
                    cx.notify();
                });
            }
        };

        let on_zoom_out = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.handle_zoom_out(cx);
                    cx.notify();
                });
            }
        };

        let on_reset_zoom = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.handle_reset_zoom(cx);
                    cx.notify();
                });
            }
        };

        let on_view_pdf = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.show_pdf_modal = true;
                    this.prompt_selected_ix = 0;
                    cx.notify();
                });
            }
        };

        let on_save_as = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.handle_save_as(cx);
                    cx.notify();
                });
            }
        };

        let on_new_folder = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.handle_new_folder(cx);
                    cx.notify();
                });
            }
        };

        let on_set_view_mode = {
            let view = view.clone();
            move |mode: ViewMode, cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.handle_set_view_mode(mode);
                    cx.notify();
                });
            }
        };

        let on_toggle_line_numbers = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.handle_toggle_line_numbers();
                    cx.notify();
                });
            }
        };

        let on_toggle_word_wrap = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.handle_toggle_word_wrap();
                    cx.notify();
                });
            }
        };

        let on_set_content_alignment = {
            let view = view.clone();
            move |alignment: ContentAlignment, cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.handle_set_content_alignment(alignment);
                    cx.notify();
                });
            }
        };

        let on_undo = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.handle_format_cmd("undo", cx);
                    cx.notify();
                });
            }
        };

        let on_redo = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.handle_format_cmd("redo", cx);
                    cx.notify();
                });
            }
        };

        let on_open_replace = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.show_find_replace = true;
                    this.refresh_find();
                    this.show_replace_row = true;
                    this.pending_find_focus = true;
                    this.open_menu = None;
                    cx.notify();
                });
            }
        };

        let on_set_theme = {
            let view = view.clone();
            move |mode: ThemeMode, cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.handle_set_theme(mode);
                    cx.notify();
                });
            }
        };

        let on_set_font_size = {
            let view = view.clone();
            move |size: f32, cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.handle_set_font_size(size);
                    cx.notify();
                });
            }
        };

        let on_toggle_update_checks = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.handle_toggle_update_checks();
                    cx.notify();
                });
            }
        };

        let on_check_updates_now = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.run_update_check(true, cx);
                });
            }
        };

        let on_open_about = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.open_menu = None;
                    this.show_about_modal = true;
                    this.prompt_selected_ix = 0;
                    cx.notify();
                });
            }
        };

        let on_close_about = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.show_about_modal = false;
                    cx.notify();
                });
            }
        };

        let on_open_menu = {
            let view = view.clone();
            move |id: MenuId, cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.open_menu = Some(id);
                    this.menu_selected_ix = None;
                    this.menu_open_submenu_ix = None;
                    cx.notify();
                });
            }
        };

        let on_close_menu = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.open_menu = None;
                    this.menu_selected_ix = None;
                    this.menu_open_submenu_ix = None;
                    cx.notify();
                });
            }
        };

        let on_toggle_heading = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    if this.open_menu == Some(MenuId::Heading) {
                        this.open_menu = None;
                    } else {
                        this.open_menu = Some(MenuId::Heading);
                        this.menu_selected_ix = None;
                        this.menu_open_submenu_ix = None;
                    }
                    cx.notify();
                });
            }
        };

        let on_toggle_overflow = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    if this.open_menu == Some(MenuId::Overflow) {
                        this.open_menu = None;
                    } else {
                        this.open_menu = Some(MenuId::Overflow);
                        this.menu_selected_ix = None;
                        this.menu_open_submenu_ix = None;
                    }
                    cx.notify();
                });
            }
        };

        let on_toggle_settings = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    if this.open_menu == Some(MenuId::Settings) {
                        this.open_menu = None;
                    } else {
                        this.open_menu = Some(MenuId::Settings);
                        this.menu_selected_ix = None;
                        this.menu_open_submenu_ix = None;
                    }
                    cx.notify();
                });
            }
        };

        let on_set_submenu = {
            let view = view.clone();
            move |ix: Option<usize>, cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    if this.menu_open_submenu_ix != ix {
                        this.menu_open_submenu_ix = ix;
                        this.submenu_selected_ix = None;
                        cx.notify();
                    }
                });
            }
        };

        let on_export_html = {
            let on_export = on_export.clone();
            move |cx: &mut App| on_export("html", cx)
        };
        let on_export_md = {
            let on_export = on_export.clone();
            move |cx: &mut App| on_export("markdown", cx)
        };
        let on_export_txt = {
            let on_export = on_export.clone();
            move |cx: &mut App| on_export("text", cx)
        };

        let on_close_active_tab = {
            let on_close_tab = on_close_tab.clone();
            let tab_id = active_tab_id.clone();
            move |cx: &mut App| on_close_tab(tab_id.clone(), cx)
        };

        let on_minimize = {
            move |window: &mut Window, _: &mut App| {
                window.minimize_window();
            }
        };

        let on_maximize = {
            move |window: &mut Window, _: &mut App| {
                window.zoom_window();
            }
        };

        let on_close_window = {
            let view = view.clone();
            move |_window: &mut Window, cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.request_close_window();
                    cx.notify();
                });
            }
        };

        // Callbacks for Formatting Toolbar
        let on_format = {
            let view = view.clone();
            move |cmd: &'static str, cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.handle_format_cmd(cmd, cx);
                    cx.notify();
                });
            }
        };

        let on_insert_table = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.handle_insert_table(cx);
                    cx.notify();
                });
            }
        };

        let on_table_hover = {
            let view = view.clone();
            move |cols: usize, rows: usize, cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.table_picker_size = (cols, rows);
                    cx.notify();
                });
            }
        };

        let on_table_pick = {
            let view = view.clone();
            move |cols: usize, rows: usize, cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.handle_insert_table_size(cols, rows, cx);
                    this.open_menu = None;
                    cx.notify();
                });
            }
        };

        let on_table_edit_action = {
            let view = view.clone();
            move |action: &'static str, cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.handle_table_edit_action(action, cx);
                    this.open_menu = None;
                    this.menu_open_submenu_ix = None;
                    cx.notify();
                });
            }
        };

        let on_insert_link = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.handle_insert_link(cx);
                    cx.notify();
                });
            }
        };

        let on_toggle_find = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.show_find_replace = !this.show_find_replace;
                    if this.show_find_replace {
                        this.refresh_find();
                        this.pending_find_focus = true;
                    } else {
                        this.refresh_find();
                        this.pending_find_focus = false;
                        this.applied_focus = None;
                        this.field_owns_keys = false;
                    }
                    cx.notify();
                });
            }
        };

        let on_find_next = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.find_next();
                    cx.notify();
                });
            }
        };
        let on_find_prev = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.find_previous();
                    cx.notify();
                });
            }
        };
        let on_replace = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.replace_current();
                    cx.notify();
                });
            }
        };
        let on_replace_all = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.replace_all_matches();
                    cx.notify();
                });
            }
        };
        let on_toggle_case = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.match_case = !this.match_case;
                    this.find_index = None;
                    this.refresh_find();
                    cx.notify();
                });
            }
        };

        let on_toggle_replace = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.show_replace_row = !this.show_replace_row;
                    cx.notify();
                });
            }
        };

        let on_find_close = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.show_find_replace = false;
                    this.refresh_find();
                    this.pending_find_focus = false;
                    this.applied_focus = None;
                    this.field_owns_keys = false;
                    cx.notify();
                });
            }
        };

        // Callbacks for Sidebar
        let on_select_file = {
            let view = view.clone();
            move |file: WorkspaceFile, cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    match read_note(&file.path) {
                        Ok(content) => this.open_loaded_tab(
                            file.name.clone(),
                            content,
                            Some(file.path.clone()),
                            Some(file.id.clone()),
                        ),
                        Err(err) => this.show_error(err),
                    }
                    // A sidebar click keeps arrow navigation in the tree, like
                    // clicking a folder; clicking the editor hands it back.
                    this.sidebar_keyboard = true;
                    cx.notify();
                });
            }
        };

        let on_range_select = {
            let view = view.clone();
            move |target: SidebarTarget, cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.extend_selection_to(target);
                    cx.notify();
                });
            }
        };

        let on_toggle_select = {
            let view = view.clone();
            move |target: SidebarTarget, cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.toggle_selection(target);
                    cx.notify();
                });
            }
        };

        let on_blank_click = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.multi_selection = None;
                    this.selected_target = None;
                    this.sidebar_keyboard = false;
                    cx.notify();
                });
            }
        };

        let on_begin_create = {
            let view = view.clone();
            move |parent_id: Option<String>, is_folder: bool, cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.begin_create(parent_id, is_folder, cx);
                    cx.notify();
                });
            }
        };

        let on_drop_item = {
            let view = view.clone();
            move |targets: Vec<SidebarTarget>, folder_id: Option<String>, copy: bool, cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.handle_drop_items(targets, folder_id, copy, cx);
                    cx.notify();
                });
            }
        };

        let on_delete_file = {
            let view = view.clone();
            move |id: String, cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.request_trash_file(id);
                    cx.notify();
                });
            }
        };

        let on_delete_folder = {
            let view = view.clone();
            move |id: String, cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.request_trash_folder(id);
                    cx.notify();
                });
            }
        };

        let on_toggle_folder = {
            let view = view.clone();
            move |folder_id: String, cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.toggle_folder(&folder_id);
                    cx.notify();
                });
            }
        };

        let on_sidebar_context = {
            let view = view.clone();
            move |target: SidebarTarget, position: Point<Pixels>, cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.sidebar_menu = Some(SidebarMenu { target, position });
                    this.prompt_selected_ix = 0;
                    cx.notify();
                });
            }
        };
        let on_sidebar_rename = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    if let Some(menu) = this.sidebar_menu.take() {
                        this.begin_rename(menu.target, cx);
                    }
                    cx.notify();
                });
            }
        };
        let on_sidebar_delete = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    if let Some(menu) = this.sidebar_menu.take() {
                        match menu.target {
                            SidebarTarget::File(id) => this.request_trash_file(id),
                            SidebarTarget::Folder(id) => this.request_trash_folder(id),
                        }
                    }
                    cx.notify();
                });
            }
        };
        let on_sidebar_dismiss = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.sidebar_menu = None;
                    cx.notify();
                });
            }
        };
        let on_close_context_menu = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.dismiss_editor_context_menu();
                    cx.notify();
                });
            }
        };
        let on_rename_cancel = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.cancel_rename(cx);
                });
            }
        };
        let on_rename_commit = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    if this.save_as_tab.is_some() {
                        this.commit_save_naming(None, cx);
                    } else {
                        this.commit_rename(None, cx);
                    }
                    cx.notify();
                });
            }
        };

        let on_toggle_workspace_menu = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.workspace_menu_open = !this.workspace_menu_open;
                    this.sidebar_menu = None;
                    cx.notify();
                });
            }
        };

        let on_choose_dir = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    if let Some(message) = file_dialog_error() {
                        this.show_error(message);
                    } else if let Some(dir) = pick_directory() {
                        this.prepare_workspace_switch(dir);
                    }
                    cx.notify();
                });
            }
        };

        let on_cut = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.handle_cut(cx);
                    cx.notify();
                });
            }
        };
        let on_copy = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.handle_copy(cx);
                    cx.notify();
                });
            }
        };
        let on_paste = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.handle_paste(cx);
                    cx.notify();
                });
            }
        };
        let on_select_all = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.handle_select_all();
                    cx.notify();
                });
            }
        };
        let on_dismiss_error = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.error_message = None;
                    cx.notify();
                });
            }
        };
        let on_prompt = {
            let view = view.clone();
            move |choice: &'static str, cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.answer_prompt(choice, cx);
                    cx.notify();
                });
            }
        };

        // Status bar callbacks
        let on_toggle_mode = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.handle_toggle_mode();
                    cx.notify();
                });
            }
        };

        // Modal callbacks
        let on_close_shortcuts = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.show_shortcuts_modal = false;
                    cx.notify();
                });
            }
        };

        let on_close_pdf = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.show_pdf_modal = false;
                    cx.notify();
                });
            }
        };

        let on_open_external_pdf = {
            let view = view.clone();
            move |cx: &mut App| {
                let path = view
                    .read_with(cx, |this, _| {
                        this.active_tab().and_then(|t| t.file_path.clone())
                    })
                    .ok()
                    .flatten();
                if let Some(path) = path {
                    cx.open_with_system(&path);
                }
            }
        };

        let on_exit_zen = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.handle_exit_zen();
                    cx.notify();
                });
            }
        };

        let window_width = window.viewport_size().width;
        let is_narrow = window_width < px(640.0);

        let menu_bar_element = render_menu_bar(
            MenuBarProps {
                open_menu: self.open_menu,
                is_narrow,
                on_open_menu: on_open_menu.clone(),
                on_close_menu: on_close_menu.clone(),
            },
            &theme,
        )
        .into_any_element();

        let settings_gear_element = render_settings_button(
            self.open_menu == Some(MenuId::Settings),
            self.available_update.is_some(),
            on_toggle_settings.clone(),
            &theme,
        )
        .into_any_element();

        let menu_overlay: Option<AnyElement> = if let Some(open_id) = self.open_menu {
            if open_id == MenuId::Table {
                let table_menu_x = if is_narrow {
                    (window.viewport_size().width / 2.0 + px(128.0))
                        .min(window.viewport_size().width - px(200.0))
                } else {
                    (window.viewport_size().width / 2.0 + px(180.0))
                        .min(window.viewport_size().width - px(200.0))
                };
                let point = Point::new(table_menu_x.max(px(8.0)), px(76.0));
                let in_table = active_formats.table;
                Some(
                    render_table_picker_menu(
                        TablePickerMenuProps {
                            point,
                            hovered_size: self.table_picker_size,
                            in_table,
                            open_submenu_ix: self.menu_open_submenu_ix,
                            focus_handle: self.menu_focus_handle.clone(),
                            on_hover_size: on_table_hover.clone(),
                            on_pick_size: on_table_pick.clone(),
                            on_edit_action: on_table_edit_action.clone(),
                            on_close: on_close_menu.clone(),
                            on_set_submenu: on_set_submenu.clone(),
                        },
                        &theme,
                    )
                    .into_any_element(),
                )
            } else {
                let (point, anchor, width, items) = match open_id {
                MenuId::File => (
                    Point::new(px(8.0), px(76.0)),
                    Corner::TopLeft,
                    px(220.0),
                    build_file_menu_items(
                        active_tab_has_pdf,
                        // "New note" starts the inline sidebar editor so the
                        // user names the file, not an auto "Untitled-N" tab.
                        {
                            let bc = on_begin_create.clone();
                            move |cx: &mut App| bc(None, false, cx)
                        },
                        on_new_folder.clone(),
                        on_open.clone(),
                        on_choose_dir.clone(),
                        on_save.clone(),
                        on_save_as.clone(),
                        on_export_html.clone(),
                        on_export_md.clone(),
                        on_export_txt.clone(),
                        on_view_pdf.clone(),
                        on_close_active_tab.clone(),
                    ),
                ),
                MenuId::Edit => (
                    Point::new(px(48.0), px(76.0)),
                    Corner::TopLeft,
                    px(200.0),
                    build_edit_menu_items(
                        on_undo.clone(),
                        on_redo.clone(),
                        on_cut.clone(),
                        on_copy.clone(),
                        on_paste.clone(),
                        on_select_all.clone(),
                        on_toggle_find.clone(),
                        on_open_replace.clone(),
                    ),
                ),
                MenuId::View => (
                    Point::new(px(92.0), px(76.0)),
                    Corner::TopLeft,
                    px(220.0),
                    build_view_menu_items(
                        self.settings.view_mode,
                        self.view_locked_raw(),
                        self.settings.show_line_numbers,
                        self.settings.word_wrap,
                        self.show_sidebar,
                        self.settings.show_code_files,
                        self.settings.content_alignment,
                        on_set_view_mode.clone(),
                        on_toggle_line_numbers.clone(),
                        on_toggle_word_wrap.clone(),
                        on_set_content_alignment.clone(),
                        on_zoom_in.clone(),
                        on_zoom_out.clone(),
                        on_reset_zoom.clone(),
                        on_toggle_sidebar.clone(),
                        on_toggle_show_code_files.clone(),
                        on_toggle_zen.clone(),
                    ),
                ),
                MenuId::Settings => (
                    Point::new(window.viewport_size().width - px(8.0), px(76.0)),
                    Corner::TopRight,
                    px(220.0),
                    build_settings_menu_items(
                        self.settings.theme,
                        self.settings.base_font_size,
                        on_set_theme.clone(),
                        on_set_font_size.clone(),
                        on_open_shortcuts.clone(),
                        on_open_about.clone(),
                        self.settings.check_for_updates,
                        on_toggle_update_checks.clone(),
                        on_check_updates_now.clone(),
                    ),
                ),
                MenuId::Hamburger => {
                    let file_items = build_file_menu_items(
                        active_tab_has_pdf,
                        // "New note" starts the inline sidebar editor, same as
                        // the File menu.
                        {
                            let bc = on_begin_create.clone();
                            move |cx: &mut App| bc(None, false, cx)
                        },
                        on_new_folder.clone(),
                        on_open.clone(),
                        on_choose_dir.clone(),
                        on_save.clone(),
                        on_save_as.clone(),
                        on_export_html.clone(),
                        on_export_md.clone(),
                        on_export_txt.clone(),
                        on_view_pdf.clone(),
                        on_close_active_tab.clone(),
                    );
                    let edit_items = build_edit_menu_items(
                        on_undo.clone(),
                        on_redo.clone(),
                        on_cut.clone(),
                        on_copy.clone(),
                        on_paste.clone(),
                        on_select_all.clone(),
                        on_toggle_find.clone(),
                        on_open_replace.clone(),
                    );
                    let view_items = build_view_menu_items(
                        self.settings.view_mode,
                        self.view_locked_raw(),
                        self.settings.show_line_numbers,
                        self.settings.word_wrap,
                        self.show_sidebar,
                        self.settings.show_code_files,
                        self.settings.content_alignment,
                        on_set_view_mode.clone(),
                        on_toggle_line_numbers.clone(),
                        on_toggle_word_wrap.clone(),
                        on_set_content_alignment.clone(),
                        on_zoom_in.clone(),
                        on_zoom_out.clone(),
                        on_reset_zoom.clone(),
                        on_toggle_sidebar.clone(),
                        on_toggle_show_code_files.clone(),
                        on_toggle_zen.clone(),
                    );
                    let items = vec![
                        MenuItem::submenu("File", None, file_items),
                        MenuItem::submenu("Edit", None, edit_items),
                        MenuItem::submenu("View", None, view_items),
                    ];
                    (
                        Point::new(px(8.0), px(76.0)),
                        Corner::TopLeft,
                        px(180.0),
                        items,
                    )
                }
                MenuId::Heading => (
                    Point::new(
                        px(heading_button_left(f32::from(window.viewport_size().width))),
                        px(76.0),
                    ),
                    Corner::TopLeft,
                    px(190.0),
                    build_heading_menu_items(on_format.clone(), on_close_menu.clone()),
                ),
                MenuId::Overflow => {
                    let overflow_x = if is_narrow {
                        (window.viewport_size().width / 2.0 + px(128.0))
                            .min(window.viewport_size().width - px(8.0))
                    } else {
                        (window.viewport_size().width / 2.0 + px(wide_center_width() / 2.0))
                            .min(window.viewport_size().width - px(8.0))
                    };
                    (
                        Point::new(
                            overflow_x.min(window.viewport_size().width - px(8.0)),
                            px(76.0),
                        ),
                        Corner::TopRight,
                        px(210.0),
                        build_overflow_menu_items(
                            is_narrow,
                            on_format.clone(),
                            on_insert_table.clone(),
                            on_toggle_find.clone(),
                            on_close_menu.clone(),
                        ),
                    )
                }
                MenuId::Table => unreachable!(),
            };

            // Cache the built items so keyboard navigation (SelectNext /
            // Confirm) can act on them between renders.
            self.menu_items_cache = items.clone();

            Some(
                    render_dropdown_menu(
                        MenuProps {
                            id: ("menu-dropdown", open_id as usize).into(),
                            point,
                            anchor_corner: anchor,
                            width,
                            max_width: window.viewport_size().width - px(16.0),
                            items,
                            focus_handle: self.menu_focus_handle.clone(),
                            selected_index: self.menu_selected_ix,
                            open_submenu_ix: self.menu_open_submenu_ix,
                            submenu_selected_index: self.submenu_selected_ix,
                            on_close: on_close_menu.clone(),
                            on_set_submenu: on_set_submenu.clone(),
                        },
                        &theme,
                    )
                .into_any_element(),
            )
            }
        } else {
            None
        };

        let root = div()
            .id("litemark-root")
            .track_focus(&self.focus_handle)
            .key_context("Editor")
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .bg(theme.bg_editor)
            .text_color(theme.text_primary)
            .font_family(Theme::FONT_PROSE);
        Self::bind_key_actions(root, &view, cx)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    this.editor_mouse_down(event, window, cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    this.editor_right_mouse_down(event, window, cx);
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _window, cx| {
                this.editor_mouse_move(event, cx);
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, event: &MouseUpEvent, window, cx| {
                    this.editor_mouse_up(event, window, cx);
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, event: &MouseUpEvent, window, cx| {
                    this.editor_mouse_up(event, window, cx);
                }),
            )
            .on_mouse_down_out(cx.listener(|this, _event: &MouseDownEvent, _window, cx| {
                this.handle_mouse_down_out();
                cx.notify();
            }))
            .child({
                let view = cx.weak_entity();
                canvas(
                    |_, _, _| (),
                    move |_, (), window, _cx| {
                        window.on_mouse_event(move |event: &MouseExitEvent, phase, _window, cx| {
                            if phase != DispatchPhase::Capture {
                                return;
                            }
                            let _ = view.update(cx, |this, cx| {
                                this.handle_mouse_exit(event);
                                cx.notify();
                            });
                        });
                    },
                )
                .absolute()
                .size_full()
            })
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, _window, cx| {
                if this.table_drag.is_none() && !this.table_drag_press_held {
                    return;
                }
                let typed = event.keystroke.key_char.as_deref().unwrap_or("");
                let (drag, insert) =
                    clear_drag_for_key(this.table_drag.take(), &event.keystroke.key, typed);
                this.table_drag = drag;
                if insert.is_empty() || this.table_drag_press_held {
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            .on_modifiers_changed(cx.listener(|this, event: &ModifiersChangedEvent, _window, cx| {
                if this.table_drag.is_none() || !event.modifiers.modified() {
                    return;
                }
                this.table_drag = clear_drag_for_modifier(this.table_drag.take());
                cx.stop_propagation();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &CloseModal, window, cx| {
                this.handle_close_modal(window, cx);
            }))
            // Main App View
            .child(if !self.zen_mode {
                div()
                    .flex()
                    .flex_col()
                    .size_full()
                    // 1. HeaderBar
                    .child(render_header_bar(
                        HeaderBarProps {
                            tabs: self
                                .tabs
                                .iter()
                                .map(|t| HeaderTab {
                                    id: t.id.clone(),
                                    title: t.title.clone(),
                                    is_modified: t.is_modified,
                                    is_code: t.file_type.is_code(),
                                })
                                .collect(),
                            active_tab_id: self.active_tab_id.clone(),
                            is_sidebar_open: self.show_sidebar,
                            is_maximized: is_max,
                            on_select_tab: on_select_tab.clone(),
                            on_close_tab: on_close_tab.clone(),
                            on_new_tab: on_new_tab.clone(),
                            on_toggle_sidebar: on_toggle_sidebar.clone(),
                            on_minimize,
                            on_maximize,
                            on_close_window,
                            drag_armed: self.title_drag_armed.clone(),
                        },
                        &theme,
                    ))
                    // 2. Formatting Toolbar
                    .child(render_formatting_toolbar(
                        FormattingToolbarProps {
                            active_formats,
                            show_overflow_menu: self.open_menu == Some(MenuId::Overflow),
                            show_heading_menu: self.open_menu == Some(MenuId::Heading),
                            show_table_menu: self.open_menu == Some(MenuId::Table),
                            is_narrow,
                            menu_bar_element,
                            settings_gear_element,
                            on_format: on_format.clone(),
                            on_insert_table: on_insert_table.clone(),
                            on_insert_link: on_insert_link.clone(),
                            on_toggle_find: on_toggle_find.clone(),
                            on_toggle_overflow: on_toggle_overflow.clone(),
                            on_toggle_heading: on_toggle_heading.clone(),
                        },
                        &theme,
                    ))
                    // 3. Find & Replace Bar (Overlay if enabled)
                    .when(self.show_find_replace, |s| {
                        s.child(render_find_replace_bar(
                            FindReplaceBarProps {
                                find_field: self.find_field.clone(),
                                replace_field: self.replace_field.clone(),
                                match_label: self.match_label(),
                                match_case: self.match_case,
                                not_found: self.not_found,
                                on_find_next: on_find_next.clone(),
                                on_find_prev: on_find_prev.clone(),
                                on_replace: on_replace.clone(),
                                on_replace_all: on_replace_all.clone(),
                                on_close: on_find_close.clone(),
                                on_toggle_case: on_toggle_case.clone(),
                                show_replace: self.show_replace_row,
                                width: window_width,
                                on_toggle_replace: on_toggle_replace.clone(),
                            },
                            &theme,
                        ))
                    })
                    // 4. Middle Area: Sidebar + Editor
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .size_full()
                            .relative()
                            .overflow_hidden()
                            .when(self.show_sidebar, |s| {
                                let sidebar = render_sidebar(
                                    SidebarProps {
                                        workspace: &self.workspace,
                                        width: crate::theme::sidebar_width(window_width),
                                        selected_file_id: match &self.selected_target {
                                            Some(SidebarTarget::File(id)) => Some(id.clone()),
                                            _ => None,
                                        },
                                        open_file_id: self
                                            .active_tab()
                                            .and_then(|tab| tab.workspace_file_id.clone()),
                                        selected_folder_id: match &self.selected_target {
                                            Some(SidebarTarget::Folder(id)) => {
                                                Some(id.clone())
                                            }
                                            _ => None,
                                        },
                                        focus_handle: self.sidebar_focus_handle.clone(),
                                        modified_file_ids: self
                                            .tabs
                                            .iter()
                                            .filter(|t| t.is_modified)
                                            .filter_map(|t| t.workspace_file_id.clone())
                                            .collect(),
                                        search_query: self.search_query.clone(),
                                        creating: self.create_draft.clone(),
                                        create_field: self.create_field.clone(),
                                        search_field: self.search_field.clone(),
                                        dragging: cx.has_active_drag(),
                                        on_select_file: on_select_file.clone(),
                                        on_begin_create: on_begin_create.clone(),
                                        on_delete_file: on_delete_file.clone(),
                                        on_delete_folder: on_delete_folder.clone(),
                                        on_toggle_folder: on_toggle_folder.clone(),
                                        on_workspace_menu: on_toggle_workspace_menu.clone(),
                                        workspace_menu_open: self.workspace_menu_open,
                                        on_close: on_toggle_sidebar.clone(),
                                        on_context_menu: on_sidebar_context.clone(),
                                        on_drop_item: on_drop_item.clone(),
                                        on_range_select: on_range_select.clone(),
                                        on_toggle_select: on_toggle_select.clone(),
                                        on_blank_click: on_blank_click.clone(),
                                        multi_targets: self
                                            .active_multi()
                                            .map(|m| m.items.clone())
                                            .unwrap_or_default(),
                                    },
                                    &theme,
                                );
                                // On narrow windows the sidebar floats over the
                                // editor as a drawer instead of squeezing it.
                                if window_width < px(600.0) {
                                    let close = on_toggle_sidebar.clone();
                                    s.child(deferred(
                                        div()
                                            .absolute()
                                            .top_0()
                                            .left_0()
                                            .size_full()
                                            // A click on the editor beside the drawer closes it.
                                            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                                                cx.stop_propagation();
                                                close(cx);
                                            })
                                            .child(
                                                div()
                                                    .h_full()
                                                    .occlude()
                                                    .shadow_2xl()
                                                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                                        cx.stop_propagation()
                                                    })
                                                    .child(sidebar),
                                            ),
                                    ))
                                } else {
                                    s.child(sidebar)
                                }
                            })
                            .child(render_editor(
                                EditorProps {
                                    zoom_level: self.settings.zoom_level,
                                    show_zoom_hud: self.show_zoom_hud,
                                    scroll: self.editor_scroll.clone(),
                                    on_scroll: Some(Box::new(cx.listener(
                                        |this, ev: &ScrollWheelEvent, window, cx| {
                                            this.note_scroll(ev, window, cx)
                                        },
                                    ))),
                                    editor_element: self.editor_surface(
                                        cx,
                                        &theme,
                                        self.settings.show_line_numbers,
                                    ),
                                },
                                &theme,
                            ))
                            .when(self.workspace_menu_open && self.show_sidebar, |parent| {
                let recents: Vec<(String, PathBuf)> = self
                    .settings
                    .recent_folders
                    .iter()
                    .filter(|p| p.is_dir() && **p != self.workspace.folder_path)
                    .map(|p| {
                        let name = p
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("Folder")
                            .to_string();
                        (name, p.clone())
                    })
                    .collect();
                let view = view.clone();
                parent.child(render_workspace_menu(
                    self.workspace.folder_path.display().to_string(),
                    recents,
                    {
                        let view = view.clone();
                        move |dir: PathBuf, cx: &mut App| {
                            let _ = view.update(cx, |this, cx| {
                                this.workspace_menu_open = false;
                                this.prepare_workspace_switch(dir);
                                cx.notify();
                            });
                        }
                    },
                    {
                        let view = view.clone();
                        let choose = on_choose_dir.clone();
                        move |cx: &mut App| {
                            let _ = view.update(cx, |this, cx| {
                                this.workspace_menu_open = false;
                                cx.notify();
                            });
                            choose(cx);
                        }
                    },
                    {
                        let view = view.clone();
                        move |cx: &mut App| {
                            let _ = view.update(cx, |this, cx| {
                                this.workspace_menu_open = false;
                                cx.notify();
                            });
                        }
                    },
                    &theme,
                ))
            })
                    )
                    // 5. StatusBar
                    .child(render_status_bar(
                        StatusBarProps {
                            line: stats_line,
                            words: stats_words,
                            chars: stats_chars,
                            selected_words: stats_selection,
                            width: window_width,
                            zoom_level: self.settings.zoom_level,
                            view_mode: self.effective_view_mode(),
                            on_zoom_in: on_zoom_in.clone(),
                            on_zoom_out: on_zoom_out.clone(),
                            on_reset_zoom: on_reset_zoom.clone(),
                            on_toggle_mode: on_toggle_mode.clone(),
                        },
                        &theme,
                    ))
            } else {
                // Zen Mode View
                div()
                    .size_full()
                    .relative()
                    .child(render_editor(
                        EditorProps {
                            zoom_level: self.settings.zoom_level,
                            show_zoom_hud: self.show_zoom_hud,
                            scroll: self.editor_scroll.clone(),
                            on_scroll: Some(Box::new(cx.listener(
                                |this, ev: &ScrollWheelEvent, window, cx| this.note_scroll(ev, window, cx),
                            ))),
                            editor_element: self.editor_surface(cx, &theme, false),
                        },
                        &theme,
                    ))
                    .child({
                        let exit_z = on_exit_zen.clone();
                        div()
                            .id("zen-exit-btn")
                            .absolute()
                            .top(px(16.0))
                            .right(px(24.0))
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .px(px(12.0))
                            .py(px(6.0))
                            .rounded(px(8.0))
                            .bg(theme.bg_surface)
                            .border_1()
                            .border_color(theme.border_primary)
                            .shadow_xl()
                            .cursor_pointer()
                            .hover(|h| h.bg(theme.bg_hover))
                            .on_click(move |_, _, cx| exit_z(cx))
                            .child(render_icon("minimize-2", 14.0, theme.text_muted))
                            .child("Exit Zen Mode")
                            .child(kbd_badge("Esc", &theme))
                    })
            })
            // Update notice
            .when_some(
                self.available_update.clone().filter(|_| !self.zen_mode),
                |s, release| {
                    let url = release.url.clone();
                    let version = release.version.clone();
                    let view = cx.entity();
                    s.child(
                        div()
                            .absolute()
                            .top(px(76.0))
                            .right(px(16.0))
                            .w(px(300.0))
                            .p(px(14.0))
                            .rounded(px(10.0))
                            .bg(theme.bg_surface)
                            .border_1()
                            .border_color(theme.border_primary)
                            .shadow_2xl()
                            .flex()
                            .flex_col()
                            .gap(px(6.0))
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .text_color(theme.text_strong)
                                    .child(format!("LiteMark {} is available", release.version)),
                            )
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .text_color(theme.text_muted)
                                    .child(format!("Current: {}", current_version())),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .mt(px(6.0))
                                    .child(
                                        div()
                                            .id("update-download")
                                            .px(px(14.0))
                                            .py(px(6.0))
                                            .rounded(px(6.0))
                                            .cursor_pointer()
                                            .text_size(px(13.0))
                                            .text_color(theme.text_strong)
                                            .bg(theme.bg_active)
                                            .hover(|h| h.bg(theme.selection_bg))
                                            .on_click(move |_, _, cx| cx.open_url(&url))
                                            .child("Download"),
                                    )
                                    .child(
                                        div()
                                            .id("update-dismiss")
                                            .px(px(8.0))
                                            .py(px(6.0))
                                            .rounded(px(6.0))
                                            .cursor_pointer()
                                            .text_size(px(13.0))
                                            .text_color(theme.text_muted)
                                            .hover(|h| h.bg(theme.bg_hover))
                                            .on_click(move |_, _, cx| {
                                                let version = version.clone();
                                                view.update(cx, |this, cx| {
                                                    this.settings.dismissed_update = Some(version);
                                                    this.settings.save();
                                                    this.available_update = None;
                                                    cx.notify();
                                                });
                                            })
                                            .child("Dismiss"),
                                    ),
                            ),
                    )
                },
            )
            // Save Toast Notification
            .when_some(self.saved_toast.clone(), |s, toast| {
                s.child(
                    div()
                        .absolute()
                        .bottom(px(40.0))
                        .right(px(24.0))
                        .px(px(14.0))
                        .py(px(8.0))
                        .rounded(px(8.0))
                        .bg(theme.bg_surface)
                        .border_1()
                        .border_color(theme.border_primary)
                        .shadow_2xl()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .text_size(px(12.0))
                        .text_color(theme.text_primary)
                        .child(
                            div()
                                .w(px(8.0))
                                .h(px(8.0))
                                .rounded_full()
                                .bg(theme.text_faint),
                        )
                        .child(toast),
                )
            })
            // Modals
            .when(self.show_shortcuts_modal, |s| {
                s.child(render_shortcuts_modal(
                    (window.viewport_size().height - px(32.0)).min(px(560.0)),
                    self.prompt_focus_handle.clone(),
                    on_close_shortcuts.clone(),
                    &theme,
                ))
            })
            .when(self.show_pdf_modal, |s| {
                let title = active_tab_title.clone();
                s.child(render_pdf_preview_modal(
                    title,
                    pdf_page_count,
                    self.prompt_selected_ix,
                    self.prompt_focus_handle.clone(),
                    on_close_pdf.clone(),
                    on_open_external_pdf.clone(),
                    &theme,
                ))
            })
            .when(self.show_about_modal, |s| {
                s.child(render_about_modal(
                    self.prompt_focus_handle.clone(),
                    on_close_about.clone(),
                    &theme,
                ))
            })
            .when_some(menu_overlay, |s, overlay| s.child(overlay))
            .when_some(self.error_message.clone(), |s, message| {
                let dismiss = on_dismiss_error.clone();
                s.child(
                    div()
                        .absolute()
                        .top(px(88.0))
                        .left(px(24.0))
                        .right(px(24.0))
                        .px(px(14.0))
                        .py(px(10.0))
                        .rounded(px(8.0))
                        .bg(theme.bg_surface)
                        .border_1()
                        .border_color(theme.border_surface)
                        .shadow_lg()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(12.0))
                        .child(
                            div()
                                .flex_1()
                                .text_size(px(13.0))
                                .text_color(theme.text_primary)
                                .child(message),
                        )
                        .child(
                            div()
                                .id("dismiss-error")
                                .px(px(10.0))
                                .py(px(4.0))
                                .rounded(px(6.0))
                                .cursor_pointer()
                                .text_size(px(12.0))
                                .bg(theme.bg_hover)
                                .child("Dismiss")
                                .on_click(move |_, _, cx| dismiss(cx)),
                        ),
                )
            })
            .when_some(self.prompt.clone(), |parent, prompt| {
                let (title, message, choices) = self.prompt_modal_data(&prompt);
                parent.child(render_prompt_modal(
                    title,
                    message,
                    choices,
                    self.prompt_selected_ix,
                    self.prompt_focus_handle.clone(),
                    on_prompt.clone(),
                    &theme,
                ))
            })
            .child(render_client_resize_borders(window))
            .when_some(self.sidebar_menu.clone(), |parent, menu| {
                parent.child(render_sidebar_context_menu(
                    &menu,
                    self.prompt_selected_ix,
                    self.prompt_focus_handle.clone(),
                    on_sidebar_rename.clone(),
                    on_sidebar_delete.clone(),
                    on_sidebar_dismiss.clone(),
                    &theme,
                ))
            })
            .when_some(self.editor_context_menu.clone(), |parent, menu| {
                let has_selection = active_has_selection;
                let items = if menu.is_table {
                    build_table_context_menu_items(
                        has_selection,
                        on_cut.clone(),
                        on_copy.clone(),
                        on_paste.clone(),
                        on_table_edit_action.clone(),
                    )
                } else {
                    build_editor_context_menu_items(
                        has_selection,
                        on_cut.clone(),
                        on_copy.clone(),
                        on_paste.clone(),
                        on_select_all.clone(),
                    )
                };
                parent.child(render_context_menu(
                    "editor-context-menu".into(),
                    menu.position,
                    px(180.0),
                    items,
                    self.menu_focus_handle.clone(),
                    on_close_context_menu.clone(),
                    &theme,
                ))
            })
            .when(
                self.rename_target.is_some() || self.save_as_tab.is_some(),
                |parent| {
                    let cancel = on_rename_cancel.clone();
                    let commit = on_rename_commit.clone();
                    let backdrop_cancel = cancel.clone();
                    let is_save = self.save_as_tab.is_some();
                    let is_folder =
                        matches!(self.rename_target, Some(SidebarTarget::Folder(_)));
                    let title = if is_save {
                        "Save note"
                    } else if is_folder {
                        "Rename Folder"
                    } else {
                        "Rename Note"
                    };
                    parent.child(
                    div()
                        .absolute()
                        .size_full()
                        .top_0()
                        .left_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(hsla(0.0, 0.0, 0.0, 0.35))
                        .on_mouse_down(MouseButton::Left, move |_, _, cx| backdrop_cancel(cx))
                        .child(
                            div()
                                .w(px(360.0))
                                .p(px(16.0))
                                .flex()
                                .flex_col()
                                .gap(px(10.0))
                                .rounded(px(8.0))
                                .bg(theme.menu_bg)
                                .border_1()
                                .border_color(theme.border_surface)
                                .shadow_lg()
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                .child(
                                    div()
                                        .text_size(px(14.0))
                                        .font_weight(FontWeight::MEDIUM)
                                        .child(title),
                                )
                                .child(self.rename_field.clone())
                                .child(
                                    div()
                                        .flex()
                                        .justify_end()
                                        .gap(px(8.0))
                                        .child(
                                            div()
                                                .id("rename-cancel")
                                                .px(px(10.0))
                                                .py(px(4.0))
                                                .rounded(px(6.0))
                                                .cursor_pointer()
                                                .child("Cancel")
                                                .on_click(move |_, _, cx| cancel(cx)),
                                        )
                                        .child(
                                            div()
                                                .id("rename-commit")
                                                .px(px(10.0))
                                                .py(px(4.0))
                                                .rounded(px(6.0))
                                                .cursor_pointer()
                                                .bg(theme.bg_hover)
                                                .child(if is_save { "Save" } else { "Rename" })
                                                .on_click(move |_, _, cx| commit(cx)),
                                        ),
                                ),
                        ),
                )
                },
            )
    }
}

impl LiteMarkApp {
    /// Per-frame state sync before the tree is built: window close, caret
    /// follow, which focus slot owns the keyboard, and line-field colors.
    fn sync_frame_state(&mut self, theme: &Theme, window: &mut Window, cx: &mut Context<Self>) {
        if self.follow_caret() == Some(true) {
            window.request_animation_frame();
        }
        self.coast_scroll(window);
        self.install_close_hook(window, cx);
        if self.allow_window_close {
            window.remove_window();
        }
        if !self.show_find_replace
            && matches!(self.field_focus, TextInput::Find | TextInput::Replace)
        {
            self.field_focus = TextInput::Editor;
            self.field_owns_keys = false;
            self.applied_focus = None;
        }
        if self.rename_target.is_none()
            && self.save_as_tab.is_none()
            && self.field_focus == TextInput::Rename
        {
            self.field_focus = TextInput::Editor;
            self.field_owns_keys = false;
            self.applied_focus = None;
        }
        if self.create_draft.is_none() && self.field_focus == TextInput::Create {
            self.field_focus = TextInput::Editor;
            self.field_owns_keys = false;
            self.applied_focus = None;
        }
        if self.pending_find_focus && self.show_find_replace {
            self.pending_find_focus = false;
            self.field_focus = TextInput::Find;
        }
        if self.pending_rename_focus && (self.rename_target.is_some() || self.save_as_tab.is_some())
        {
            self.pending_rename_focus = false;
            self.field_focus = TextInput::Rename;
        }
        if self.pending_create_focus && self.create_draft.is_some() {
            self.pending_create_focus = false;
            self.field_focus = TextInput::Create;
        }
        let modal_keyboard = self.modal_choice_count() > 0;
        let next_focus = if self.open_menu.is_some() {
            FocusSlot::Menu
        } else if modal_keyboard {
            FocusSlot::Prompt
        } else if self.sidebar_keyboard {
            FocusSlot::Sidebar
        } else {
            FocusSlot::Editor
        };
        if self.open_menu.is_some() {
            if focus_if_changed(self.applied_focus, next_focus) {
                self.menu_focus_handle.focus(window);
                self.applied_focus = Some(FocusSlot::Menu);
            }
        } else if modal_keyboard {
            if focus_if_changed(self.applied_focus, FocusSlot::Prompt) {
                self.prompt_focus_handle.focus(window);
                self.applied_focus = Some(FocusSlot::Prompt);
            }
        } else if self.sidebar_keyboard {
            if focus_if_changed(self.applied_focus, FocusSlot::Sidebar) {
                self.sidebar_focus_handle.focus(window);
                self.applied_focus = Some(FocusSlot::Sidebar);
            }
        } else if let Some(field) = self.line_field_for_focus() {
            // Apply field focus after the editor slot so a clicked Replace
            // field keeps the input handler, the same way Find does.
            field.read(cx).focus(window);
            self.applied_focus = None;
            self.field_owns_keys = true;
        } else if focus_if_changed(self.applied_focus, FocusSlot::Editor) {
            self.focus_handle.focus(window);
            self.applied_focus = Some(FocusSlot::Editor);
            self.field_focus = TextInput::Editor;
            self.field_owns_keys = false;
        }
        if self.text_field_focused(window, cx) {
            self.field_owns_keys = true;
        }
        let text_color = theme.text_primary;
        let placeholder_color = theme.placeholder;
        let field_bg = theme.bg_input;
        let field_border = theme.border_primary;
        self.find_field.update(cx, |field, _cx| {
            field.set_colors(text_color, placeholder_color, field_bg, field_border);
        });
        self.replace_field.update(cx, |field, _cx| {
            field.set_colors(text_color, placeholder_color, field_bg, field_border);
        });
        self.search_field.update(cx, |field, _cx| {
            field.set_colors(text_color, placeholder_color, rgba(0x00000000), rgba(0x00000000));
        });
        self.create_field.update(cx, |field, _cx| {
            field.set_colors(text_color, placeholder_color, field_bg, field_border);
        });
    }
}
