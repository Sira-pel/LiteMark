//! Keyboard shortcut bindings on the root element.

use super::*;

impl LiteMarkApp {
    /// Registers the root element's keyboard action handlers.
    pub(super) fn bind_key_actions(
        root: Stateful<Div>,
        view: &WeakEntity<Self>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        root
            .on_action(cx.listener(|this, _: &ToggleBold, window, cx| {
                this.handle_format_cmd("bold", cx);
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ToggleItalic, window, cx| {
                this.handle_format_cmd("italic", cx);
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ToggleUnderline, window, cx| {
                this.handle_format_cmd("underline", cx);
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ToggleStrikethrough, window, cx| {
                this.handle_format_cmd("strike", cx);
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &Heading1, window, cx| {
                this.handle_format_cmd("h1", cx);
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &Heading2, window, cx| {
                this.handle_format_cmd("h2", cx);
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &Heading3, window, cx| {
                this.handle_format_cmd("h3", cx);
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &Heading4, window, cx| {
                this.handle_format_cmd("h4", cx);
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &Heading5, window, cx| {
                this.handle_format_cmd("h5", cx);
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &Heading6, window, cx| {
                this.handle_format_cmd("h6", cx);
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ToggleBulletList, window, cx| {
                this.handle_format_cmd("ul", cx);
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ToggleNumberedList, window, cx| {
                this.handle_format_cmd("ol", cx);
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ToggleChecklist, window, cx| {
                this.handle_format_cmd("checklist", cx);
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ToggleQuote, window, cx| {
                this.handle_format_cmd("quote", cx);
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ToggleInlineCode, window, cx| {
                this.handle_format_cmd("code", cx);
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &InsertCodeBlock, window, cx| {
                this.handle_format_cmd("codeblock", cx);
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &InsertDivider, window, cx| {
                this.handle_format_cmd("hr", cx);
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &InsertTable, window, cx| {
                this.handle_insert_table(cx);
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &InsertLink, window, cx| {
                this.handle_insert_link(cx);
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ClearFormatting, window, cx| {
                this.handle_format_cmd("clear", cx);
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &SaveFile, _window, cx| {
                this.handle_save(cx);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &OpenFile, _window, cx| {
                this.handle_open_file();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &NewTab, _window, cx| {
                this.handle_new_tab();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &NewNote, _window, cx| {
                this.begin_create(None, false, cx);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &NewFolder, _window, cx| {
                this.handle_new_folder(cx);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &DeleteNote, _window, cx| {
                this.delete_selected(cx);
            }))
            .on_action(cx.listener(|this, _: &NextTab, _window, cx| {
                this.cycle_tab(1);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &PrevTab, _window, cx| {
                this.cycle_tab(-1);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &CloseActiveTab, _window, cx| {
                let id = this.active_tab_id.clone();
                this.handle_close_tab(id);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &FindReplace, _window, cx| {
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
            }))
            .on_action(cx.listener(|this, _: &ToggleSidebar, _window, cx| {
                this.handle_toggle_sidebar();
                cx.notify();
            }))
            .on_action(cx.listener(|_this, _: &MinimizeWindow, window, _cx| {
                window.minimize_window();
            }))
            .on_action(cx.listener(|this, _: &QuitApp, _window, cx| {
                // Same path as the window close button (prompts on unsaved tabs).
                this.request_close_window();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ToggleZenMode, _window, cx| {
                this.handle_toggle_zen();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ExitZenMode, _window, cx| {
                this.handle_exit_zen();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ZoomIn, _window, cx| {
                this.handle_zoom_in(cx);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ZoomOut, _window, cx| {
                this.handle_zoom_out(cx);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ResetZoom, _window, cx| {
                this.handle_reset_zoom(cx);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ToggleViewMode, _window, cx| {
                this.handle_toggle_mode();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &UndoAction, window, cx| {
                this.handle_format_cmd("undo", cx);
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &RedoAction, window, cx| {
                this.handle_format_cmd("redo", cx);
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &SaveFileAs, _window, cx| {
                this.handle_save_as(cx);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &OpenReplace, _window, cx| {
                this.show_find_replace = true;
                this.refresh_find();
                this.show_replace_row = true;
                this.pending_find_focus = true;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ToggleLineNumbers, _window, cx| {
                this.handle_toggle_line_numbers();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &Cut, window, cx| {
                this.handle_cut(cx);
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &Copy, window, cx| {
                this.handle_copy(cx);
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &Paste, window, cx| {
                this.handle_paste(cx);
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &SelectAll, window, cx| {
                if this.sidebar_keyboard && this.open_menu.is_none() && this.modal_choice_count() == 0
                {
                    this.sidebar_select_all();
                    cx.notify();
                    return;
                }
                this.handle_select_all();
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &OpenShortcuts, _window, cx| {
                this.show_shortcuts_modal = true;
                this.prompt_selected_ix = 0;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &OpenFileMenu, _window, cx| {
                this.open_menu = Some(MenuId::File);
                this.menu_selected_ix = None;
                this.menu_open_submenu_ix = None;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &OpenEditMenu, _window, cx| {
                this.open_menu = Some(MenuId::Edit);
                this.menu_selected_ix = None;
                this.menu_open_submenu_ix = None;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &OpenViewMenu, _window, cx| {
                this.open_menu = Some(MenuId::View);
                this.menu_selected_ix = None;
                this.menu_open_submenu_ix = None;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &OpenSettingsMenu, _window, cx| {
                this.open_menu = Some(MenuId::Settings);
                this.menu_selected_ix = None;
                this.menu_open_submenu_ix = None;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &OpenHamburgerMenu, _window, cx| {
                this.open_menu = Some(MenuId::Hamburger);
                this.menu_selected_ix = None;
                this.menu_open_submenu_ix = None;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &FocusMenuBar, window, cx| {
                if this.open_menu.is_some() {
                    this.open_menu = None;
                    this.menu_selected_ix = None;
                    this.menu_open_submenu_ix = None;
                } else if window.viewport_size().width < px(640.0) {
                    this.open_menu = Some(MenuId::Hamburger);
                    this.menu_selected_ix = None;
                    this.menu_open_submenu_ix = None;
                } else {
                    this.open_menu = Some(MenuId::File);
                    this.menu_selected_ix = None;
                    this.menu_open_submenu_ix = None;
                }
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &CancelMenu, _window, cx| {
                if this.editor_context_menu.is_some() {
                    this.dismiss_editor_context_menu();
                } else if this.menu_open_submenu_ix.is_some() {
                    this.menu_open_submenu_ix = None;
                    this.submenu_selected_ix = None;
                } else {
                    this.open_menu = None;
                    this.menu_selected_ix = None;
                    this.submenu_selected_ix = None;
                }
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &PromptUp, _window, cx| {
                let count = this.modal_choice_count();
                if count > 0 {
                    this.prompt_selected_ix =
                        step_prompt_selection(this.prompt_selected_ix, count, true);
                    cx.notify();
                }
            }))
            .on_action(cx.listener(|this, _: &PromptDown, _window, cx| {
                let count = this.modal_choice_count();
                if count > 0 {
                    this.prompt_selected_ix =
                        step_prompt_selection(this.prompt_selected_ix, count, false);
                    cx.notify();
                }
            }))
            .on_action(cx.listener(|this, _: &PromptConfirm, _window, cx| {
                if let Some(prompt) = this.prompt.clone() {
                    let (_, _, choices) = this.prompt_modal_data(&prompt);
                    if let Some((choice, _)) = choices.get(this.prompt_selected_ix) {
                        this.answer_prompt(choice, cx);
                    }
                    cx.notify();
                } else if this.show_about_modal || this.show_shortcuts_modal {
                    this.show_about_modal = false;
                    this.show_shortcuts_modal = false;
                    cx.notify();
                } else if this.show_pdf_modal {
                    // 0 = "Open in Document Viewer", 1 = Close.
                    if this.prompt_selected_ix == 0 {
                        if let Some(ref path) =
                            this.active_tab().and_then(|t| t.file_path.clone())
                        {
                            cx.open_with_system(path);
                        }
                    } else {
                        this.show_pdf_modal = false;
                    }
                    cx.notify();
                } else if let Some(menu) = this.sidebar_menu.take() {
                    // 0 = Rename, 1 = Delete.
                    if this.prompt_selected_ix == 0 {
                        this.begin_rename(menu.target, cx);
                    } else {
                        match menu.target {
                            SidebarTarget::File(id) => this.request_trash_file(id),
                            SidebarTarget::Folder(id) => this.request_trash_folder(id),
                        }
                    }
                    cx.notify();
                }
            }))
            .on_action(cx.listener(|this, _: &PrevMenu, window, cx| {
                let is_narrow = window.viewport_size().width < px(640.0);
                if is_narrow {
                    this.open_menu = match this.open_menu {
                        Some(MenuId::Hamburger) => Some(MenuId::Settings),
                        Some(MenuId::Settings) => Some(MenuId::Hamburger),
                        _ => Some(MenuId::Hamburger),
                    };
                } else {
                    this.open_menu = match this.open_menu {
                        Some(MenuId::File) => Some(MenuId::Settings),
                        Some(MenuId::Edit) => Some(MenuId::File),
                        Some(MenuId::View) => Some(MenuId::Edit),
                        Some(MenuId::Settings) => Some(MenuId::View),
                        _ => Some(MenuId::File),
                    };
                }
                this.menu_selected_ix = None;
                this.menu_open_submenu_ix = None;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &NextMenu, window, cx| {
                let is_narrow = window.viewport_size().width < px(640.0);
                if is_narrow {
                    this.open_menu = match this.open_menu {
                        Some(MenuId::Hamburger) => Some(MenuId::Settings),
                        Some(MenuId::Settings) => Some(MenuId::Hamburger),
                        _ => Some(MenuId::Hamburger),
                    };
                } else {
                    this.open_menu = match this.open_menu {
                        Some(MenuId::File) => Some(MenuId::Edit),
                        Some(MenuId::Edit) => Some(MenuId::View),
                        Some(MenuId::View) => Some(MenuId::Settings),
                        Some(MenuId::Settings) => Some(MenuId::File),
                        _ => Some(MenuId::File),
                    };
                }
                this.menu_selected_ix = None;
                this.menu_open_submenu_ix = None;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &SelectNext, _window, cx| {
                if this.sidebar_keyboard {
                    this.sidebar_move_selection(1);
                } else {
                    this.menu_move_selection(1);
                }
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ExtendSelectNext, _window, cx| {
                if this.sidebar_keyboard {
                    this.sidebar_extend_selection(1);
                    cx.notify();
                }
            }))
            .on_action(cx.listener(|this, _: &ExtendSelectPrev, _window, cx| {
                if this.sidebar_keyboard {
                    this.sidebar_extend_selection(-1);
                    cx.notify();
                }
            }))
            .on_action(cx.listener(|this, _: &SelectPrev, _window, cx| {
                if this.sidebar_keyboard {
                    this.sidebar_move_selection(-1);
                } else {
                    this.menu_move_selection(-1);
                }
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &SelectFirst, _window, cx| {
                if this.sidebar_keyboard {
                    if let Some(first) = this.sidebar_visible_targets().first().cloned() {
                        this.selected_target = Some(first);
                        cx.notify();
                    }
                } else if this.open_menu.is_some() && this.open_menu != Some(MenuId::Table) {
                    let selectable = this.menu_selectable_ixs();
                    if let Some(&first) = selectable.first() {
                        this.menu_selected_ix = Some(first);
                        cx.notify();
                    }
                }
            }))
            .on_action(cx.listener(|this, _: &SelectLast, _window, cx| {
                if this.sidebar_keyboard {
                    if let Some(last) = this.sidebar_visible_targets().last().cloned() {
                        this.selected_target = Some(last);
                        cx.notify();
                    }
                } else if this.open_menu.is_some() && this.open_menu != Some(MenuId::Table) {
                    let selectable = this.menu_selectable_ixs();
                    if let Some(&last) = selectable.last() {
                        this.menu_selected_ix = Some(last);
                        cx.notify();
                    }
                }
            }))
            .on_action({
                // Deliberately NOT a cx.listener: the menu path runs item
                // actions that update this view, which must happen outside
                // the view update to avoid a re-entrant borrow.
                let view = view.clone();
                move |_: &Confirm, window, cx| {
                    let confirmed = view
                        .update(cx, |this, cx| {
                            if this.sidebar_keyboard {
                                this.sidebar_confirm(cx);
                                None
                            } else {
                                this.menu_take_confirm_action(cx)
                            }
                        })
                        .ok()
                        .flatten();
                    if let Some(action) = confirmed {
                        action(window, cx);
                    }
                }
            })
            .on_action(cx.listener(|this, _: &SidebarCollapse, _window, cx| {
                if this.sidebar_keyboard {
                    this.sidebar_collapse(cx);
                    cx.notify();
                }
            }))
            .on_action(cx.listener(|this, _: &SidebarExpand, _window, cx| {
                if this.sidebar_keyboard {
                    this.sidebar_expand(cx);
                    cx.notify();
                }
            }))
            .on_action(cx.listener(|this, _: &EditorLeft, window, cx| {
                let mode = this.effective_view_mode();
                this.editor_move(cx, |tab| tab.move_left_in_mode(false, mode));
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &EditorRight, window, cx| {
                let mode = this.effective_view_mode();
                this.editor_move(cx, |tab| tab.move_right_in_mode(false, mode));
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &EditorUp, window, cx| {
                let paint = this.editor_paint.clone();
                let mode = this.effective_view_mode();
                this.editor_move(cx, |tab| {
                    let p = paint.borrow();
                    if mode == ViewMode::Wysiwyg
                        && !cell_has_line_in_direction(&p, tab.cursor_offset(), -1)
                        && tab.table_edge_exit(-1)
                    {
                        return;
                    }
                    let parsed = tab.parsed();
                    if move_vertical_visual_in(&p, &parsed, &tab.content, &mut tab.caret, -1, false, mode) {
                        tab.pending_formats = ActiveFormats::default();
                        tab.refresh_cursor();
                    } else {
                        tab.move_up(false);
                    }
                });
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &EditorDown, window, cx| {
                let paint = this.editor_paint.clone();
                let mode = this.effective_view_mode();
                this.editor_move(cx, |tab| {
                    let p = paint.borrow();
                    if mode == ViewMode::Wysiwyg
                        && !cell_has_line_in_direction(&p, tab.cursor_offset(), 1)
                        && tab.table_edge_exit(1)
                    {
                        return;
                    }
                    let parsed = tab.parsed();
                    if move_vertical_visual_in(&p, &parsed, &tab.content, &mut tab.caret, 1, false, mode) {
                        tab.pending_formats = ActiveFormats::default();
                        tab.refresh_cursor();
                    } else {
                        tab.move_down(false);
                    }
                });
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &EditorSelectLeft, window, cx| {
                let mode = this.effective_view_mode();
                this.editor_move(cx, |tab| tab.move_left_in_mode(true, mode));
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &EditorSelectRight, window, cx| {
                let mode = this.effective_view_mode();
                this.editor_move(cx, |tab| tab.move_right_in_mode(true, mode));
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &EditorSelectUp, window, cx| {
                let paint = this.editor_paint.clone();
                let mode = this.effective_view_mode();
                this.editor_move(cx, |tab| {
                    let p = paint.borrow();
                    let parsed = tab.parsed();
                    if move_vertical_visual_in(&p, &parsed, &tab.content, &mut tab.caret, -1, true, mode) {
                        tab.pending_formats = ActiveFormats::default();
                        tab.refresh_cursor();
                    } else {
                        tab.move_up(true);
                    }
                });
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &EditorSelectDown, window, cx| {
                let paint = this.editor_paint.clone();
                let mode = this.effective_view_mode();
                this.editor_move(cx, |tab| {
                    let p = paint.borrow();
                    let parsed = tab.parsed();
                    if move_vertical_visual_in(&p, &parsed, &tab.content, &mut tab.caret, 1, true, mode) {
                        tab.pending_formats = ActiveFormats::default();
                        tab.refresh_cursor();
                    } else {
                        tab.move_down(true);
                    }
                });
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &EditorHome, window, cx| {
                let paint = this.editor_paint.clone();
                let mode = this.effective_view_mode();
                this.editor_move(cx, |tab| {
                    if mode == ViewMode::Wysiwyg {
                        let p = paint.borrow();
                        if editor_home_visual(&p, &mut tab.caret, false) {
                            tab.pending_formats = ActiveFormats::default();
                            tab.refresh_cursor();
                            return;
                        }
                    }
                    tab.move_home_in_mode(false, mode);
                });
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &EditorEnd, window, cx| {
                let paint = this.editor_paint.clone();
                let mode = this.effective_view_mode();
                this.editor_move(cx, |tab| {
                    if mode == ViewMode::Wysiwyg {
                        let p = paint.borrow();
                        if editor_end_visual(&p, &mut tab.caret, false) {
                            tab.pending_formats = ActiveFormats::default();
                            tab.refresh_cursor();
                            return;
                        }
                    }
                    tab.move_end_in_mode(false, mode);
                });
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &EditorSelectHome, window, cx| {
                let paint = this.editor_paint.clone();
                let mode = this.effective_view_mode();
                this.editor_move(cx, |tab| {
                    if mode == ViewMode::Wysiwyg {
                        let p = paint.borrow();
                        if editor_home_visual(&p, &mut tab.caret, true) {
                            tab.pending_formats = ActiveFormats::default();
                            tab.refresh_cursor();
                            return;
                        }
                    }
                    tab.move_home_in_mode(true, mode);
                });
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &EditorSelectEnd, window, cx| {
                let paint = this.editor_paint.clone();
                let mode = this.effective_view_mode();
                this.editor_move(cx, |tab| {
                    if mode == ViewMode::Wysiwyg {
                        let p = paint.borrow();
                        if editor_end_visual(&p, &mut tab.caret, true) {
                            tab.pending_formats = ActiveFormats::default();
                            tab.refresh_cursor();
                            return;
                        }
                    }
                    tab.move_end_in_mode(true, mode);
                });
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &EditorSelectLineUp, window, cx| {
                this.editor_move(cx, |tab| tab.select_line_up());
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &EditorSelectLineDown, window, cx| {
                this.editor_move(cx, |tab| tab.select_line_down());
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &EditorDocStart, window, cx| {
                this.editor_move(cx, |tab| tab.move_doc_start(false));
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &EditorDocEnd, window, cx| {
                this.editor_move(cx, |tab| tab.move_doc_end(false));
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &EditorSelectDocStart, window, cx| {
                this.editor_move(cx, |tab| tab.move_doc_start(true));
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &EditorSelectDocEnd, window, cx| {
                this.editor_move(cx, |tab| tab.move_doc_end(true));
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &EditorWordLeft, window, cx| {
                this.editor_move(cx, |tab| tab.move_word_left(false));
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &EditorWordRight, window, cx| {
                this.editor_move(cx, |tab| tab.move_word_right(false));
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &EditorSelectWordLeft, window, cx| {
                this.editor_move(cx, |tab| tab.move_word_left(true));
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &EditorSelectWordRight, window, cx| {
                this.editor_move(cx, |tab| tab.move_word_right(true));
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &EditorDeleteWordLeft, window, cx| {
                this.editor_delete_word_left(cx);
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &EditorDeleteWordRight, window, cx| {
                this.editor_delete_word_right(cx);
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &EditorDeleteLineStart, window, cx| {
                this.editor_delete_line_start(cx);
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &EditorDeleteLineEnd, window, cx| {
                this.editor_delete_line_end(cx);
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &EditorBackspace, window, cx| {
                this.editor_backspace(cx);
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &EditorDelete, window, cx| {
                this.editor_delete(cx);
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &EditorNewline, window, cx| {
                this.editor_newline(cx);
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &EditorPlainNewline, window, cx| {
                this.editor_plain_newline(cx);
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &EditorTableRowBelow, window, cx| {
                this.editor_table_row_below(cx);
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &IndentAction, window, cx| {
                this.editor_indent(cx);
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &OutdentAction, window, cx| {
                this.editor_outdent(cx);
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &EditorMoveLineUp, window, cx| {
                this.dispatch_editor_input(EditorInput::MoveLineUp);
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &EditorMoveLineDown, window, cx| {
                this.dispatch_editor_input(EditorInput::MoveLineDown);
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &FindNextMatch, _w, cx| {
                this.find_next();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &FindPrevMatch, _w, cx| {
                this.find_previous();
                cx.notify();
            }))
    }
}
