use crate::actions::*;
use crate::markdown::{export_stem, parse_document, render_export};
use crate::model::text_ops::{
    home_offset, line_end, next_grapheme, offset_to_utf16, range_from_utf16, range_to_utf16,
};
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Prompt {
    CloseTab { id: String },
    CloseWindow,
    DeleteFile { id: String },
    DeleteFolder { id: String },
    TrashDirtyFile { id: String },
    TrashDirtyFolder { id: String },
}

pub struct LiteMarkApp {
    pub tabs: Vec<DocumentTab>,
    pub active_tab_id: String,
    pub workspace: WorkspaceDirectory,
    pub settings: EditorSettings,
    pub show_sidebar: bool,
    pub show_find_replace: bool,
    pub show_shortcuts_modal: bool,
    pub show_pdf_modal: bool,
    pub show_about_modal: bool,
    pub open_menu: Option<MenuId>,
    pub menu_selected_ix: Option<usize>,
    pub menu_open_submenu_ix: Option<usize>,
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
    pub editor_paint: Rc<RefCell<EditorPaint>>,
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
    pub rename_target: Option<SidebarTarget>,
    pub rename_field: Entity<LineField>,
    pub field_owns_keys: bool,
    pub field_focus: TextInput,
    pub bounds_subscription: Option<Subscription>,
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
                        move |previous, app| {
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
                    Arc::new(|_, _| {}),
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
                    Arc::new(|_, _| {}),
                    Arc::new({
                        let app_handle = app_handle.clone();
                        move |_previous, app| {
                            app_handle
                                .update(app, |this, cx| {
                                    this.commit_rename(cx);
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

        Self {
            tabs: vec![initial_tab],
            active_tab_id: first_id,
            workspace,
            settings,
            show_sidebar: true,
            show_find_replace: false,
            show_shortcuts_modal: false,
            show_pdf_modal: false,
            show_about_modal: false,
            open_menu: None,
            menu_selected_ix: None,
            menu_open_submenu_ix: None,
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
            appearance_subscription: None,
            tab_ids,
            error_message,
            prompt: None,
            editor_paint: Rc::new(RefCell::new(EditorPaint::default())),
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
            rename_target: None,
            rename_field,
            field_owns_keys: false,
            field_focus: TextInput::Editor,
            bounds_subscription: None,
            title_drag_armed: Rc::new(Cell::new(false)),
        }
    }

    fn claim_field(&mut self, which: TextInput, cx: &mut Context<Self>) {
        self.field_focus = which;
        self.field_owns_keys = !document_accepts_keystrokes(which);
        if which == TextInput::Editor {
            self.applied_focus = None;
        }
        cx.notify();
    }

    fn line_field_for_focus(&self) -> Option<Entity<LineField>> {
        match self.field_focus {
            TextInput::Find if self.show_find_replace => Some(self.find_field.clone()),
            TextInput::Replace if self.show_find_replace => Some(self.replace_field.clone()),
            TextInput::Rename if self.rename_target.is_some() => Some(self.rename_field.clone()),
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
        }
        window.refresh();
        cx.notify();
        true
    }

    pub fn active_tab(&self) -> Option<&DocumentTab> {
        self.tabs.iter().find(|t| t.id == self.active_tab_id)
    }

    pub fn active_tab_mut(&mut self) -> Option<&mut DocumentTab> {
        self.tabs.iter_mut().find(|t| t.id == self.active_tab_id)
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
            ClosePlan::Prompt => self.prompt = Some(Prompt::CloseTab { id }),
            ClosePlan::Close { next_active } => self.remove_tab(&id, next_active),
        }
    }

    pub fn handle_save(&mut self, cx: &mut Context<Self>) {
        let id = self.active_tab_id.clone();
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

    pub fn handle_insert_table(&mut self, cx: &mut App) {
        self.handle_format_cmd("table", cx);
    }

    pub fn handle_insert_link(&mut self, cx: &mut App) {
        self.handle_format_cmd("link", cx);
    }

    pub fn handle_toggle_sidebar(&mut self) {
        self.show_sidebar = !self.show_sidebar;
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

    pub fn handle_new_folder(&mut self) {
        self.create_folder(None, "New Folder".into());
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

    pub fn handle_cut(&mut self, cx: &mut App) {
        let text = self
            .active_tab()
            .map(|tab| tab.selected_text())
            .unwrap_or_default();
        if text.is_empty() {
            return;
        }
        cx.write_to_clipboard(ClipboardItem::new_string(text));
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
        if let Some(tab) = self.active_tab() {
            let text = tab.selected_text();
            if !text.is_empty() {
                cx.write_to_clipboard(ClipboardItem::new_string(text));
            }
        }
    }

    pub fn handle_paste(&mut self, cx: &mut App) {
        if let Some(item) = cx.read_from_clipboard() {
            if let Some(text) = item.text() {
                if self.active_tab().is_some_and(|tab| tab.is_read_only()) {
                    self.show_error("PDF files are read-only.");
                    return;
                }
                if let Some(tab) = self.active_tab_mut() {
                    let range = tab.selection_range();
                    tab.edit(range, text.as_ref());
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
            "No matches".into()
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
        }
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

    fn create_note(&mut self, folder_id: Option<String>, desired: String) {
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
            }
            Err(err) => self.show_error(format!("Couldn't create note: {err}")),
        }
    }

    fn create_folder(&mut self, parent_id: Option<String>, desired: String) {
        let folders = self.folder_pairs();
        let dir = child_directory(&self.workspace.folder_path, &folders, parent_id.as_deref());
        let name = unique_name(&sanitize_name(&desired), &|candidate| {
            dir.join(candidate).exists()
        });
        match safe_create_folder(&dir, &name) {
            Ok(_) => {
                if let Some(pid) = &parent_id {
                    if let Some(folder) = self.workspace.folders.iter_mut().find(|f| &f.id == pid) {
                        folder.is_open = true;
                    }
                }
                self.rescan_workspace();
            }
            Err(err) => self.show_error(format!("Couldn't create folder: {err}")),
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
        self.prompt = Some(match plan_trash(modified) {
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
        self.prompt = Some(match plan_trash(modified) {
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
        self.pending_rename_focus = true;
        self.rename_field.update(cx, |field, _cx| {
            field.set_text(name);
        });
    }

    fn commit_rename(&mut self, cx: &mut Context<Self>) {
        let Some(target) = self.rename_target.take() else {
            return;
        };
        let name = self.rename_field.read(cx).content().trim().to_string();
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
                self.retarget_open_tabs(&old, &new_path);
                self.rescan_workspace();
            }
            Err(err) => {
                self.show_error(format!("Couldn't rename: {err}"));
                self.rename_target = Some(target);
            }
        }
    }

    fn trash_file(&mut self, id: &str) {
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
        match move_to_trash(&file.path) {
            Ok(()) => {
                let closing: Vec<String> = self
                    .tabs
                    .iter()
                    .filter(|tab| tab.file_path.as_ref() == Some(&file.path))
                    .map(|tab| tab.id.clone())
                    .collect();
                self.rescan_workspace();
                for tab_id in closing {
                    self.discard_tab(&tab_id);
                }
            }
            Err(err) => self.show_error(err),
        }
    }

    fn trash_folder(&mut self, id: &str) {
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
        match move_to_trash(&folder.path) {
            Ok(()) => {
                let prefix = folder.path.clone();
                let closing: Vec<String> = self
                    .tabs
                    .iter()
                    .filter(|tab| {
                        tab.file_path
                            .as_ref()
                            .is_some_and(|path| path.starts_with(&prefix))
                    })
                    .map(|tab| tab.id.clone())
                    .collect();
                self.rescan_workspace();
                for tab_id in closing {
                    self.discard_tab(&tab_id);
                }
            }
            Err(err) => self.show_error(err),
        }
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

    fn answer_prompt(&mut self, choice: &'static str, cx: &mut Context<Self>) {
        let Some(prompt) = self.prompt.take() else {
            return;
        };
        match prompt {
            Prompt::CloseTab { id } => match choice {
                "Save" => match self.save_tab(&id) {
                    Ok(title) => {
                        self.show_toast(format!("Saved {title}"), cx);
                        self.finish_close_saved(&id);
                    }
                    Err(err) => {
                        self.show_error(err);
                        self.prompt = Some(Prompt::CloseTab { id });
                    }
                },
                "Discard" => self.discard_tab(&id),
                _ => {}
            },
            Prompt::CloseWindow => match choice {
                "Save" => match self.save_all_dirty() {
                    Ok(()) => self.allow_window_close = true,
                    Err(err) => {
                        self.show_error(err);
                        self.prompt = Some(Prompt::CloseWindow);
                    }
                },
                "Discard" => self.allow_window_close = true,
                _ => {}
            },
            Prompt::DeleteFile { id } => {
                if choice == "Trash" {
                    self.trash_file(&id);
                }
            }
            Prompt::DeleteFolder { id } => {
                if choice == "Trash" {
                    self.trash_folder(&id);
                }
            }
            Prompt::TrashDirtyFile { id } => match trash_action(match choice {
                "Save" => TrashChoice::Save,
                "Discard" => TrashChoice::Discard,
                _ => TrashChoice::Cancel,
            }) {
                TrashAction::Abort => {}
                TrashAction::DiscardThenTrash => self.trash_file(&id),
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
                        self.prompt = Some(Prompt::TrashDirtyFile { id });
                        return;
                    }
                    self.trash_file(&id);
                }
            },
            Prompt::TrashDirtyFolder { id } => match trash_action(match choice {
                "Save" => TrashChoice::Save,
                "Discard" => TrashChoice::Discard,
                _ => TrashChoice::Cancel,
            }) {
                TrashAction::Abort => {}
                TrashAction::DiscardThenTrash => self.trash_folder(&id),
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
                        self.prompt = Some(Prompt::TrashDirtyFolder { id });
                        return;
                    }
                    self.trash_folder(&id);
                }
            },
        }
    }

    fn request_close_window(&mut self) {
        if self.tabs.iter().any(|tab| tab.is_modified) {
            self.prompt = Some(Prompt::CloseWindow);
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
                    this.prompt = Some(Prompt::CloseWindow);
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
        if let Some(tab) = self.active_tab_mut() {
            let range = if tab.selection_range().is_empty() {
                let cursor = tab.cursor_offset();
                cursor..next_grapheme(&tab.content, cursor)
            } else {
                tab.selection_range()
            };
            tab.edit(range, "");
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
            find_color: theme.text_hi.into(),
            cursor_color: theme.text_strong.into(),
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
        if self.rename_target.is_none() && self.field_focus == TextInput::Rename {
            self.field_focus = TextInput::Editor;
            self.field_owns_keys = false;
            self.applied_focus = None;
        }
        if self.pending_find_focus && self.show_find_replace {
            self.pending_find_focus = false;
            self.field_focus = TextInput::Find;
        }
        if self.pending_rename_focus && self.rename_target.is_some() {
            self.pending_rename_focus = false;
            self.field_focus = TextInput::Rename;
        }
        let next_focus = if self.open_menu.is_some() {
            FocusSlot::Menu
        } else {
            FocusSlot::Editor
        };
        if self.open_menu.is_some() {
            if focus_if_changed(self.applied_focus, next_focus) {
                self.menu_focus_handle.focus(window);
                self.applied_focus = Some(FocusSlot::Menu);
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
        let stats_col = active_tab.cursor_col;
        let stats_words = active_tab.word_count;
        let stats_chars = active_tab.char_count;
        let stats_lines = active_tab.line_count;
        let stats_read = active_tab.reading_time_min;
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
                    this.handle_new_folder();
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
                    cx.notify();
                });
            }
        };

        let on_create_file = {
            let view = view.clone();
            move |folder_id: Option<String>, name: String, cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.create_note(folder_id, name);
                    cx.notify();
                });
            }
        };

        let on_create_folder = {
            let view = view.clone();
            move |parent_id: Option<String>, name: String, cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.create_folder(parent_id, name);
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
        let on_rename_cancel = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.rename_target = None;
                    this.field_owns_keys = false;
                    this.applied_focus = None;
                    cx.notify();
                });
            }
        };
        let on_rename_commit = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.commit_rename(cx);
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
                        this.workspace = scan_workspace_dir(&dir);
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

        let menu_overlay = if let Some(open_id) = self.open_menu {
            let (point, anchor, width, items) = match open_id {
                MenuId::File => (
                    Point::new(px(8.0), px(76.0)),
                    Corner::TopLeft,
                    px(220.0),
                    build_file_menu_items(
                        active_tab_has_pdf,
                        on_new_tab.clone(),
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
                        on_new_tab.clone(),
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
                MenuId::Overflow => {
                    let overflow_x = if is_narrow {
                        (window.viewport_size().width / 2.0 + px(128.0))
                            .min(window.viewport_size().width - px(8.0))
                    } else {
                        (window.viewport_size().width / 2.0 + px(270.0))
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
                            on_close_menu.clone(),
                        ),
                    )
                }
            };

            Some(render_dropdown_menu(
                MenuProps {
                    id: ("menu-dropdown", open_id as usize).into(),
                    point,
                    anchor_corner: anchor,
                    width,
                    items,
                    focus_handle: self.menu_focus_handle.clone(),
                    selected_index: self.menu_selected_ix,
                    open_submenu_ix: self.menu_open_submenu_ix,
                    on_close: on_close_menu.clone(),
                    on_set_submenu: on_set_submenu.clone(),
                },
                &theme,
            ))
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
                this.handle_select_all();
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &OpenShortcuts, _window, cx| {
                this.show_shortcuts_modal = true;
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
                if this.menu_open_submenu_ix.is_some() {
                    this.menu_open_submenu_ix = None;
                } else {
                    this.open_menu = None;
                    this.menu_selected_ix = None;
                }
                cx.notify();
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
                this.editor_move(cx, |tab| {
                    let p = paint.borrow();
                    if move_vertical_visual(&p, &tab.content, &mut tab.caret, -1, false) {
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
                this.editor_move(cx, |tab| {
                    let p = paint.borrow();
                    if move_vertical_visual(&p, &tab.content, &mut tab.caret, 1, false) {
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
                this.editor_move(cx, |tab| {
                    let p = paint.borrow();
                    if move_vertical_visual(&p, &tab.content, &mut tab.caret, -1, true) {
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
                this.editor_move(cx, |tab| {
                    let p = paint.borrow();
                    if move_vertical_visual(&p, &tab.content, &mut tab.caret, 1, true) {
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
                let mode = this.settings.view_mode;
                this.editor_move(cx, |tab| tab.move_home_in_mode(false, mode));
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &EditorEnd, window, cx| {
                this.editor_move(cx, |tab| tab.move_end(false));
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &EditorSelectHome, window, cx| {
                let mode = this.settings.view_mode;
                this.editor_move(cx, |tab| tab.move_home_in_mode(true, mode));
                window.refresh();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &EditorSelectEnd, window, cx| {
                this.editor_move(cx, |tab| tab.move_end(true));
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
            .on_action(cx.listener(|this, _: &CloseModal, _window, cx| {
                if this.prompt.is_some() {
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
                            is_narrow,
                            menu_bar_element,
                            settings_gear_element,
                            on_format: on_format.clone(),
                            on_insert_table: on_insert_table.clone(),
                            on_insert_link: on_insert_link.clone(),
                            on_toggle_find: on_toggle_find.clone(),
                            on_toggle_overflow: on_toggle_overflow.clone(),
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
                            .overflow_hidden()
                            .when(self.show_sidebar, |s| {
                                s.child(render_sidebar(
                                    SidebarProps {
                                        workspace: self.workspace.clone(),
                                        active_file_id: active_tab.workspace_file_id.clone(),
                                        open_file_ids: self
                                            .tabs
                                            .iter()
                                            .filter_map(|t| t.workspace_file_id.clone())
                                            .collect(),
                                        search_query: self.search_query.clone(),
                                        creating_in_folder: None,
                                        on_select_file: on_select_file.clone(),
                                        on_create_file: on_create_file.clone(),
                                        on_create_folder: on_create_folder.clone(),
                                        on_delete_file: on_delete_file.clone(),
                                        on_delete_folder: on_delete_folder.clone(),
                                        on_toggle_folder: on_toggle_folder.clone(),
                                        on_choose_dir: on_choose_dir.clone(),
                                        on_close: on_toggle_sidebar.clone(),
                                        on_context_menu: on_sidebar_context.clone(),
                                    },
                                    &theme,
                                ))
                            })
                            .child(render_editor(
                                EditorProps {
                                    zoom_level: self.settings.zoom_level,
                                    show_zoom_hud: self.show_zoom_hud,
                                    editor_element: self.editor_surface(
                                        cx,
                                        &theme,
                                        self.settings.show_line_numbers,
                                    ),
                                },
                                &theme,
                            )),
                    )
                    // 5. StatusBar
                    .child(render_status_bar(
                        StatusBarProps {
                            line: stats_line,
                            col: stats_col,
                            words: stats_words,
                            chars: stats_chars,
                            lines: stats_lines,
                            reading_time_min: stats_read,
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
                s.child(render_shortcuts_modal(on_close_shortcuts.clone(), &theme))
            })
            .when(self.show_pdf_modal, |s| {
                let title = active_tab.title.clone();
                s.child(render_pdf_preview_modal(
                    title,
                    pdf_page_count,
                    on_close_pdf.clone(),
                    on_open_external_pdf.clone(),
                    &theme,
                ))
            })
            .when(self.show_about_modal, |s| {
                s.child(render_about_modal(on_close_about.clone(), &theme))
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
                let (title, message, choices): (&'static str, String, Vec<(&'static str, bool)>) =
                    match &prompt {
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
                    };
                parent.child(render_prompt_modal(
                    title,
                    message,
                    choices,
                    on_prompt.clone(),
                    &theme,
                ))
            })
            .child(render_client_resize_borders(window))
            .when_some(self.sidebar_menu.clone(), |parent, menu| {
                parent.child(render_sidebar_context_menu(
                    &menu,
                    on_sidebar_rename.clone(),
                    on_sidebar_delete.clone(),
                    on_sidebar_dismiss.clone(),
                    &theme,
                ))
            })
            .when(self.rename_target.is_some(), |parent| {
                let cancel = on_rename_cancel.clone();
                let commit = on_rename_commit.clone();
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
                                        .child("Rename"),
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
                                                .child("Rename")
                                                .on_click(move |_, _, cx| commit(cx)),
                                        ),
                                ),
                        ),
                )
            })
    }
}
