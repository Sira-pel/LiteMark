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
    pub zen_mode: bool,
    pub zen_notice: bool,
    pub saved_toast: Option<String>,
    pub show_zoom_hud: bool,
    pub search_query: String,
    pub find_term: String,
    pub replace_term: String,
    pub match_case: bool,
    pub not_found: bool,
}

impl LiteMarkApp {
    pub fn new(initial_file: Option<PathBuf>) -> Self {
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
            zen_mode: false,
            zen_notice: false,
            saved_toast: None,
            show_zoom_hud: false,
            search_query: String::new(),
            find_term: String::new(),
            replace_term: String::new(),
            match_case: false,
            not_found: false,
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
            let (content, ext, suggested) = match format {
                "html" => (export_to_html(&tab.title, &tab.content), "html", format!("{}.html", tab.title.trim_end_matches(".md"))),
                "txt" => (export_to_plaintext(&tab.content), "txt", format!("{}.txt", tab.title.trim_end_matches(".md"))),
                _ => (tab.content.clone(), "md", tab.title.clone()),
            };

            if let Some(target) = pick_save_file(&suggested, ext) {
                let _ = std::fs::write(&target, content);
                self.saved_toast = Some(format!("Exported to {}", target.display()));
            }
        }
    }

    pub fn handle_format_cmd(&mut self, cmd: &'static str) {
        if let Some(tab) = self.active_tab_mut() {
            match cmd {
                "undo" => { tab.undo(); },
                "redo" => { tab.redo(); },
                "bold" => {
                    let new_c = format!("{} **bold text**", tab.content);
                    tab.set_content(new_c);
                },
                "italic" => {
                    let new_c = format!("{} *italic text*", tab.content);
                    tab.set_content(new_c);
                },
                "underline" => {
                    let new_c = format!("{} <u>underlined</u>", tab.content);
                    tab.set_content(new_c);
                },
                "strike" => {
                    let new_c = format!("{} ~~strikethrough~~", tab.content);
                    tab.set_content(new_c);
                },
                "h1" => {
                    let new_c = format!("{}\n\n# Heading 1\n", tab.content);
                    tab.set_content(new_c);
                },
                "h2" => {
                    let new_c = format!("{}\n\n## Heading 2\n", tab.content);
                    tab.set_content(new_c);
                },
                "h3" => {
                    let new_c = format!("{}\n\n### Heading 3\n", tab.content);
                    tab.set_content(new_c);
                },
                "ul" => {
                    let new_c = format!("{}\n- Item 1\n- Item 2\n", tab.content);
                    tab.set_content(new_c);
                },
                "ol" => {
                    let new_c = format!("{}\n1. First\n2. Second\n", tab.content);
                    tab.set_content(new_c);
                },
                "checklist" => {
                    let new_c = format!("{}\n- [ ] Task to complete\n- [x] Completed task\n", tab.content);
                    tab.set_content(new_c);
                },
                "quote" => {
                    let new_c = format!("{}\n> Quoted reference\n", tab.content);
                    tab.set_content(new_c);
                },
                "code" => {
                    let new_c = format!("{} `code`", tab.content);
                    tab.set_content(new_c);
                },
                "codeblock" => {
                    let new_c = format!("{}\n```rust\nfn main() {{\n    println!(\"Hello, LiteMark!\");\n}}\n```\n", tab.content);
                    tab.set_content(new_c);
                },
                "hr" => {
                    let new_c = format!("{}\n\n---\n\n", tab.content);
                    tab.set_content(new_c);
                },
                "clear" => {
                    // strip common markdown
                    let cleaned = tab.content
                        .replace("**", "")
                        .replace('*', "")
                        .replace("<u>", "")
                        .replace("</u>", "")
                        .replace("~~", "")
                        .replace('`', "");
                    tab.set_content(cleaned);
                },
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
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::new(self.settings.theme);
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

        let view = cx.entity().downgrade();

        // Callbacks for HeaderBar
        let on_select_tab = {
            let view = view.clone();
            move |id: String, cx: &mut App| {
                let _ = view.update(cx, |this, _cx| {
                    this.active_tab_id = id;
                });
            }
        };

        let on_close_tab = {
            let view = view.clone();
            move |id: String, cx: &mut App| {
                let _ = view.update(cx, |this, _cx| {
                    this.handle_close_tab(id);
                });
            }
        };

        let on_new_tab = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, _cx| {
                    this.handle_new_tab();
                });
            }
        };

        let on_toggle_sidebar = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, _cx| {
                    this.handle_toggle_sidebar();
                });
            }
        };

        let on_save = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, _cx| {
                    this.handle_save();
                });
            }
        };

        let on_open = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, _cx| {
                    this.handle_open_file();
                });
            }
        };

        let on_export = {
            let view = view.clone();
            move |fmt: &'static str, cx: &mut App| {
                let _ = view.update(cx, |this, _cx| {
                    this.handle_export(fmt);
                });
            }
        };

        let on_toggle_theme = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, _cx| {
                    this.handle_toggle_theme();
                });
            }
        };

        let on_toggle_zen = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, _cx| {
                    this.handle_toggle_zen();
                });
            }
        };

        let on_open_shortcuts = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, _cx| {
                    this.show_shortcuts_modal = true;
                });
            }
        };

        let on_zoom_in = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, _cx| {
                    this.handle_zoom_in();
                });
            }
        };

        let on_zoom_out = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, _cx| {
                    this.handle_zoom_out();
                });
            }
        };

        let on_reset_zoom = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, _cx| {
                    this.handle_reset_zoom();
                });
            }
        };

        let on_view_pdf = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, _cx| {
                    this.show_pdf_modal = true;
                });
            }
        };

        // Formatting toolbar callbacks
        let on_format = {
            let view = view.clone();
            move |cmd: &'static str, cx: &mut App| {
                let _ = view.update(cx, |this, _cx| {
                    this.handle_format_cmd(cmd);
                });
            }
        };

        let on_insert_table = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, _cx| {
                    this.handle_insert_table();
                });
            }
        };

        let on_insert_link = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, _cx| {
                    this.handle_insert_link();
                });
            }
        };

        let on_toggle_find = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, _cx| {
                    this.show_find_replace = !this.show_find_replace;
                });
            }
        };

        // Find replace callbacks
        let on_find_close = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, _cx| {
                    this.show_find_replace = false;
                });
            }
        };

        let on_noop = move |_cx: &mut App| {};

        // Sidebar callbacks
        let on_select_file = {
            let view = view.clone();
            move |file: WorkspaceFile, cx: &mut App| {
                let _ = view.update(cx, |this, _cx| {
                    if let Ok(c) = std::fs::read_to_string(&file.path) {
                        let new_id = format!("tab-{}", this.tabs.len() + 1);
                        let tab = DocumentTab::new(new_id.clone(), file.name, c, Some(file.path), Some(file.id));
                        this.tabs.push(tab);
                        this.active_tab_id = new_id;
                    }
                });
            }
        };

        let on_create_file = {
            let view = view.clone();
            move |folder_id: Option<String>, name: String, cx: &mut App| {
                let _ = view.update(cx, |this, _cx| {
                    let parent = if let Some(fid) = folder_id {
                        this.workspace.folders.iter().find(|f| f.id == fid).map(|f| f.path.clone()).unwrap_or_else(|| this.workspace.folder_path.clone())
                    } else {
                        this.workspace.folder_path.clone()
                    };
                    if let Ok(p) = safe_create_file(&parent, &name, "# New Note\n\n") {
                        this.workspace = scan_workspace_dir(&this.workspace.folder_path);
                        let new_id = format!("tab-{}", this.tabs.len() + 1);
                        let tab = DocumentTab::new(new_id.clone(), name, "# New Note\n\n".to_string(), Some(p.clone()), Some(format!("file-{}", p.display())));
                        this.tabs.push(tab);
                        this.active_tab_id = new_id;
                    }
                });
            }
        };

        let on_create_folder = {
            let view = view.clone();
            move |parent_id: Option<String>, name: String, cx: &mut App| {
                let _ = view.update(cx, |this, _cx| {
                    let parent = if let Some(pid) = parent_id {
                        this.workspace.folders.iter().find(|f| f.id == pid).map(|f| f.path.clone()).unwrap_or_else(|| this.workspace.folder_path.clone())
                    } else {
                        this.workspace.folder_path.clone()
                    };
                    let _ = safe_create_folder(&parent, &name);
                    this.workspace = scan_workspace_dir(&this.workspace.folder_path);
                });
            }
        };

        let on_delete_file = {
            let view = view.clone();
            move |fid: String, cx: &mut App| {
                let _ = view.update(cx, |this, _cx| {
                    if let Some(f) = this.workspace.files.iter().find(|f| f.id == fid).cloned() {
                        let _ = safe_delete(&f.path, false);
                        this.workspace = scan_workspace_dir(&this.workspace.folder_path);
                    }
                });
            }
        };

        let on_delete_folder = {
            let view = view.clone();
            move |fld_id: String, cx: &mut App| {
                let _ = view.update(cx, |this, _cx| {
                    if let Some(fld) = this.workspace.folders.iter().find(|f| f.id == fld_id).cloned() {
                        let _ = safe_delete(&fld.path, true);
                        this.workspace = scan_workspace_dir(&this.workspace.folder_path);
                    }
                });
            }
        };

        let on_choose_dir = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, _cx| {
                    if let Some(dir) = pick_directory() {
                        this.workspace = scan_workspace_dir(&dir);
                    }
                });
            }
        };

        // Editor callbacks
        let on_content_change = {
            let view = view.clone();
            move |c: String, cx: &mut App| {
                let _ = view.update(cx, |this, _cx| {
                    this.handle_content_change(c);
                });
            }
        };

        // Status bar callbacks
        let on_toggle_mode = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, _cx| {
                    this.handle_toggle_mode();
                });
            }
        };

        // Modal callbacks
        let on_close_shortcuts = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, _cx| {
                    this.show_shortcuts_modal = false;
                });
            }
        };

        let on_close_pdf = {
            let view = view.clone();
            move |cx: &mut App| {
                let _ = view.update(cx, |this, _cx| {
                    this.show_pdf_modal = false;
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
                let _ = view.update(cx, |this, _cx| {
                    this.handle_exit_zen();
                });
            }
        };

        div()
            .id("litemark-root")
            .flex()
            .flex_col()
            .size_full()
            .bg(theme.bg_editor)
            .text_color(theme.text_primary)
            .font_family(Theme::FONT_PROSE)
            // Keyboard shortcut bindings
            .on_action(cx.listener(|this, _: &ToggleBold, _window, _cx| this.handle_format_cmd("bold")))
            .on_action(cx.listener(|this, _: &ToggleItalic, _window, _cx| this.handle_format_cmd("italic")))
            .on_action(cx.listener(|this, _: &ToggleUnderline, _window, _cx| this.handle_format_cmd("underline")))
            .on_action(cx.listener(|this, _: &ToggleStrikethrough, _window, _cx| this.handle_format_cmd("strike")))
            .on_action(cx.listener(|this, _: &SaveFile, _window, _cx| this.handle_save()))
            .on_action(cx.listener(|this, _: &OpenFile, _window, _cx| this.handle_open_file()))
            .on_action(cx.listener(|this, _: &NewTab, _window, _cx| this.handle_new_tab()))
            .on_action(cx.listener(|this, _: &CloseActiveTab, _window, _cx| {
                let id = this.active_tab_id.clone();
                this.handle_close_tab(id);
            }))
            .on_action(cx.listener(|this, _: &FindReplace, _window, _cx| this.show_find_replace = !this.show_find_replace))
            .on_action(cx.listener(|this, _: &ToggleSidebar, _window, _cx| this.handle_toggle_sidebar()))
            .on_action(cx.listener(|this, _: &ToggleZenMode, _window, _cx| this.handle_toggle_zen()))
            .on_action(cx.listener(|this, _: &ExitZenMode, _window, _cx| this.handle_exit_zen()))
            .on_action(cx.listener(|this, _: &ZoomIn, _window, _cx| this.handle_zoom_in()))
            .on_action(cx.listener(|this, _: &ZoomOut, _window, _cx| this.handle_zoom_out()))
            .on_action(cx.listener(|this, _: &ResetZoom, _window, _cx| this.handle_reset_zoom()))
            .on_action(cx.listener(|this, _: &ToggleViewMode, _window, _cx| this.handle_toggle_mode()))
            .on_action(cx.listener(|this, _: &UndoAction, _window, _cx| this.handle_format_cmd("undo")))
            .on_action(cx.listener(|this, _: &RedoAction, _window, _cx| this.handle_format_cmd("redo")))
            .on_action(cx.listener(|this, _: &CloseModal, _window, _cx| {
                this.show_shortcuts_modal = false;
                this.show_pdf_modal = false;
                this.show_find_replace = false;
                if this.zen_mode {
                    this.handle_exit_zen();
                }
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
                            zoom_level: self.settings.zoom_level,
                            has_pdf: active_tab_has_pdf,
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
                            on_zoom_in: on_zoom_in.clone(),
                            on_zoom_out: on_zoom_out.clone(),
                            on_reset_zoom: on_reset_zoom.clone(),
                            on_view_pdf: on_view_pdf.clone(),
                        },
                        &theme,
                    ))
                    // 2. Formatting Toolbar
                    .child(render_formatting_toolbar(
                        FormattingToolbarProps {
                            active_formats,
                            on_format: on_format.clone(),
                            on_insert_table: on_insert_table.clone(),
                            on_insert_link: on_insert_link.clone(),
                            on_toggle_find: on_toggle_find.clone(),
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
                                .bg(theme.accent_emerald)
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
