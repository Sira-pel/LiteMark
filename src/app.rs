use gpui::prelude::*;
use gpui::*;
use std::path::PathBuf;
use crate::actions::*;
use crate::markdown::{export_to_html, export_to_plaintext};
use crate::model::*;
use crate::services::*;
use crate::theme::Theme;
use crate::ui::*;

pub struct LiteMarkApp {
    pub tabs: Vec<DocumentTab>,
    pub active_tab_id: String,
    pub workspace: WorkspaceDirectory,
    pub settings: EditorSettings,
    pub show_sidebar: bool,
    pub show_find_replace: bool,
    pub show_shortcuts_modal: bool,
    pub show_pdf_modal: bool,
    pub show_file_menu: bool,
    pub show_overflow_menu: bool,
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
            show_file_menu: false,
            show_overflow_menu: false,
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
}

impl Render for LiteMarkApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::new(self.settings.theme);
        self.focus_handle.focus(window);

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

        let on_toggle_theme = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.handle_toggle_theme();
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

        let on_toggle_file_menu = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.show_file_menu = !this.show_file_menu;
                    this.show_overflow_menu = false;
                    cx.notify();
                });
            }
        };

        let on_toggle_overflow = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, cx| {
                    this.show_overflow_menu = !this.show_overflow_menu;
                    this.show_file_menu = false;
                    cx.notify();
                });
            }
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
            .on_action(cx.listener(|this, _: &CloseModal, _window, cx| {
                if this.show_file_menu {
                    this.show_file_menu = false;
                } else if this.show_overflow_menu {
                    this.show_overflow_menu = false;
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
                            has_pdf: active_tab_has_pdf,
                            is_maximized: is_max,
                            show_file_menu: self.show_file_menu,
                            on_select_tab: on_select_tab.clone(),
                            on_close_tab: on_close_tab.clone(),
                            on_new_tab: on_new_tab.clone(),
                            on_toggle_sidebar: on_toggle_sidebar.clone(),
                            on_save: on_save.clone(),
                            on_open: on_open.clone(),
                            on_export: on_export.clone(),
                            on_toggle_theme: on_toggle_theme.clone(),
                            on_toggle_zen: on_toggle_zen.clone(),
                            on_open_shortcuts: on_open_shortcuts.clone(),
                            on_view_pdf: on_view_pdf.clone(),
                            on_toggle_file_menu: on_toggle_file_menu.clone(),
                            on_minimize: on_minimize.clone(),
                            on_maximize: on_maximize.clone(),
                            on_close_window: on_close_window.clone(),
                        },
                        &theme,
                    ))
                    // 2. Formatting Toolbar
                    .child(render_formatting_toolbar(
                        FormattingToolbarProps {
                            active_formats,
                            show_overflow_menu: self.show_overflow_menu,
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
                                on_find_next: on_noop.clone(),
                                on_find_prev: on_noop.clone(),
                                on_replace: on_noop.clone(),
                                on_replace_all: on_noop.clone(),
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
                                    show_line_numbers: self.settings.show_line_numbers,
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
                            show_line_numbers: false,
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
    }
}
