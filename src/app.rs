use crate::actions::*;
use crate::markdown::{export_stem, parse_document, render_export, TableColumnAlign};
use crate::model::text_ops::{
    home_offset, line_end, offset_to_utf16, range_from_utf16, range_to_utf16,
};
use crate::model::clipboard::{clipboard_item, paste_text};
use crate::model::*;
use crate::services::*;
use crate::theme::{sz, Theme, ThemeMode};
use crate::ui::MenuItem;
use crate::ui::*;
use gpui::prelude::*;
use gpui::*;
use std::cell::{Cell, RefCell};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

/// An item action stored in a menu: run with the window and app.
pub(crate) type MenuAction = Arc<dyn Fn(&mut Window, &mut App) + 'static>;

/// Tabs closed optimistically during a trash, kept so they can be restored
/// in place if the trash fails.
pub(crate) struct OptimisticClose {
    tabs: Vec<(usize, DocumentTab)>,
    prev_active: String,
    placeholder_id: Option<String>,
    prev_selected: Option<SidebarTarget>,
}

/// Result of moving or copying one dropped row.
enum DropOutcome {
    /// Carries the single-item toast text.
    Done(String),
    /// Nothing to do (already in that folder, dropped on itself).
    Skipped,
    Failed(String),
}

/// Shared state for one bulk trash: the result counter and the selection to
/// put back (once, after the last row finishes) if anything failed.
struct BulkTrash {
    report: RefCell<BulkTrashReport>,
    prev_selected: Option<SidebarTarget>,
}

type BulkReport = Rc<BulkTrash>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultiSelection {
    pub anchor: SidebarTarget,
    pub cursor: SidebarTarget,
    pub items: Vec<SidebarTarget>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Prompt {
    CloseTab { id: String },
    CloseWindow,
    DeleteFile { id: String },
    DeleteFolder { id: String },
    TrashDirtyFile { id: String },
    TrashDirtyFolder { id: String },
    /// Several sidebar rows selected with Shift, trashed together.
    DeleteMany { targets: Vec<SidebarTarget> },
    /// A new workspace folder was picked; ask whether to copy the current
    /// note into it before switching.
    SwitchWorkspace { new_dir: PathBuf },
}

/// What to resume after an untitled tab is saved with a chosen name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PendingAfterSave {
    CloseTab(String),
    CloseWindow,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EditorContextMenu {
    pub position: Point<Pixels>,
    pub is_table: bool,
}

pub struct LiteMarkApp {
    pub tabs: Vec<DocumentTab>,
    pub active_tab_id: String,
    pub workspace: WorkspaceDirectory,
    pub settings: EditorSettings,
    pub show_sidebar: bool,
    pub show_find_replace: bool,
    pub show_replace_row: bool,
    pub show_shortcuts_modal: bool,
    pub show_pdf_modal: bool,
    pub show_about_modal: bool,
    pub open_menu: Option<MenuId>,
    pub menu_selected_ix: Option<usize>,
    pub menu_open_submenu_ix: Option<usize>,
    pub submenu_selected_ix: Option<usize>,
    pub menu_items_cache: Vec<MenuItem>,
    /// Currently highlighted sidebar item (drives row highlight and the
    /// Ctrl+Delete target). Falls back to the active tab's note when None.
    pub selected_target: Option<SidebarTarget>,
    /// Shift-selected range in the sidebar; only honored while
    /// `selected_target` is still its cursor, so any other selection change
    /// drops it.
    pub multi_selection: Option<MultiSelection>,
    /// True while the sidebar tree owns arrow-key navigation.
    pub sidebar_keyboard: bool,
    pub sidebar_focus_handle: FocusHandle,
    pub zen_mode: bool,
    pub zen_notice: bool,
    pub saved_toast: Option<String>,
    pub show_zoom_hud: bool,
    pub search_query: String,
    pub find_term: String,
    pub replace_term: String,
    pub match_case: bool,
    pub not_found: bool,
    pub focus_handle: FocusHandle,
    pub menu_focus_handle: FocusHandle,
    #[allow(dead_code)]
    pub appearance_subscription: Option<Subscription>,
    pub tab_ids: TabIds,
    pub error_message: Option<String>,
    pub prompt: Option<Prompt>,
    pub prompt_focus_handle: FocusHandle,
    pub prompt_selected_ix: usize,
    pub save_as_tab: Option<String>,
    pub pending_after_save: Option<PendingAfterSave>,
    pub editor_paint: Rc<RefCell<EditorPaint>>,
    pub editor_scroll: ScrollHandle,
    pub find_field: Entity<LineField>,
    pub replace_field: Entity<LineField>,
    pub find_hits: Vec<Range<usize>>,
    pub find_index: Option<usize>,
    pub applied_focus: Option<FocusSlot>,
    pub close_hook_installed: bool,
    pub allow_window_close: bool,
    pub selecting: bool,
    pub zoom_epoch: u64,
    pub toast_epoch: u64,
    pub pending_find_focus: bool,
    pub pending_rename_focus: bool,
    pub sidebar_menu: Option<SidebarMenu>,
    pub workspace_menu_open: bool,
    pub rename_target: Option<SidebarTarget>,
    pub rename_value: String,
    pub rename_field: Entity<LineField>,
    pub create_draft: Option<SidebarCreate>,
    pub create_field: Entity<LineField>,
    pub search_field: Entity<LineField>,
    pub pending_create_focus: bool,
    pub field_owns_keys: bool,
    pub field_focus: TextInput,
    pub bounds_subscription: Option<Subscription>,
    pub table_picker_size: (usize, usize),
    pub editor_context_menu: Option<EditorContextMenu>,
    title_drag_armed: Rc<Cell<bool>>,
}

impl LiteMarkApp {
    pub fn new(initial_file: Option<PathBuf>, cx: &mut Context<Self>) -> Self {
        let settings = EditorSettings::load();
        let notes_dir = get_or_create_notes_dir();
        seed_welcome_note(&notes_dir);
        let workspace = scan_workspace_dir(&notes_dir);

        let mut tab_ids = TabIds::new();
        let first_id = tab_ids.alloc();
        let mut error_message = None;
        let initial_tab = if let Some(path) = initial_file {
            let name = path
                .file_name()
                .and_then(|f| f.to_str())
                .unwrap_or("Untitled.md")
                .to_string();
            match read_note(&path) {
                Ok(content) => DocumentTab::new(first_id.clone(), name, content, Some(path), None),
                Err(err) => {
                    error_message = Some(err);
                    Self::welcome_tab(&workspace, first_id.clone())
                }
            }
        } else {
            Self::welcome_tab(&workspace, first_id.clone())
        };

        let app_handle = cx.weak_entity();
        let find_field = cx.new({
            let app_handle = app_handle.clone();
            move |cx| {
                LineField::new(
                    cx,
                    "find-field",
                    "Find in document...",
                    Arc::new({
                        let app_handle = app_handle.clone();
                        move |text, app| {
                            app_handle
                                .update(app, |this, cx| {
                                    this.find_term = text.to_string();
                                    this.find_index = None;
                                    this.refresh_find();
                                    cx.notify();
                                })
                                .ok();
                        }
                    }),
                    Arc::new({
                        let app_handle = app_handle.clone();
                        move |_text, previous, app| {
                            app_handle
                                .update(app, |this, cx| {
                                    if previous {
                                        this.find_previous();
                                    } else {
                                        this.find_next();
                                    }
                                    cx.notify();
                                })
                                .ok();
                        }
                    }),
                    Arc::new({
                        let app_handle = app_handle.clone();
                        move |app| {
                            app_handle
                                .update(app, |this, cx| this.claim_field(TextInput::Find, cx))
                                .ok();
                        }
                    }),
                )
            }
        });
        let replace_field = cx.new({
            let app_handle = app_handle.clone();
            move |cx| {
                LineField::new(
                    cx,
                    "replace-field",
                    "Replace with...",
                    Arc::new({
                        let app_handle = app_handle.clone();
                        move |text, app| {
                            app_handle
                                .update(app, |this, cx| {
                                    this.replace_term = text.to_string();
                                    cx.notify();
                                })
                                .ok();
                        }
                    }),
                    Arc::new(|_text, _previous, _app| {}),
                    Arc::new({
                        let app_handle = app_handle.clone();
                        move |app| {
                            app_handle
                                .update(app, |this, cx| this.claim_field(TextInput::Replace, cx))
                                .ok();
                        }
                    }),
                )
            }
        });
        let rename_field = cx.new({
            let app_handle = app_handle.clone();
            move |cx| {
                LineField::new(
                    cx,
                    "rename-field",
                    "New name",
                    Arc::new({
                        let app_handle = app_handle.clone();
                        move |text, app| {
                            app_handle
                                .update(app, |this, cx| {
                                    this.rename_value = text.to_string();
                                    cx.notify();
                                })
                                .ok();
                        }
                    }),
                    Arc::new({
                        let app_handle = app_handle.clone();
                        move |text, _previous, app| {
                            let text = text.to_string();
                            app_handle
                                .update(app, |this, cx| {
                                    if this.save_as_tab.is_some() {
                                        this.commit_save_naming(Some(text), cx);
                                    } else {
                                        this.rename_value = text.clone();
                                        this.commit_rename(Some(text), cx);
                                    }
                                    cx.notify();
                                })
                                .ok();
                        }
                    }),
                    Arc::new({
                        let app_handle = app_handle.clone();
                        move |app| {
                            app_handle
                                .update(app, |this, cx| this.claim_field(TextInput::Rename, cx))
                                .ok();
                        }
                    }),
                )
            }
        });

        let create_field = cx.new({
            let app_handle = app_handle.clone();
            move |cx| {
                LineField::new(
                    cx,
                    "create-field",
                    "Name",
                    Arc::new({
                        let app_handle = app_handle.clone();
                        move |_text, app| {
                            app_handle.update(app, |_, cx| cx.notify()).ok();
                        }
                    }),
                    Arc::new({
                        let app_handle = app_handle.clone();
                        move |text, _previous, app| {
                            let text = text.to_string();
                            app_handle
                                .update(app, |this, cx| {
                                    this.commit_create(text, cx);
                                    cx.notify();
                                })
                                .ok();
                        }
                    }),
                    Arc::new({
                        let app_handle = app_handle.clone();
                        move |app| {
                            app_handle
                                .update(app, |this, cx| this.claim_field(TextInput::Create, cx))
                                .ok();
                        }
                    }),
                )
            }
        });

        let search_field = cx.new({
            let app_handle = app_handle.clone();
            move |cx| {
                LineField::new(
                    cx,
                    "search-field",
                    "Search notes",
                    Arc::new({
                        let app_handle = app_handle.clone();
                        move |text, app| {
                            let text = text.to_string();
                            app_handle
                                .update(app, |this, cx| {
                                    this.search_query = text;
                                    this.multi_selection = None;
                                    cx.notify();
                                })
                                .ok();
                        }
                    }),
                    Arc::new({
                        let app_handle = app_handle.clone();
                        move |_text, _previous, app| {
                            app_handle
                                .update(app, |this, cx| {
                                    this.search_enter(cx);
                                    cx.notify();
                                })
                                .ok();
                        }
                    }),
                    Arc::new({
                        let app_handle = app_handle.clone();
                        move |app| {
                            app_handle
                                .update(app, |this, cx| this.claim_field(TextInput::Search, cx))
                                .ok();
                        }
                    }),
                )
            }
        });

        let initial_selected = initial_tab.workspace_file_id.clone();

        Self {
            tabs: vec![initial_tab],
            active_tab_id: first_id,
            selected_target: initial_selected.map(SidebarTarget::File),
            multi_selection: None,
            workspace,
            settings,
            show_sidebar: true,
            show_find_replace: false,
            show_replace_row: false,
            show_shortcuts_modal: false,
            show_pdf_modal: false,
            show_about_modal: false,
            open_menu: None,
            menu_selected_ix: None,
            menu_open_submenu_ix: None,
            submenu_selected_ix: None,
            menu_items_cache: Vec::new(),
            sidebar_keyboard: false,
            sidebar_focus_handle: cx.focus_handle(),
            zen_mode: false,
            zen_notice: false,
            saved_toast: None,
            show_zoom_hud: false,
            search_query: String::new(),
            find_term: String::new(),
            replace_term: String::new(),
            match_case: false,
            not_found: false,
            focus_handle: cx.focus_handle(),
            menu_focus_handle: cx.focus_handle(),
            prompt_focus_handle: cx.focus_handle(),
            appearance_subscription: None,
            tab_ids,
            error_message,
            prompt: None,
            prompt_selected_ix: 0,
            save_as_tab: None,
            pending_after_save: None,
            editor_paint: Rc::new(RefCell::new(EditorPaint::default())),
            editor_scroll: ScrollHandle::new(),
            find_field,
            replace_field,
            find_hits: Vec::new(),
            find_index: None,
            applied_focus: None,
            close_hook_installed: false,
            allow_window_close: false,
            selecting: false,
            zoom_epoch: 0,
            toast_epoch: 0,
            pending_find_focus: false,
            pending_rename_focus: false,
            sidebar_menu: None,
            workspace_menu_open: false,
            rename_target: None,
            rename_value: String::new(),
            rename_field,
            create_draft: None,
            create_field,
            search_field,
            pending_create_focus: false,
            field_owns_keys: false,
            field_focus: TextInput::Editor,
            bounds_subscription: None,
            table_picker_size: (2, 2),
            editor_context_menu: None,
            title_drag_armed: Rc::new(Cell::new(false)),
        }
    }

    /// Enter in the search box: select the first match and move keyboard
    /// focus to the list so the arrows walk through the results.
    fn search_enter(&mut self, cx: &mut Context<Self>) {
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
    fn clear_search(&mut self, cx: &mut Context<Self>) {
        self.search_query.clear();
        self.search_field.update(cx, |field, _cx| field.set_text(""));
        self.field_focus = TextInput::Editor;
        self.field_owns_keys = false;
        self.applied_focus = None;
    }

    fn claim_field(&mut self, which: TextInput, cx: &mut Context<Self>) {
        self.field_focus = which;
        self.field_owns_keys = !document_accepts_keystrokes(which);
        self.sidebar_keyboard = sidebar_keyboard_after_claim(self.sidebar_keyboard, which);
        if which == TextInput::Editor {
            self.applied_focus = None;
        }
        cx.notify();
    }

    fn line_field_for_focus(&self) -> Option<Entity<LineField>> {
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
    fn forward_keystroke_to_field(
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
        let mode = self.settings.view_mode;
        let tab = self.tabs.iter_mut().find(|t| t.id == self.active_tab_id)?;
        tab.set_view_mode(mode);
        Some(tab)
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
            Err(err) => self.show_error(err),
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
            Err(err) => self.show_error(format!("Couldn't export: {err}")),
        }
    }

    pub fn handle_format_cmd(&mut self, cmd: &'static str, _cx: &mut App) {
        if !command_edits_document(cmd, self.field_focus, self.field_owns_keys) {
            return;
        }
        if self.active_tab().is_some_and(|tab| tab.is_read_only()) && cmd != "undo" && cmd != "redo"
        {
            self.show_error("PDF files are read-only.");
            return;
        }
        let mode = self.settings.view_mode;
        if let Some(tab) = self.active_tab_mut() {
            match cmd {
                "undo" => {
                    tab.undo();
                }
                "redo" => {
                    tab.redo();
                }
                other => tab.apply_format_in_mode(other, mode),
            }
        }
        self.refresh_find();
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
        if let Some(tab) = self.active_tab_mut() {
            tab.insert_table_with_size(cols, rows);
        }
        self.refresh_find();
    }

    pub fn handle_table_edit_action(&mut self, action: &'static str, _cx: &mut App) {
        if let Some(tab) = self.active_tab_mut() {
            match action {
                "row_above" => tab.table_add_row_above(),
                "row_below" => tab.table_add_row_below(),
                "col_left" => tab.table_add_col_left(),
                "col_right" => tab.table_add_col_right(),
                "align_left" => tab.table_align_column(TableColumnAlign::Left),
                "align_center" => tab.table_align_column(TableColumnAlign::Center),
                "align_right" => tab.table_align_column(TableColumnAlign::Right),
                "delete_row" => tab.table_delete_row(),
                "delete_col" => tab.table_delete_col(),
                "delete_table" => tab.table_delete_table(),
                _ => {}
            }
        }
        self.refresh_find();
    }

    pub fn handle_insert_link(&mut self, cx: &mut App) {
        self.handle_format_cmd("link", cx);
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
                if let Some(tab) = self.active_tab_mut() {
                    tab.file_path = Some(path.clone());
                    if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
                        tab.title = name.to_string();
                    }
                    tab.file_type = FileType::from_filename(&tab.title);
                    tab.is_modified = false;
                    let title = tab.title.clone();
                    self.show_toast(format!("Saved {title}"), cx);
                }
                self.rescan_workspace();
            }
            Err(err) => self.show_error(format!("Couldn't save {}: {err}", path.display())),
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
    fn commit_create(&mut self, raw: String, cx: &mut Context<Self>) {
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
            self.show_error(err);
        }
        cx.notify();
    }

    pub fn handle_toggle_word_wrap(&mut self) {
        self.settings.word_wrap = !self.settings.word_wrap;
        self.settings.save();
    }

    pub fn handle_set_content_alignment(&mut self, alignment: ContentAlignment) {
        self.settings.content_alignment = alignment;
        self.settings.save();
    }

    pub fn handle_toggle_line_numbers(&mut self) {
        self.settings.show_line_numbers = !self.settings.show_line_numbers;
        self.settings.save();
    }

    pub fn handle_set_view_mode(&mut self, mode: ViewMode) {
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
    fn selection_clipboard_item(&mut self) -> Option<ClipboardItem> {
        let tab = self.active_tab_mut()?;
        let markdown = tab.selected_text();
        if markdown.is_empty() {
            return None;
        }
        let visible = tab.selected_visible_text();
        Some(clipboard_item(visible, markdown))
    }

    pub fn handle_cut(&mut self, cx: &mut App) {
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

    fn welcome_tab(workspace: &WorkspaceDirectory, id: String) -> DocumentTab {
        if let Some(file) = workspace
            .files
            .iter()
            .find(|file| file.name == "Welcome.md")
        {
            if let Ok(content) = read_note(&file.path) {
                return DocumentTab::new(
                    id,
                    file.name.clone(),
                    content,
                    Some(file.path.clone()),
                    Some(file.id.clone()),
                );
            }
        }
        DocumentTab::new(
            id,
            "Welcome.md".into(),
            INITIAL_MARKDOWN.to_string(),
            None,
            None,
        )
    }

    fn input_blocked(&self) -> bool {
        self.prompt.is_some()
    }

    fn text_field_focused(&self, window: &Window, cx: &App) -> bool {
        self.find_field.read(cx).is_focused(window)
            || self.replace_field.read(cx).is_focused(window)
            || self.rename_field.read(cx).is_focused(window)
            || self.create_field.read(cx).is_focused(window)
            || self.search_field.read(cx).is_focused(window)
    }

    fn show_error(&mut self, message: impl Into<String>) {
        self.error_message = Some(message.into());
    }

    fn show_toast(&mut self, message: impl Into<String>, cx: &mut Context<Self>) {
        self.saved_toast = Some(message.into());
        self.toast_epoch = self.toast_epoch.wrapping_add(1);
        let epoch = self.toast_epoch;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(TRANSIENT_TTL_MS))
                .await;
            this.update(cx, |this, cx| {
                if this.toast_epoch == epoch {
                    this.saved_toast = None;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    fn flash_zoom(&mut self, cx: &mut Context<Self>) {
        self.show_zoom_hud = true;
        self.zoom_epoch = self.zoom_epoch.wrapping_add(1);
        let epoch = self.zoom_epoch;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(TRANSIENT_TTL_MS))
                .await;
            this.update(cx, |this, cx| {
                if this.zoom_epoch == epoch {
                    this.show_zoom_hud = false;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    fn refresh_find(&mut self) {
        self.find_hits = self
            .active_tab()
            .map(|tab| tab.find_hit_ranges(&self.find_term, self.match_case))
            .unwrap_or_default();
        if self.find_term.is_empty() {
            self.find_index = None;
            self.not_found = false;
        } else if self.find_hits.is_empty() {
            self.find_index = None;
            self.not_found = true;
        } else {
            self.not_found = false;
            if self
                .find_index
                .is_some_and(|index| index >= self.find_hits.len())
            {
                self.find_index = None;
            }
        }
    }

    fn match_label(&self) -> String {
        if self.find_term.is_empty() {
            String::new()
        } else if self.find_hits.is_empty() {
            "No results".into()
        } else if let Some(index) = self.find_index {
            format!("{} of {}", index + 1, self.find_hits.len())
        } else {
            format!("{} matches", self.find_hits.len())
        }
    }

    fn find_next(&mut self) {
        self.step_find(true);
    }

    fn find_previous(&mut self) {
        self.step_find(false);
    }

    fn step_find(&mut self, forward: bool) {
        if self.find_term.is_empty() {
            return;
        }
        let term = self.find_term.clone();
        let case = self.match_case;
        let current = self.find_index;
        let index = self
            .active_tab_mut()
            .and_then(|tab| tab.select_match(&term, case, current, forward));
        self.find_index = index;
        self.refresh_find();
        if index.is_none() {
            self.not_found = true;
        } else {
            self.scroll_selection_into_view();
        }
    }

    /// Scrolls the editor so the row holding the selection start is visible.
    /// Uses the geometry from the last paint, which is still valid after a
    /// find step because the content did not change.
    fn scroll_selection_into_view(&mut self) {
        let Some(start) = self.active_tab().map(|tab| tab.selection_range().start) else {
            return;
        };
        let paint = self.editor_paint.borrow();
        let Some(row) = paint
            .rows
            .iter()
            .rev()
            .find(|row| row.byte_start <= start)
            .or_else(|| paint.rows.first())
        else {
            return;
        };
        let view = self.editor_scroll.bounds();
        let Some(delta) =
            scroll_delta_for_row(row.origin.y, row.line_height, view.top(), view.size.height)
        else {
            return;
        };
        let offset = self.editor_scroll.offset();
        let max = self.editor_scroll.max_offset().height;
        let y = (offset.y - delta).clamp(-max, px(0.0));
        self.editor_scroll.set_offset(point(offset.x, y));
    }

    fn replace_current(&mut self) {
        if self.active_tab().is_some_and(|tab| tab.is_read_only()) {
            self.show_error("PDF files are read-only.");
            return;
        }
        let term = self.find_term.clone();
        let replacement = self.replace_term.clone();
        let case = self.match_case;
        let current = self.find_index;
        let index = self
            .active_tab_mut()
            .and_then(|tab| tab.replace_current_match(&term, &replacement, case, current));
        self.find_index = index;
        self.refresh_find();
        if index.is_some() {
            self.scroll_selection_into_view();
        }
    }

    fn replace_all_matches(&mut self) {
        if self.active_tab().is_some_and(|tab| tab.is_read_only()) {
            self.show_error("PDF files are read-only.");
            return;
        }
        let term = self.find_term.clone();
        let replacement = self.replace_term.clone();
        let case = self.match_case;
        let changed = self
            .active_tab_mut()
            .is_some_and(|tab| tab.replace_every_match(&term, &replacement, case));
        self.find_index = None;
        self.refresh_find();
        if !changed && !term.is_empty() {
            self.not_found = true;
        }
    }

    fn save_tab(&mut self, id: &str) -> Result<String, String> {
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
            tab.file_type = FileType::from_filename(&tab.title);
            tab.is_modified = false;
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

    fn open_loaded_tab(
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

    fn remove_tab(&mut self, id: &str, next_active: Option<String>) {
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

    fn discard_tab(&mut self, id: &str) {
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

    fn finish_close_saved(&mut self, id: &str) {
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

    fn folder_pairs(&self) -> Vec<(String, PathBuf)> {
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
        // Clicking a folder selects it (Ctrl+Delete target) and keeps
        // keyboard navigation in the sidebar tree.
        self.multi_selection = None;
        self.selected_target = Some(SidebarTarget::Folder(folder_id.to_string()));
        self.sidebar_keyboard = true;
    }

    /// Point the sidebar selection at the active tab's note (tab-bar clicks,
    /// tab closes) and hand keyboard focus back to the editor.
    fn sync_selection_to_active_tab(&mut self) {
        self.selected_target = self
            .active_tab()
            .and_then(|tab| tab.workspace_file_id.clone())
            .map(SidebarTarget::File);
        self.sidebar_keyboard = false;
    }

    /// Flatten the visible sidebar tree into selection targets, in row order.
    fn sidebar_visible_targets(&self) -> Vec<SidebarTarget> {
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
    fn active_multi(&self) -> Option<&MultiSelection> {
        self.multi_selection
            .as_ref()
            .filter(|m| self.selected_target.as_ref() == Some(&m.cursor) && m.items.len() > 1)
    }

    /// Extend the selection from its anchor to `to` over the visible rows.
    fn extend_selection_to(&mut self, to: SidebarTarget) {
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

    /// Ctrl+A in the sidebar: select every visible row.
    fn sidebar_select_all(&mut self) {
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
    fn sidebar_extend_selection(&mut self, delta: i32) {
        let targets = self.sidebar_visible_targets();
        if let Some(next) = step_cursor(&targets, self.selected_target.as_ref(), delta) {
            self.extend_selection_to(targets[next].clone());
        }
    }

    /// Ask before trashing every Shift-selected row. Rows inside a selected
    /// folder go with it; modified notes must be saved or closed first.
    fn request_trash_many(&mut self, items: Vec<SidebarTarget>) {
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
    fn sidebar_move_selection(&mut self, delta: i32) {
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
    fn sidebar_confirm(&mut self, cx: &mut Context<Self>) {
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
    fn sidebar_collapse(&mut self, cx: &mut Context<Self>) {
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
    fn sidebar_expand(&mut self, cx: &mut Context<Self>) {
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
        let mut new_workspace = scan_workspace_dir(&self.workspace.folder_path);
        for folder in &mut new_workspace.folders {
            if let Some(&open) = open_map.get(&folder.path) {
                folder.is_open = open;
            }
        }
        self.workspace = new_workspace;
        self.refresh_workspace_ids();
    }

    fn create_note(&mut self, folder_id: Option<String>, desired: String) -> Result<(), String> {
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

    fn create_folder(&mut self, parent_id: Option<String>, desired: String) -> Result<(), String> {
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

    fn refresh_workspace_ids(&mut self) {
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
    fn modal_choice_count(&self) -> usize {
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

    /// Handle a note or folder dragged and dropped onto a sidebar folder,
    /// or onto the notes list itself (`folder_id: None` = move to the
    /// workspace root). A plain drop moves; holding Ctrl drops a copy.
    /// Drop of one row or a whole multi-selection. Rows inside a selected
    /// folder travel with it, and a folder dropped on itself is skipped.
    fn handle_drop_items(
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

    fn handle_drop_item(
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

    fn drop_one(
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

    fn move_or_copy_note(
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
                    .map_err(|err| err.to_string())
            });
            match result {
                Ok(new_path) => {
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
                        self.open_loaded_tab(display, String::new(), Some(new_path), new_file_id);
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

    fn move_or_copy_folder(
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

    /// Route a picked workspace directory: when the current note lives on
    /// disk inside the current workspace, ask whether to copy it into the
    /// new one before switching; otherwise switch right away.
    fn prepare_workspace_switch(&mut self, new_dir: PathBuf) {
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
    fn migratable_note_path(&self) -> Option<PathBuf> {
        let tab = self.active_tab()?;
        if !matches!(tab.file_type, FileType::Markdown | FileType::PlainText) {
            return None;
        }
        tab.file_path
            .clone()
            .filter(|path| path.starts_with(&self.workspace.folder_path))
    }

    /// Switch the workspace to `new_dir`, optionally copying the current
    /// note (including unsaved edits) into it before the switch. The
    /// original file on disk is never removed.
    fn apply_workspace_switch(&mut self, new_dir: PathBuf, migrate_from: Option<PathBuf>) {
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
        self.workspace = scan_workspace_dir(&new_dir);
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

    fn retarget_open_tabs(&mut self, old: &Path, new_path: &Path) {
        for tab in &mut self.tabs {
            let Some(path) = tab.file_path.clone() else {
                continue;
            };
            if path == old {
                tab.file_path = Some(new_path.to_path_buf());
                if let Some(name) = new_path.file_name().and_then(|name| name.to_str()) {
                    tab.title = name.to_string();
                    tab.file_type = FileType::from_filename(&tab.title);
                }
            } else if let Ok(rest) = path.strip_prefix(old) {
                tab.file_path = Some(new_path.join(rest));
            }
        }
    }

    fn request_trash_file(&mut self, id: String) {
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

    fn request_trash_folder(&mut self, id: String) {
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

    fn dirty_tab_ids_for_path(&self, path: &Path) -> Vec<String> {
        self.tabs
            .iter()
            .filter(|tab| tab.is_modified && tab.file_path.as_deref() == Some(path))
            .map(|tab| tab.id.clone())
            .collect()
    }

    fn dirty_tab_ids_under(&self, prefix: &Path) -> Vec<String> {
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

    fn save_ids(&mut self, ids: &[String]) -> Result<(), String> {
        for id in ids {
            self.save_tab(id)?;
        }
        Ok(())
    }

    fn begin_rename(&mut self, target: SidebarTarget, cx: &mut Context<Self>) {
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

    fn cancel_rename(&mut self, cx: &mut Context<Self>) {
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
    fn begin_save_naming(&mut self, tab_id: &str, cx: &mut Context<Self>) {
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
    fn commit_save_naming(
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

    fn first_unnamed_dirty_tab(&self) -> Option<String> {
        self.tabs
            .iter()
            .find(|tab| tab.is_modified && tab.file_path.is_none())
            .map(|tab| tab.id.clone())
    }

    /// Continue the close-window flow after a tab was saved with a name:
    /// name the next untitled dirty tab, or finish the save-all.
    fn resume_close_window(&mut self, cx: &mut Context<Self>) {
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

    fn commit_rename(&mut self, name_override: Option<String>, _cx: &mut Context<Self>) {
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
    fn optimistic_close_tabs(&mut self, pred: impl Fn(&Path) -> bool) -> OptimisticClose {
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
    fn restore_optimistic_close(
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

    fn trash_file(&mut self, id: &str, cx: &mut Context<Self>) {
        self.trash_file_in(id, cx, None);
    }

    fn trash_file_in(&mut self, id: &str, cx: &mut Context<Self>, bulk: Option<BulkReport>) {
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

    fn trash_folder(&mut self, id: &str, cx: &mut Context<Self>) {
        self.trash_folder_in(id, cx, None);
    }

    /// Settle one background trash: rescan, undo the optimistic close on
    /// failure, and report. A bulk trash shows a single message at the end.
    fn finish_trash(
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
    fn record_bulk(&mut self, bulk: &BulkTrash, result: Result<(), String>, cx: &mut Context<Self>) {
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
    fn report_missing(&mut self, message: &str, bulk: Option<BulkReport>) {
        match bulk {
            Some(bulk) => {
                if let Some(message) = bulk.report.borrow_mut().record(Err(message.into())) {
                    self.show_error(message);
                }
            }
            None => self.show_error(message),
        }
    }

    fn trash_folder_in(&mut self, id: &str, cx: &mut Context<Self>, bulk: Option<BulkReport>) {
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

    fn save_all_dirty(&mut self) -> Result<(), String> {
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

    /// Indices of selectable items (non-divider, non-heading, enabled) in
    /// a list of menu items.
    fn selectable_menu_ixs(items: &[MenuItem]) -> Vec<usize> {
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

    fn menu_selectable_ixs(&self) -> Vec<usize> {
        Self::selectable_menu_ixs(&self.menu_items_cache)
    }

    /// Move the menu selection by `delta` items (wrapping), skipping
    /// dividers, headings and disabled entries. With a submenu open,
    /// navigation happens inside the submenu instead.
    fn menu_move_selection(&mut self, delta: i32) {
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
    fn menu_take_confirm_action(&mut self, cx: &mut Context<Self>) -> Option<MenuAction> {
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
    fn show_prompt(&mut self, prompt: Prompt) {
        let (_, _, choices) = self.prompt_modal_data(&prompt);
        self.prompt_selected_ix = default_prompt_choice_index(&choices);
        self.prompt = Some(prompt);
    }

    /// Title, message and button choices for a prompt. The bool marks the
    /// default choice.
    fn prompt_modal_data(
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

    fn answer_prompt(&mut self, choice: &'static str, cx: &mut Context<Self>) {
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

    fn request_close_window(&mut self) {
        if self.tabs.iter().any(|tab| tab.is_modified) {
            self.show_prompt(Prompt::CloseWindow);
        } else {
            self.allow_window_close = true;
        }
    }

    fn install_close_hook(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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

    fn editor_backspace(&mut self, _cx: &mut App) {
        if !document_accepts_keystrokes(self.field_focus)
            || self.field_owns_keys
            || self.input_blocked()
        {
            return;
        }
        if self.active_tab().is_some_and(|tab| tab.is_read_only()) {
            self.show_error("PDF files are read-only.");
            return;
        }
        let mode = self.settings.view_mode;
        if let Some(tab) = self.active_tab_mut() {
            tab.smart_backspace_in_mode(mode);
        }
        self.refresh_find();
    }

    fn editor_delete(&mut self, _cx: &mut App) {
        if !document_accepts_keystrokes(self.field_focus)
            || self.field_owns_keys
            || self.input_blocked()
        {
            return;
        }
        if self.active_tab().is_some_and(|tab| tab.is_read_only()) {
            self.show_error("PDF files are read-only.");
            return;
        }
        let mode = self.settings.view_mode;
        if let Some(tab) = self.active_tab_mut() {
            tab.delete_forward_in_mode(mode);
        }
        self.refresh_find();
    }

    fn editor_delete_word_left(&mut self, _cx: &mut App) {
        if !document_accepts_keystrokes(self.field_focus)
            || self.field_owns_keys
            || self.input_blocked()
        {
            return;
        }
        if self.active_tab().is_some_and(|tab| tab.is_read_only()) {
            self.show_error("PDF files are read-only.");
            return;
        }
        if let Some(tab) = self.active_tab_mut() {
            tab.delete_word_left();
        }
        self.refresh_find();
    }

    fn editor_delete_word_right(&mut self, _cx: &mut App) {
        if !document_accepts_keystrokes(self.field_focus)
            || self.field_owns_keys
            || self.input_blocked()
        {
            return;
        }
        if self.active_tab().is_some_and(|tab| tab.is_read_only()) {
            self.show_error("PDF files are read-only.");
            return;
        }
        if let Some(tab) = self.active_tab_mut() {
            tab.delete_word_right();
        }
        self.refresh_find();
    }

    fn editor_delete_line_start(&mut self, _cx: &mut App) {
        if !document_accepts_keystrokes(self.field_focus)
            || self.field_owns_keys
            || self.input_blocked()
        {
            return;
        }
        if self.active_tab().is_some_and(|tab| tab.is_read_only()) {
            self.show_error("PDF files are read-only.");
            return;
        }
        if let Some(tab) = self.active_tab_mut() {
            tab.delete_to_line_start();
        }
        self.refresh_find();
    }

    fn editor_delete_line_end(&mut self, _cx: &mut App) {
        if !document_accepts_keystrokes(self.field_focus)
            || self.field_owns_keys
            || self.input_blocked()
        {
            return;
        }
        if self.active_tab().is_some_and(|tab| tab.is_read_only()) {
            self.show_error("PDF files are read-only.");
            return;
        }
        if let Some(tab) = self.active_tab_mut() {
            tab.delete_to_line_end();
        }
        self.refresh_find();
    }

    fn editor_newline(&mut self, _cx: &mut App) {
        if !document_accepts_keystrokes(self.field_focus)
            || self.field_owns_keys
            || self.input_blocked()
        {
            return;
        }
        if self.active_tab().is_some_and(|tab| tab.is_read_only()) {
            self.show_error("PDF files are read-only.");
            return;
        }
        if let Some(tab) = self.active_tab_mut() {
            tab.smart_newline(false);
        }
        self.refresh_find();
    }

    fn editor_plain_newline(&mut self, _cx: &mut App) {
        if !document_accepts_keystrokes(self.field_focus)
            || self.field_owns_keys
            || self.input_blocked()
        {
            return;
        }
        if self.active_tab().is_some_and(|tab| tab.is_read_only()) {
            self.show_error("PDF files are read-only.");
            return;
        }
        if let Some(tab) = self.active_tab_mut() {
            tab.smart_newline(true);
        }
        self.refresh_find();
    }

    fn editor_table_row_below(&mut self, _cx: &mut App) {
        if !document_accepts_keystrokes(self.field_focus)
            || self.field_owns_keys
            || self.input_blocked()
        {
            return;
        }
        if self.active_tab().is_some_and(|tab| tab.is_read_only()) {
            self.show_error("PDF files are read-only.");
            return;
        }
        if let Some(tab) = self.active_tab_mut() {
            tab.table_insert_row_below();
        }
        self.refresh_find();
    }

    fn editor_indent(&mut self, _cx: &mut App) {
        if !document_accepts_keystrokes(self.field_focus)
            || self.field_owns_keys
            || self.input_blocked()
        {
            return;
        }
        if self.active_tab().is_some_and(|tab| tab.is_read_only()) {
            self.show_error("PDF files are read-only.");
            return;
        }
        if let Some(tab) = self.active_tab_mut() {
            if !tab.tab_table_cell(false) {
                tab.indent();
            }
        }
        self.refresh_find();
    }

    fn editor_outdent(&mut self, _cx: &mut App) {
        if !document_accepts_keystrokes(self.field_focus)
            || self.field_owns_keys
            || self.input_blocked()
        {
            return;
        }
        if self.active_tab().is_some_and(|tab| tab.is_read_only()) {
            self.show_error("PDF files are read-only.");
            return;
        }
        if let Some(tab) = self.active_tab_mut() {
            if !tab.tab_table_cell(true) {
                tab.outdent();
            }
        }
        self.refresh_find();
    }

    fn editor_move(&mut self, _cx: &mut App, edit: impl FnOnce(&mut DocumentTab)) {
        if !document_accepts_keystrokes(self.field_focus)
            || self.field_owns_keys
            || self.input_blocked()
        {
            return;
        }
        if let Some(tab) = self.active_tab_mut() {
            edit(tab);
        }
    }

    fn editor_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Clicking the editor hands keyboard focus back from the sidebar.
        self.sidebar_keyboard = false;
        self.workspace_menu_open = false;
        // The inline create row stops its own presses, so any press that gets
        // here is a click away from it.
        if self.create_draft.is_some() {
            self.cancel_create(cx);
        }
        // A click outside the sidebar, or a plain click that isn't a
        // Shift+click, ends a multi-selection. A press inside the sidebar keeps
        // it: that press may be the start of dragging the whole selection, and
        // clicking a row clears it itself.
        let in_sidebar = self.show_sidebar && event.position.x < crate::theme::sidebar_width(window.viewport_size().width);
        if !window.modifiers().shift && !in_sidebar {
            self.multi_selection = None;
            self.selected_target = None;
        }
        if self.input_blocked() || window.default_prevented() {
            return;
        }
        if self.settings.view_mode == ViewMode::Wysiwyg {
            let checkbox_range = {
                let paint = self.editor_paint.borrow();
                checkbox_for_position(&paint, event.position)
            };
            if let Some(range) = checkbox_range {
                if let Some(tab) = self.active_tab_mut() {
                    if !tab.is_read_only() && range.end <= tab.content.len() {
                        let current = &tab.content[range.clone()];
                        let replacement = if current == "[ ]" { "[x]" } else { "[ ]" };
                        tab.edit(range, replacement);
                    }
                }
                window.refresh();
                cx.notify();
                return;
            }
        }

        let (offset, clicked_link) = {
            let paint = self.editor_paint.borrow();
            let off = index_for_position(&paint, event.position);
            let link = off.and_then(|o| link_for_offset(&paint, o));
            (off, link)
        };
        let Some(offset) = offset else {
            return;
        };

        if (event.modifiers.control || event.modifiers.platform)
            && self.settings.view_mode == ViewMode::Wysiwyg
        {
            if let Some(url) = clicked_link {
                let _ = std::process::Command::new("xdg-open").arg(&url).spawn();
                return;
            }
        }

        self.sidebar_menu = None;
        self.editor_context_menu = None;
        self.focus_handle.focus(window);
        self.applied_focus = Some(FocusSlot::Editor);
        self.field_focus = TextInput::Editor;
        self.field_owns_keys = false;
        if let Some(tab) = self.active_tab_mut() {
            if event.click_count >= 3 {
                tab.place(offset, false);
                tab.select_line();
            } else if event.click_count == 2 {
                tab.place(offset, false);
                tab.select_word();
            } else {
                tab.place(offset, event.modifiers.shift);
            }
        }
        self.selecting = event.click_count <= 1;
        window.refresh();
        cx.notify();
    }

    fn editor_mouse_move(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        if !self.selecting || self.input_blocked() {
            return;
        }
        let offset = {
            let paint = self.editor_paint.borrow();
            index_for_position(&paint, event.position)
        };
        let Some(offset) = offset else {
            return;
        };
        if let Some(tab) = self.active_tab_mut() {
            tab.place(offset, true);
        }
        cx.notify();
    }

    fn editor_right_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.input_blocked() || window.default_prevented() {
            return;
        }

        let offset = {
            let paint = self.editor_paint.borrow();
            index_for_position(&paint, event.position)
        };
        let Some(offset) = offset else {
            self.editor_context_menu = None;
            return;
        };

        self.sidebar_menu = None;
        self.focus_handle.focus(window);
        self.applied_focus = Some(FocusSlot::Editor);
        self.field_focus = TextInput::Editor;
        self.field_owns_keys = false;

        if let Some(tab) = self.active_tab_mut() {
            let sel = tab.selection_range();
            if sel.is_empty() || !sel.contains(&offset) {
                tab.place(offset, false);
            }
        }

        let is_table = self.active_tab().is_some_and(|tab| tab.active_formats.table);
        self.open_menu = None;
        self.editor_context_menu = Some(EditorContextMenu {
            position: event.position,
            is_table,
        });

        window.refresh();
        cx.notify();
    }

    fn active_content(&self) -> String {
        self.active_tab()
            .map(|tab| tab.content.clone())
            .unwrap_or_default()
    }

    fn editor_surface(
        &mut self,
        cx: &mut Context<Self>,
        theme: &Theme,
        show_line_numbers: bool,
    ) -> AnyElement {
        let zoom = self.settings.zoom_level as f32 / 100.0;
        let view_mode = self.settings.view_mode;
        let (content, parsed, selection, marked) = if let Some(tab) = self.active_tab_mut() {
            let parsed = tab.parsed();
            (
                tab.content.clone(),
                parsed,
                tab.selection_range(),
                tab.marked_range.clone(),
            )
        } else {
            (
                String::new(),
                Arc::new(parse_document("")),
                0..0,
                None,
            )
        };
        let font_family = match view_mode {
            ViewMode::Wysiwyg => Theme::FONT_PROSE.into(),
            ViewMode::Raw => Theme::FONT_MONO.into(),
        };
        EditorElement {
            entity: cx.entity(),
            focus: self.focus_handle.clone(),
            paint: self.editor_paint.clone(),
            content: content.into(),
            parsed,
            view_mode,
            selection,
            marked,
            find_hits: self.find_hits.clone(),
            active_find: self.find_index,
            show_line_numbers,
            word_wrap: self.settings.word_wrap,
            centered: self.settings.content_alignment == ContentAlignment::Centered,
            max_measure: Some(sz::MEASURE_FALLBACK * zoom),
            font_size: px(self.settings.base_font_size * zoom),
            text_color: theme.text_primary.into(),
            strong_color: theme.text_strong.into(),
            muted_color: theme.text_muted.into(),
            faint_color: theme.text_faint.into(),
            h1_color: theme.h1.into(),
            h2_color: theme.h2.into(),
            code_color: theme.code_text.into(),
            code_bg: theme.tile_hover.into(),
            surface_bg: theme.bg_surface.into(),
            border_color: theme.border_subtle.into(),
            link_color: theme.text_hi.into(),
            selection_color: theme.selection_bg.into(),
            find_color: find_match_color(theme.mode.is_dark(), false),
            cursor_color: theme.text_strong.into(),
            active_find_color: find_match_color(theme.mode.is_dark(), true),
            gutter_color: theme.text_muted.into(),
            gutter_bg: theme.bg_sidebar.into(),
            font_family,
            mono_font_family: Theme::FONT_MONO.into(),
        }
        .into_any_element()
    }
}

fn planned_title_fallback() -> String {
    "Saved".into()
}

impl EntityInputHandler for LiteMarkApp {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let content = self.active_content();
        let mut range = range_from_utf16(&content, &range_utf16);
        range.start = range.start.min(content.len());
        range.end = range.end.min(content.len());
        if !content.is_char_boundary(range.start) || !content.is_char_boundary(range.end) {
            return None;
        }
        *adjusted_range = Some(range_to_utf16(&content, &range));
        Some(content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let tab = self.active_tab()?;
        Some(UTF16Selection {
            range: range_to_utf16(&tab.content, &tab.selection_range()),
            reversed: tab.caret.reversed,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        let tab = self.active_tab()?;
        tab.marked_range
            .as_ref()
            .map(|range| range_to_utf16(&tab.content, range))
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(tab) = self.active_tab_mut() {
            tab.marked_range = None;
        }
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.forward_keystroke_to_field(text, window, cx) || self.input_blocked() {
            return;
        }
        if self.active_tab().is_some_and(|tab| tab.is_read_only()) {
            self.show_error("PDF files are read-only.");
            window.refresh();
            cx.notify();
            return;
        }
        let content = self.active_content();
        let mut range = range_utf16
            .as_ref()
            .map(|range| range_from_utf16(&content, range))
            .or_else(|| self.active_tab().and_then(|tab| tab.marked_range.clone()))
            .or_else(|| self.active_tab().map(|tab| tab.selection_range()))
            .unwrap_or(0..0);

        // 1. In WYSIWYG mode, preserve block prefixes when typing at start of line
        if self.settings.view_mode == ViewMode::Wysiwyg && range.is_empty() && text != "\n" {
            let cursor = range.start;
            let line_start = home_offset(&content, cursor);
            let line_end_pos = line_end(&content, cursor);
            let line = &content[line_start..line_end_pos];
            let prefix_len = block_prefix_len(line);
            if prefix_len > 0 && cursor < line_start + prefix_len {
                let target = line_start + prefix_len;
                range = target..target;
            }
        }

        // 2. Handle Enter / Newline
        if text == "\n" {
            if let Some(tab) = self.active_tab_mut() {
                tab.smart_newline(false);
            }
            self.refresh_find();
            window.refresh();
            cx.notify();
            return;
        }

        // 2b. If WYSIWYG pending inline formats are armed, wrap newly typed text directly
        let has_pending = self
            .active_tab()
            .is_some_and(|tab| tab.has_pending_inline_formats());
        if self.settings.view_mode == ViewMode::Wysiwyg && range.is_empty() && has_pending {
            let mode = self.settings.view_mode;
            if let Some(tab) = self.active_tab_mut() {
                tab.insert_text_in_mode(range, text, mode);
            }
            self.refresh_find();
            window.refresh();
            cx.notify();
            return;
        }

        // 3. Selection auto-wrapping or typing auto-pairing
        if text.chars().count() == 1 {
            let ch = text.chars().next().unwrap();
            if !range.is_empty() {
                let closing = match ch {
                    '(' => Some(')'),
                    '[' => Some(']'),
                    '{' => Some('}'),
                    '"' => Some('"'),
                    '\'' => Some('\''),
                    '`' => Some('`'),
                    '*' => Some('*'),
                    '_' => Some('_'),
                    '~' => Some('~'),
                    _ => None,
                };
                if let Some(close_ch) = closing {
                    let selected = &content[range.clone()];
                    let wrapped = format!("{}{}{}", ch, selected, close_ch);
                    if let Some(tab) = self.active_tab_mut() {
                        tab.edit(range.clone(), &wrapped);
                        tab.caret.range = (range.start + 1)..(range.start + 1 + selected.len());
                        tab.caret.reversed = false;
                        tab.refresh_cursor();
                    }
                    self.refresh_find();
                    window.refresh();
                    cx.notify();
                    return;
                }
            } else {
                let cursor = range.start;
                // Over-typing closing bracket or quote
                if (ch == ')' || ch == ']' || ch == '}' || ch == '"' || ch == '\'' || ch == '`')
                    && cursor < content.len()
                    && content[cursor..].starts_with(ch)
                {
                    if let Some(tab) = self.active_tab_mut() {
                        tab.caret.move_to(cursor + ch.len_utf8());
                        tab.refresh_cursor();
                    }
                    window.refresh();
                    cx.notify();
                    return;
                }

                // Auto-closing brackets and backticks
                let auto_pair = match ch {
                    '(' => Some("()"),
                    '[' => Some("[]"),
                    '{' => Some("{}"),
                    '`' => Some("``"),
                    '"' => {
                        let prev_is_alnum = cursor > 0
                            && content[..cursor]
                                .chars()
                                .next_back()
                                .is_some_and(|c| c.is_alphanumeric());
                        if !prev_is_alnum {
                            Some("\"\"")
                        } else {
                            None
                        }
                    }
                    _ => None,
                };

                if let Some(pair) = auto_pair {
                    if let Some(tab) = self.active_tab_mut() {
                        tab.edit(range, pair);
                        tab.caret.move_to(cursor + ch.len_utf8());
                        tab.refresh_cursor();
                    }
                    self.refresh_find();
                    window.refresh();
                    cx.notify();
                    return;
                }
            }
        }

        let mode = self.settings.view_mode;
        if let Some(tab) = self.active_tab_mut() {
            tab.insert_text_in_mode(range, text, mode);
        }
        self.refresh_find();
        window.refresh();
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.forward_keystroke_to_field(new_text, window, cx) {
            return;
        }
        if self.active_tab().is_some_and(|tab| tab.is_read_only()) {
            self.show_error("PDF files are read-only.");
            cx.notify();
            window.refresh();
            return;
        }
        let content = self.active_content();
        let range = range_utf16
            .as_ref()
            .map(|range| range_from_utf16(&content, range))
            .or_else(|| self.active_tab().and_then(|tab| tab.marked_range.clone()))
            .or_else(|| self.active_tab().map(|tab| tab.selection_range()))
            .unwrap_or(0..0);
        let start = range.start;
        if let Some(tab) = self.active_tab_mut() {
            tab.edit(range, new_text);
            if new_text.is_empty() {
                tab.marked_range = None;
            } else {
                let end = (start + new_text.len()).min(tab.content.len());
                tab.marked_range = Some(start..end);
            }
            if let Some(selected) = new_selected_range {
                let relative = range_from_utf16(new_text, &selected);
                let sel_start = (start + relative.start).min(tab.content.len());
                let sel_end = (start + relative.end).min(tab.content.len());
                tab.caret.range = sel_start..sel_end;
                tab.caret.reversed = false;
                tab.refresh_cursor();
            }
        }
        self.refresh_find();
        window.refresh();
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        _element_bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let content = self.active_content();
        let range = range_from_utf16(&content, &range_utf16);
        let paint = self.editor_paint.borrow();
        let row = paint
            .rows
            .iter()
            .rev()
            .find(|row| row.byte_start <= range.start)?;
        if !row.cells.is_empty() {
            if let Some(cell) = row
                .cells
                .iter()
                .find(|c| c.raw_range.start <= range.start && range.start <= c.raw_range.end)
            {
                if let Some(line) = cell
                    .lines
                    .iter()
                    .find(|l| {
                        range.start >= l.offset_map.line_start && range.start <= l.offset_map.line_end
                    })
                    .or_else(|| cell.lines.first())
                {
                    let local_start = line.offset_map.source_to_display(range.start);
                    let local_end = line.offset_map.source_to_display(range.end);
                    let x0 = line.shaped.x_for_index(local_start);
                    let x1 = line.shaped.x_for_index(local_end);
                    return Some(Bounds::from_corners(
                        point(line.text_origin.x + x0, line.text_origin.y),
                        point(line.text_origin.x + x1.max(x0), line.text_origin.y + line.line_height),
                    ));
                }
                let local_start = cell.offset_map.source_to_display(range.start);
                let local_end = cell.offset_map.source_to_display(range.end);
                let x0 = cell.shaped.x_for_index(local_start);
                let x1 = cell.shaped.x_for_index(local_end);
                return Some(Bounds::from_corners(
                    point(cell.text_origin.x + x0, cell.text_origin.y),
                    point(cell.text_origin.x + x1.max(x0), cell.text_origin.y + row.line_height),
                ));
            }
        }
        let local_start = row.offset_map.source_to_display(range.start);
        let local_end = row.offset_map.source_to_display(range.end);
        let x0 = row.shaped.x_for_index(local_start);
        let x1 = row.shaped.x_for_index(local_end);
        Some(Bounds::from_corners(
            point(row.origin.x + x0, row.origin.y),
            point(row.origin.x + x1.max(x0), row.origin.y + row.line_height),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let utf8 = {
            let paint = self.editor_paint.borrow();
            index_for_position(&paint, point)?
        };
        Some(offset_to_utf16(&self.active_content(), utf8))
    }
}

impl Render for LiteMarkApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::resolve(self.settings.theme, window.appearance());
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

        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == self.active_tab_id) {
            tab.ensure_stats();
        }
        let active_tab = self.active_tab().cloned().unwrap_or_else(|| {
            DocumentTab::new(
                "tab-1".to_string(),
                "Welcome.md".to_string(),
                INITIAL_MARKDOWN.to_string(),
                None,
                None,
            )
        });

        let active_formats = active_tab.active_formats;
        let stats_line = active_tab.cursor_line;
        let stats_words = active_tab.word_count;
        let stats_chars = active_tab.char_count;
        let stats_selection = {
            let text = active_tab.selected_text();
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
            let tab_id = active_tab.id.clone();
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
                        this.pending_find_focus = true;
                    } else {
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
                    this.editor_context_menu = None;
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
                let _ = view.update(cx, |this, _cx| {
                    if let Some(ref path) = this.active_tab().and_then(|t| t.file_path.clone()) {
                        let _ = std::process::Command::new("xdg-open").arg(path).spawn();
                    }
                });
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
                        self.settings.show_line_numbers,
                        self.settings.word_wrap,
                        self.show_sidebar,
                        self.settings.content_alignment,
                        on_set_view_mode.clone(),
                        on_toggle_line_numbers.clone(),
                        on_toggle_word_wrap.clone(),
                        on_set_content_alignment.clone(),
                        on_zoom_in.clone(),
                        on_zoom_out.clone(),
                        on_reset_zoom.clone(),
                        on_toggle_sidebar.clone(),
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
                        self.settings.show_line_numbers,
                        self.settings.word_wrap,
                        self.show_sidebar,
                        self.settings.content_alignment,
                        on_set_view_mode.clone(),
                        on_toggle_line_numbers.clone(),
                        on_toggle_word_wrap.clone(),
                        on_set_content_alignment.clone(),
                        on_zoom_in.clone(),
                        on_zoom_out.clone(),
                        on_reset_zoom.clone(),
                        on_toggle_sidebar.clone(),
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

        div()
            .id("litemark-root")
            .track_focus(&self.focus_handle)
            .key_context("Editor")
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .bg(theme.bg_editor)
            .text_color(theme.text_primary)
            .font_family(Theme::FONT_PROSE)
            // Keyboard shortcut bindings
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
                // Only act in a normal window state — never while a menu,
                // modal or naming editor is open.
                if this.open_menu.is_some()
                    || this.modal_choice_count() > 0
                    || this.rename_target.is_some()
                    || this.create_draft.is_some()
                {
                    return;
                }
                if let Some(items) = this
                    .active_multi()
                    .filter(|_| this.sidebar_keyboard)
                    .map(|m| m.items.clone())
                {
                    this.request_trash_many(items);
                    cx.notify();
                    return;
                }
                // Target whatever is selected in the sidebar (folder or
                // note); fall back to the active tab's note.
                let target = this.selected_target.clone().or_else(|| {
                    this.active_tab()
                        .and_then(|tab| tab.workspace_file_id.clone())
                        .map(SidebarTarget::File)
                });
                match target {
                    Some(SidebarTarget::Folder(id)) => {
                        this.request_trash_folder(id);
                        cx.notify();
                    }
                    Some(SidebarTarget::File(id)) => {
                        this.request_trash_file(id);
                        cx.notify();
                    }
                    None => {
                        let Some(tab) = this.active_tab() else {
                            return;
                        };
                        if tab.file_path.is_none() {
                            this.show_error(
                                "This note isn't saved yet — press Ctrl+S to name and save it first.",
                            );
                        } else {
                            this.show_error(
                                "This file isn't in the workspace folder, so it can't be trashed here.",
                            );
                        }
                    }
                }
            }))
            .on_action(cx.listener(|this, _: &CloseActiveTab, _window, cx| {
                let id = this.active_tab_id.clone();
                this.handle_close_tab(id);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &FindReplace, _window, cx| {
                this.show_find_replace = !this.show_find_replace;
                if this.show_find_replace {
                    this.pending_find_focus = true;
                } else {
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
                    this.editor_context_menu = None;
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
                            let _ = std::process::Command::new("xdg-open").arg(path).spawn();
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
                let mode = this.settings.view_mode;
                this.editor_move(cx, |tab| tab.move_left_in_mode(false, mode));
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &EditorRight, window, cx| {
                let mode = this.settings.view_mode;
                this.editor_move(cx, |tab| tab.move_right_in_mode(false, mode));
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &EditorUp, window, cx| {
                let paint = this.editor_paint.clone();
                let mode = this.settings.view_mode;
                this.editor_move(cx, |tab| {
                    let p = paint.borrow();
                    if mode == ViewMode::Wysiwyg
                        && !cell_has_line_in_direction(&p, tab.cursor_offset(), -1)
                        && tab.table_edge_exit(-1)
                    {
                        return;
                    }
                    if move_vertical_visual(&p, &tab.content, &mut tab.caret, -1, false, mode) {
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
                let mode = this.settings.view_mode;
                this.editor_move(cx, |tab| {
                    let p = paint.borrow();
                    if mode == ViewMode::Wysiwyg
                        && !cell_has_line_in_direction(&p, tab.cursor_offset(), 1)
                        && tab.table_edge_exit(1)
                    {
                        return;
                    }
                    if move_vertical_visual(&p, &tab.content, &mut tab.caret, 1, false, mode) {
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
                let mode = this.settings.view_mode;
                this.editor_move(cx, |tab| tab.move_left_in_mode(true, mode));
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &EditorSelectRight, window, cx| {
                let mode = this.settings.view_mode;
                this.editor_move(cx, |tab| tab.move_right_in_mode(true, mode));
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &EditorSelectUp, window, cx| {
                let paint = this.editor_paint.clone();
                let mode = this.settings.view_mode;
                this.editor_move(cx, |tab| {
                    let p = paint.borrow();
                    if move_vertical_visual(&p, &tab.content, &mut tab.caret, -1, true, mode) {
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
                let mode = this.settings.view_mode;
                this.editor_move(cx, |tab| {
                    let p = paint.borrow();
                    if move_vertical_visual(&p, &tab.content, &mut tab.caret, 1, true, mode) {
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
                let mode = this.settings.view_mode;
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
                let mode = this.settings.view_mode;
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
                let mode = this.settings.view_mode;
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
                let mode = this.settings.view_mode;
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
                if let Some(tab) = this.active_tab_mut() {
                    tab.move_line_up();
                }
                this.refresh_find();
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &EditorMoveLineDown, window, cx| {
                if let Some(tab) = this.active_tab_mut() {
                    tab.move_line_down();
                }
                this.refresh_find();
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
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                this.editor_mouse_move(event, cx);
                window.refresh();
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, window, cx| {
                    if this.selecting {
                        this.selecting = false;
                    }
                    window.refresh();
                    cx.notify();
                }),
            )
            .on_action(cx.listener(|this, _: &CloseModal, window, cx| {
                if this.editor_context_menu.is_some() {
                    this.editor_context_menu = None;
                } else if this.workspace_menu_open {
                    this.workspace_menu_open = false;
                } else if this.sidebar_menu.is_some() {
                    this.sidebar_menu = None;
                } else if this.rename_target.is_some() || this.save_as_tab.is_some() {
                    this.cancel_rename(cx);
                } else if this.create_draft.is_some() {
                    this.cancel_create(cx);
                } else if this.prompt.is_some() {
                    this.prompt = None;
                } else if this.error_message.is_some()
                    && this.open_menu.is_none()
                    && !this.show_about_modal
                    && !this.show_shortcuts_modal
                    && !this.show_pdf_modal
                    && !this.show_find_replace
                    && !this.zen_mode
                {
                    this.error_message = None;
                } else if this.open_menu.is_some() {
                    this.open_menu = None;
                    this.menu_selected_ix = None;
                    this.menu_open_submenu_ix = None;
                } else if this.show_about_modal {
                    this.show_about_modal = false;
                } else if this.show_shortcuts_modal {
                    this.show_shortcuts_modal = false;
                } else if this.show_pdf_modal {
                    this.show_pdf_modal = false;
                } else if this.show_find_replace {
                    this.show_find_replace = false;
                } else if this.field_focus == TextInput::Search || !this.search_query.is_empty() {
                    this.clear_search(cx);
                } else if this.active_multi().is_some() || this.selected_target.is_some() {
                    this.multi_selection = None;
                    this.selected_target = None;
                    this.sidebar_keyboard = false;
                } else if this.show_sidebar
                    && !this.zen_mode
                    && window.viewport_size().width < px(600.0)
                {
                    // The sidebar is a drawer on narrow windows.
                    this.show_sidebar = false;
                } else if this.zen_mode {
                    this.handle_exit_zen();
                }
                cx.notify();
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
                            tabs: self.tabs.clone(),
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
                                        workspace: self.workspace.clone(),
                                        width: crate::theme::sidebar_width(window_width),
                                        selected_file_id: match &self.selected_target {
                                            Some(SidebarTarget::File(id)) => Some(id.clone()),
                                            _ => None,
                                        },
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
                                        on_close: on_toggle_sidebar.clone(),
                                        on_context_menu: on_sidebar_context.clone(),
                                        on_drop_item: on_drop_item.clone(),
                                        on_range_select: on_range_select.clone(),
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
                            view_mode: self.settings.view_mode,
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
                let title = active_tab.title.clone();
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
                let has_selection = !active_tab.selection_range().is_empty();
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

/// Translucent amber wash painted behind find matches, so the text on top
/// keeps its own color and stays readable. The current match is stronger.
fn find_match_color(dark: bool, active: bool) -> Hsla {
    let color = match (dark, active) {
        (false, false) => rgba(0xffd54f73),
        (false, true) => rgba(0xff9800a6),
        (true, false) => rgba(0xe6a70033),
        (true, true) => rgba(0xf59e0b6b),
    };
    color.into()
}
