use gpui::prelude::*;
use gpui::*;
use std::path::PathBuf;
use crate::actions::*;
use crate::markdown::{export_to_html, export_to_plaintext};
use crate::model::*;
use crate::services::*;
use crate::theme::{Theme, ThemeMode};
use crate::ui::*;
use crate::ui::MenuItem;

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
}

impl LiteMarkApp {
    pub fn new(initial_file: Option<PathBuf>, cx: &mut Context<Self>) -> Self {
        let settings = EditorSettings::load();
        let notes_dir = get_or_create_notes_dir();
        seed_welcome_note(&notes_dir);
        let workspace = scan_workspace_dir(&notes_dir);

        let initial_tab = if let Some(path) = initial_file {
            let name = path.file_name().and_then(|f| f.to_str()).unwrap_or("Untitled.md").to_string();
            let content = std::fs::read_to_string(&path).unwrap_or_default();
            DocumentTab::new("tab-1".to_string(), name, content, Some(path), None)
        } else {
            let welcome_file = workspace.files.iter().find(|f| f.name == "Welcome.md");
            if let Some(wf) = welcome_file {
                let content = std::fs::read_to_string(&wf.path).unwrap_or_else(|_| INITIAL_MARKDOWN.to_string());
                DocumentTab::new("tab-1".to_string(), "Welcome.md".to_string(), content, Some(wf.path.clone()), Some(wf.id.clone()))
            } else {
                DocumentTab::new("tab-1".to_string(), "Welcome.md".to_string(), INITIAL_MARKDOWN.to_string(), None, None)
            }
        };

        Self {
            tabs: vec![initial_tab],
            active_tab_id: "tab-1".to_string(),
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
        }
    }

    pub fn active_tab(&self) -> Option<&DocumentTab> {
        self.tabs.iter().find(|t| t.id == self.active_tab_id)
    }

    pub fn active_tab_mut(&mut self) -> Option<&mut DocumentTab> {
        self.tabs.iter_mut().find(|t| t.id == self.active_tab_id)
    }

    pub fn handle_content_change(&mut self, new_content: String) {
        if let Some(tab) = self.active_tab_mut() {
            tab.set_content(new_content);
        }
    }

    pub fn handle_new_tab(&mut self) {
        let new_id = format!("tab-{}", self.tabs.len() + 1);
        let title = format!("Untitled-{}.md", self.tabs.len() + 1);
        let tab = DocumentTab::new(new_id.clone(), title, String::new(), None, None);
        self.tabs.push(tab);
        self.active_tab_id = new_id;
    }

    pub fn handle_close_tab(&mut self, id: String) {
        if self.tabs.len() <= 1 {
            if let Some(tab) = self.tabs.first_mut() {
                tab.set_content(String::new());
                tab.is_modified = false;
            }
            return;
        }

        self.tabs.retain(|t| t.id != id);
        if self.active_tab_id == id {
            if let Some(last) = self.tabs.last() {
                self.active_tab_id = last.id.clone();
            }
        }
    }

    pub fn handle_save(&mut self) {
        let folder_path = self.workspace.folder_path.clone();
        let mut save_res = None;

        if let Some(tab) = self.active_tab_mut() {
            let target_path = if let Some(ref path) = tab.file_path {
                path.clone()
            } else {
                let clean_title = if tab.title.contains('.') {
                    tab.title.clone()
                } else {
                    format!("{}.md", tab.title)
                };
                let path = folder_path.join(clean_title);
                tab.file_path = Some(path.clone());
                path
            };

            if let Ok(()) = atomic_save(&target_path, &tab.content) {
                tab.is_modified = false;
                save_res = Some(target_path);
            }
        }

        if let Some(target_path) = save_res {
            self.saved_toast = Some(format!("Saved {}", target_path.file_name().unwrap_or_default().to_string_lossy()));
            self.workspace = scan_workspace_dir(&folder_path);
        }
    }

    pub fn handle_open_file(&mut self) {
        if let Some(path) = pick_open_file() {
            let name = path.file_name().and_then(|f| f.to_str()).unwrap_or("Untitled.md").to_string();

            if name.to_lowercase().ends_with(".pdf") {
                if let Ok((extracted_md, pages)) = extract_text_from_pdf(&path) {
                    let new_id = format!("tab-{}", self.tabs.len() + 1);
                    let mut tab = DocumentTab::new(new_id.clone(), name, extracted_md, Some(path), None);
                    tab.file_type = FileType::Pdf;
                    tab.pdf_page_count = Some(pages);
                    self.tabs.push(tab);
                    self.active_tab_id = new_id;
                    self.show_pdf_modal = true;
                }
            } else if let Ok(content) = std::fs::read_to_string(&path) {
                let new_id = format!("tab-{}", self.tabs.len() + 1);
                let tab = DocumentTab::new(new_id.clone(), name, content, Some(path), None);
                self.tabs.push(tab);
                self.active_tab_id = new_id;
            }
        }
    }

    pub fn handle_export(&mut self, format: &'static str) {
        if let Some(tab) = self.active_tab() {
            let default_name = format!("{}.{}", tab.title.trim_end_matches(".md"), format);
            if let Some(dest) = pick_save_file(&default_name, format) {
                match format {
                    "html" => {
                        let html = export_to_html(&tab.content, &tab.title);
                        let _ = std::fs::write(&dest, html);
                    }
                    _ => {
                        let txt = export_to_plaintext(&tab.content);
                        let _ = std::fs::write(&dest, txt);
                    }
                }
                self.saved_toast = Some(format!("Exported to {}", dest.file_name().unwrap_or_default().to_string_lossy()));
            }
        }
    }

    pub fn handle_format_cmd(&mut self, cmd: &'static str) {
        if let Some(tab) = self.active_tab_mut() {
            match cmd {
                "undo" => { tab.undo(); }
                "redo" => { tab.redo(); }
                "bold" => {
                    let new_c = format!("{} **bold text**", tab.content);
                    tab.set_content(new_c);
                }
                "italic" => {
                    let new_c = format!("{} *italic text*", tab.content);
                    tab.set_content(new_c);
                }
                "underline" => {
                    let new_c = format!("{} <u>underlined</u>", tab.content);
                    tab.set_content(new_c);
                }
                "strike" => {
                    let new_c = format!("{} ~~strikethrough~~", tab.content);
                    tab.set_content(new_c);
                }
                "h1" => {
                    let new_c = format!("{}\n\n# Heading 1\n", tab.content);
                    tab.set_content(new_c);
                }
                "h2" => {
                    let new_c = format!("{}\n\n## Heading 2\n", tab.content);
                    tab.set_content(new_c);
                }
                "h3" => {
                    let new_c = format!("{}\n\n### Heading 3\n", tab.content);
                    tab.set_content(new_c);
                }
                "ul" => {
                    let new_c = format!("{}\n- Item 1\n- Item 2\n", tab.content);
                    tab.set_content(new_c);
                }
                "ol" => {
                    let new_c = format!("{}\n1. First\n2. Second\n", tab.content);
                    tab.set_content(new_c);
                }
                "checklist" => {
                    let new_c = format!("{}\n- [ ] Task to complete\n- [x] Completed task\n", tab.content);
                    tab.set_content(new_c);
                }
                "quote" => {
                    let new_c = format!("{}\n> Quoted reference\n", tab.content);
                    tab.set_content(new_c);
                }
                "code" => {
                    let new_c = format!("{} `code`", tab.content);
                    tab.set_content(new_c);
                }
                "codeblock" => {
                    let new_c = format!("{}\n```rust\nfn main() {{\n    println!(\"Hello, LiteMark!\");\n}}\n```\n", tab.content);
                    tab.set_content(new_c);
                }
                "hr" => {
                    let new_c = format!("{}\n\n---\n\n", tab.content);
                    tab.set_content(new_c);
                }
                "clear" => {
                    let cleaned = tab.content
                        .replace("**", "")
                        .replace('*', "")
                        .replace("<u>", "")
                        .replace("</u>", "")
                        .replace("~~", "")
                        .replace('`', "");
                    tab.set_content(cleaned);
                }
                _ => {}
            }
        }
    }

    pub fn handle_insert_table(&mut self) {
        if let Some(tab) = self.active_tab_mut() {
            let table = "\n| Feature | Status | Details |\n| :--- | :--- | :--- |\n| Speed | Ultra Fast | Native Vulkan Render |\n| Memory | ~20 MB | Zero Chromium Overhead |\n\n";
            let new_c = format!("{}{}", tab.content, table);
            tab.set_content(new_c);
        }
    }

    pub fn handle_insert_link(&mut self) {
        if let Some(tab) = self.active_tab_mut() {
            let new_c = format!("{} [LiteMark Website](https://github.com/)", tab.content);
            tab.set_content(new_c);
        }
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

    pub fn handle_zoom_in(&mut self) {
        self.settings.zoom_level = (self.settings.zoom_level + 10).min(200);
        self.settings.save();
        self.show_zoom_hud = true;
    }

    pub fn handle_zoom_out(&mut self) {
        self.settings.zoom_level = self.settings.zoom_level.saturating_sub(10).max(50);
        self.settings.save();
        self.show_zoom_hud = true;
    }

    pub fn handle_reset_zoom(&mut self) {
        self.settings.zoom_level = 100;
        self.settings.save();
        self.show_zoom_hud = true;
    }

    pub fn handle_toggle_mode(&mut self) {
        self.settings.view_mode = self.settings.view_mode.toggle();
        self.settings.save();
    }

    pub fn handle_save_as(&mut self) {
        let suggested_name = if let Some(tab) = self.active_tab() {
            if tab.title.contains('.') {
                tab.title.clone()
            } else {
                format!("{}.md", tab.title)
            }
        } else {
            "Untitled.md".to_string()
        };

        if let Some(path) = pick_save_file(&suggested_name, "md") {
            if let Some(tab) = self.active_tab_mut() {
                if let Ok(()) = atomic_save(&path, &tab.content) {
                    tab.file_path = Some(path.clone());
                    if let Some(name) = path.file_name().and_then(|f| f.to_str()) {
                        tab.title = name.to_string();
                    }
                    tab.is_modified = false;
                    self.saved_toast = Some(format!("Saved {}", tab.title));
                    self.workspace = scan_workspace_dir(&self.workspace.folder_path);
                }
            }
        }
    }

    pub fn handle_new_folder(&mut self) {
        let _ = safe_create_folder(&self.workspace.folder_path, "New Folder");
        self.workspace = scan_workspace_dir(&self.workspace.folder_path);
    }

    pub fn handle_toggle_word_wrap(&mut self) {
        self.settings.word_wrap = !self.settings.word_wrap;
        self.settings.save();
    }

    pub fn handle_toggle_line_numbers(&mut self) {
        self.settings.show_line_numbers = !self.settings.show_line_numbers;
        self.settings.save();
    }

    pub fn handle_set_view_mode(&mut self, mode: ViewMode) {
        self.settings.view_mode = mode;
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

    pub fn handle_cut(&mut self, _cx: &mut App) {
        // Selection not yet implemented in editor
    }

    pub fn handle_copy(&mut self, cx: &mut App) {
        if let Some(tab) = self.active_tab() {
            cx.write_to_clipboard(ClipboardItem::new_string(tab.content.clone()));
        }
    }

    pub fn handle_paste(&mut self, cx: &mut App) {
        if let Some(item) = cx.read_from_clipboard() {
            if let Some(text) = item.text() {
                if let Some(tab) = self.active_tab_mut() {
                    let mut new_c = tab.content.clone();
                    new_c.push_str(&text);
                    tab.set_content(new_c);
                }
            }
        }
    }

    pub fn handle_select_all(&mut self) {
        // Selection not yet implemented in editor
    }
}

impl Render for LiteMarkApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::resolve(self.settings.theme, window.appearance());
        if self.open_menu.is_some() {
            self.menu_focus_handle.focus(window);
        } else {
            self.focus_handle.focus(window);
        }

        let active_tab = self.active_tab().cloned().unwrap_or_else(|| {
            DocumentTab::new("tab-1".to_string(), "Welcome.md".to_string(), INITIAL_MARKDOWN.to_string(), None, None)
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
                    this.active_tab_id = id;
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
                    this.handle_save();
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
                    this.handle_export(fmt);
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
                    this.handle_zoom_in();
                    cx.notify();
                });
            }
        };

        let on_zoom_out = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.handle_zoom_out();
                    cx.notify();
                });
            }
        };

        let on_reset_zoom = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.handle_reset_zoom();
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
                    this.handle_save_as();
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

        let on_undo = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.handle_format_cmd("undo");
                    cx.notify();
                });
            }
        };

        let on_redo = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.handle_format_cmd("redo");
                    cx.notify();
                });
            }
        };

        let on_open_replace = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.show_find_replace = true;
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
            move |window: &mut Window, _: &mut App| {
                window.remove_window();
            }
        };

        // Callbacks for Formatting Toolbar
        let on_format = {
            let view = view.clone();
            move |cmd: &'static str, cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.handle_format_cmd(cmd);
                    cx.notify();
                });
            }
        };

        let on_insert_table = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.handle_insert_table();
                    cx.notify();
                });
            }
        };

        let on_insert_link = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.handle_insert_link();
                    cx.notify();
                });
            }
        };

        let on_toggle_find = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.show_find_replace = !this.show_find_replace;
                    cx.notify();
                });
            }
        };

        let on_noop = |_cx: &mut App| {};

        let on_find_close = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.show_find_replace = false;
                    cx.notify();
                });
            }
        };

        // Callbacks for Sidebar
        let on_select_file = {
            let view = view.clone();
            move |file: WorkspaceFile, cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    let existing = this.tabs.iter().find(|t| t.file_path.as_ref() == Some(&file.path));
                    if let Some(t) = existing {
                        this.active_tab_id = t.id.clone();
                    } else {
                        let new_id = format!("tab-{}", this.tabs.len() + 1);
                        let content = std::fs::read_to_string(&file.path).unwrap_or_default();
                        let tab = DocumentTab::new(new_id.clone(), file.name.clone(), content, Some(file.path.clone()), Some(file.id.clone()));
                        this.tabs.push(tab);
                        this.active_tab_id = new_id;
                    }
                    cx.notify();
                });
            }
        };

        let on_create_file = {
            let view = view.clone();
            move |_folder_id: Option<String>, name: String, cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    let new_file_path = this.workspace.folder_path.join(&name);
                    let _ = std::fs::write(&new_file_path, "");
                    this.workspace = scan_workspace_dir(&this.workspace.folder_path);
                    let new_id = format!("tab-{}", this.tabs.len() + 1);
                    let tab = DocumentTab::new(new_id.clone(), name, String::new(), Some(new_file_path), None);
                    this.tabs.push(tab);
                    this.active_tab_id = new_id;
                    cx.notify();
                });
            }
        };

        let on_create_folder = {
            let view = view.clone();
            move |_parent_id: Option<String>, name: String, cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    let new_dir_path = this.workspace.folder_path.join(&name);
                    let _ = std::fs::create_dir_all(&new_dir_path);
                    this.workspace = scan_workspace_dir(&this.workspace.folder_path);
                    cx.notify();
                });
            }
        };

        let on_delete_file = {
            let view = view.clone();
            move |id: String, cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    if let Some(f) = this.workspace.files.iter().find(|f| f.id == id).cloned() {
                        let _ = std::fs::remove_file(&f.path);
                        this.workspace = scan_workspace_dir(&this.workspace.folder_path);
                        this.tabs.retain(|t| t.file_path.as_ref() != Some(&f.path));
                        if this.tabs.is_empty() {
                            this.handle_new_tab();
                        } else if !this.tabs.iter().any(|t| t.id == this.active_tab_id) {
                            this.active_tab_id = this.tabs[0].id.clone();
                        }
                    }
                    cx.notify();
                });
            }
        };

        let on_delete_folder = {
            let view = view.clone();
            move |id: String, cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    if let Some(f) = this.workspace.folders.iter().find(|f| f.id == id).cloned() {
                        let _ = std::fs::remove_dir_all(&f.path);
                        this.workspace = scan_workspace_dir(&this.workspace.folder_path);
                    }
                    cx.notify();
                });
            }
        };

        let on_choose_dir = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    if let Some(dir) = pick_directory() {
                        this.workspace = scan_workspace_dir(&dir);
                        cx.notify();
                    }
                });
            }
        };

        // Editor callbacks
        let on_content_change = {
            let view = view.clone();
            move |c: String, cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.handle_content_change(c);
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

        let is_narrow = window.viewport_size().width < px(640.0);

        let menu_bar_element = render_menu_bar(
            MenuBarProps {
                open_menu: self.open_menu,
                is_narrow,
                on_open_menu: on_open_menu.clone(),
                on_close_menu: on_close_menu.clone(),
            },
            &theme,
        ).into_any_element();

        let settings_gear_element = render_settings_button(
            self.open_menu == Some(MenuId::Settings),
            on_toggle_settings.clone(),
            &theme,
        ).into_any_element();

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
                        on_set_view_mode.clone(),
                        on_toggle_line_numbers.clone(),
                        on_toggle_word_wrap.clone(),
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
                        on_view_pdf.clone(),
                        on_close_active_tab.clone(),
                    );
                    let edit_items = build_edit_menu_items(
                        on_undo.clone(),
                        on_redo.clone(),
                        on_toggle_find.clone(),
                        on_open_replace.clone(),
                    );
                    let view_items = build_view_menu_items(
                        self.settings.view_mode,
                        self.settings.show_line_numbers,
                        self.settings.word_wrap,
                        self.show_sidebar,
                        on_set_view_mode.clone(),
                        on_toggle_line_numbers.clone(),
                        on_toggle_word_wrap.clone(),
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
                MenuId::Overflow => (
                    Point::new((window.viewport_size().width / 2.0 + px(220.0)).min(window.viewport_size().width - px(8.0)), px(76.0)),
                    Corner::TopRight,
                    px(210.0),
                    build_overflow_menu_items(
                        on_format.clone(),
                        on_insert_table.clone(),
                        on_close_menu.clone(),
                    ),
                ),
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
            .key_context("LiteMark")
            .flex()
            .flex_col()
            .size_full()
            .bg(theme.bg_editor)
            .text_color(theme.text_primary)
            .font_family(Theme::FONT_PROSE)
            // Keyboard shortcut bindings
            .on_action(cx.listener(|this, _: &ToggleBold, _window, cx| { this.handle_format_cmd("bold"); cx.notify(); }))
            .on_action(cx.listener(|this, _: &ToggleItalic, _window, cx| { this.handle_format_cmd("italic"); cx.notify(); }))
            .on_action(cx.listener(|this, _: &ToggleUnderline, _window, cx| { this.handle_format_cmd("underline"); cx.notify(); }))
            .on_action(cx.listener(|this, _: &ToggleStrikethrough, _window, cx| { this.handle_format_cmd("strike"); cx.notify(); }))
            .on_action(cx.listener(|this, _: &Heading1, _window, cx| { this.handle_format_cmd("h1"); cx.notify(); }))
            .on_action(cx.listener(|this, _: &Heading2, _window, cx| { this.handle_format_cmd("h2"); cx.notify(); }))
            .on_action(cx.listener(|this, _: &Heading3, _window, cx| { this.handle_format_cmd("h3"); cx.notify(); }))
            .on_action(cx.listener(|this, _: &ToggleBulletList, _window, cx| { this.handle_format_cmd("ul"); cx.notify(); }))
            .on_action(cx.listener(|this, _: &ToggleNumberedList, _window, cx| { this.handle_format_cmd("ol"); cx.notify(); }))
            .on_action(cx.listener(|this, _: &ToggleChecklist, _window, cx| { this.handle_format_cmd("checklist"); cx.notify(); }))
            .on_action(cx.listener(|this, _: &ToggleQuote, _window, cx| { this.handle_format_cmd("quote"); cx.notify(); }))
            .on_action(cx.listener(|this, _: &ToggleInlineCode, _window, cx| { this.handle_format_cmd("code"); cx.notify(); }))
            .on_action(cx.listener(|this, _: &InsertCodeBlock, _window, cx| { this.handle_format_cmd("codeblock"); cx.notify(); }))
            .on_action(cx.listener(|this, _: &InsertDivider, _window, cx| { this.handle_format_cmd("hr"); cx.notify(); }))
            .on_action(cx.listener(|this, _: &InsertTable, _window, cx| { this.handle_insert_table(); cx.notify(); }))
            .on_action(cx.listener(|this, _: &InsertLink, _window, cx| { this.handle_insert_link(); cx.notify(); }))
            .on_action(cx.listener(|this, _: &ClearFormatting, _window, cx| { this.handle_format_cmd("clear"); cx.notify(); }))
            .on_action(cx.listener(|this, _: &SaveFile, _window, cx| { this.handle_save(); cx.notify(); }))
            .on_action(cx.listener(|this, _: &OpenFile, _window, cx| { this.handle_open_file(); cx.notify(); }))
            .on_action(cx.listener(|this, _: &NewTab, _window, cx| { this.handle_new_tab(); cx.notify(); }))
            .on_action(cx.listener(|this, _: &CloseActiveTab, _window, cx| {
                let id = this.active_tab_id.clone();
                this.handle_close_tab(id);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &FindReplace, _window, cx| { this.show_find_replace = !this.show_find_replace; cx.notify(); }))
            .on_action(cx.listener(|this, _: &ToggleSidebar, _window, cx| { this.handle_toggle_sidebar(); cx.notify(); }))
            .on_action(cx.listener(|this, _: &ToggleZenMode, _window, cx| { this.handle_toggle_zen(); cx.notify(); }))
            .on_action(cx.listener(|this, _: &ExitZenMode, _window, cx| { this.handle_exit_zen(); cx.notify(); }))
            .on_action(cx.listener(|this, _: &ZoomIn, _window, cx| { this.handle_zoom_in(); cx.notify(); }))
            .on_action(cx.listener(|this, _: &ZoomOut, _window, cx| { this.handle_zoom_out(); cx.notify(); }))
            .on_action(cx.listener(|this, _: &ResetZoom, _window, cx| { this.handle_reset_zoom(); cx.notify(); }))
            .on_action(cx.listener(|this, _: &ToggleViewMode, _window, cx| { this.handle_toggle_mode(); cx.notify(); }))
            .on_action(cx.listener(|this, _: &UndoAction, _window, cx| { this.handle_format_cmd("undo"); cx.notify(); }))
            .on_action(cx.listener(|this, _: &RedoAction, _window, cx| { this.handle_format_cmd("redo"); cx.notify(); }))
            .on_action(cx.listener(|this, _: &SaveFileAs, _window, cx| { this.handle_save_as(); cx.notify(); }))
            .on_action(cx.listener(|this, _: &OpenReplace, _window, cx| { this.show_find_replace = true; cx.notify(); }))
            .on_action(cx.listener(|this, _: &ToggleLineNumbers, _window, cx| { this.handle_toggle_line_numbers(); cx.notify(); }))
            .on_action(cx.listener(|this, _: &Cut, _window, cx| { this.handle_cut(cx); cx.notify(); }))
            .on_action(cx.listener(|this, _: &Copy, _window, cx| { this.handle_copy(cx); cx.notify(); }))
            .on_action(cx.listener(|this, _: &Paste, _window, cx| { this.handle_paste(cx); cx.notify(); }))
            .on_action(cx.listener(|this, _: &SelectAll, _window, cx| { this.handle_select_all(); cx.notify(); }))
            .on_action(cx.listener(|this, _: &OpenShortcuts, _window, cx| { this.show_shortcuts_modal = true; cx.notify(); }))
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
            .on_action(cx.listener(|this, _: &CloseModal, _window, cx| {
                if this.open_menu.is_some() {
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
                                search_term: self.find_term.clone(),
                                replace_term: self.replace_term.clone(),
                                match_case: self.match_case,
                                not_found: self.not_found,
                                on_find_next: on_noop,
                                on_find_prev: on_noop,
                                on_replace: on_noop,
                                on_replace_all: on_noop,
                                on_close: on_find_close.clone(),
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
                                        open_file_ids: self.tabs.iter().filter_map(|t| t.workspace_file_id.clone()).collect(),
                                        search_query: self.search_query.clone(),
                                        creating_in_folder: None,
                                        on_select_file: on_select_file.clone(),
                                        on_create_file: on_create_file.clone(),
                                        on_create_folder: on_create_folder.clone(),
                                        on_delete_file: on_delete_file.clone(),
                                        on_delete_folder: on_delete_folder.clone(),
                                        on_choose_dir: on_choose_dir.clone(),
                                        on_close: on_toggle_sidebar.clone(),
                                    },
                                    &theme,
                                ))
                            })
                            .child(render_editor(
                                EditorProps {
                                    content: active_tab.content.clone(),
                                    view_mode: self.settings.view_mode,
                                    zoom_level: self.settings.zoom_level,
                                    base_font_size: self.settings.base_font_size,
                                    show_line_numbers: self.settings.show_line_numbers,
                                    word_wrap: self.settings.word_wrap,
                                    show_zoom_hud: self.show_zoom_hud,
                                    on_content_change: on_content_change.clone(),
                                },
                                &theme,
                            ))
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
                            content: active_tab.content.clone(),
                            view_mode: self.settings.view_mode,
                            zoom_level: self.settings.zoom_level,
                            base_font_size: self.settings.base_font_size,
                            show_line_numbers: false,
                            word_wrap: self.settings.word_wrap,
                            show_zoom_hud: self.show_zoom_hud,
                            on_content_change: on_content_change.clone(),
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
                                .bg(theme.text_faint)
                        )
                        .child(toast)
                )
            })
            // Modals
            .when(self.show_shortcuts_modal, |s| {
                s.child(render_shortcuts_modal(
                    on_close_shortcuts.clone(),
                    &theme,
                ))
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
                s.child(render_about_modal(
                    on_close_about.clone(),
                    &theme,
                ))
            })
            .when_some(menu_overlay, |s, overlay| {
                s.child(overlay)
            })
    }
}
