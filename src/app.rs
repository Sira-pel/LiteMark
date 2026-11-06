use crate::actions::*;
use crate::editor_input::{
    dispatch_editor_input, swallow_table_drag_input, DispatchOutcome, EditorInput,
};
use crate::markdown::{export_stem, parse_document, render_export, TableColumnAlign};
use crate::model::table_ops::TableOp;
use crate::model::text_ops::{
    home_offset, line_end, offset_to_utf16, range_from_utf16, range_to_utf16, TAB_DISPLAY_COLUMNS,
};
use crate::model::clipboard::{clipboard_item, paste_text};
use crate::model::*;
use crate::services::*;
use crate::theme::{sz, Theme, ThemeMode};
use crate::ui::table_chrome::{
    action_anchor, clear_drag_for_key, clear_drag_for_modifier, commit_drag_release,
    drag_gap_if_inside, hit_test_occupied, hover_from_hit, next_align, occupied_content,
    retarget_hover, update_drag_gap, TableDrag, TableDragKind, TableHitKind, TableHot,
};
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

mod commands;
mod find;
mod scroll;
mod tabs;
mod sidebar;
mod drag_drop;
mod file_ops;
mod menus;
mod editor_events;
mod input_handler;
mod key_bindings;
mod render;


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
    /// Delete-key target). Falls back to the active tab's note when None.
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
    /// The active tab's text as a shared string, keyed by tab id, revision
    /// and length. Frames and IME queries between edits share this one copy
    /// instead of cloning the whole document each time.
    content_snapshot: Option<(String, u64, usize, SharedString)>,
    pub editor_scroll: ScrollHandle,
    /// Tab, caret and length the editor last scrolled to follow, so only a
    /// real edit or caret move scrolls (not the wheel or a repaint).
    caret_followed: Option<(String, Range<usize>, usize)>,
    caret_follow_waits: u8,
    scroll_momentum: crate::ui::editor::ScrollMomentum,
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
    /// An update check is running; further checks are ignored until it ends.
    update_check_running: bool,
    pub available_update: Option<ReleaseInfo>,
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
    pub activation_subscription: Option<Subscription>,
    pub table_picker_size: (usize, usize),
    pub editor_context_menu: Option<EditorContextMenu>,
    pub table_hover: Option<crate::ui::table_chrome::TableHover>,
    pub table_hot: Option<crate::ui::table_chrome::TableHot>,
    pub table_pressed: bool,
    pub table_drag: Option<crate::ui::table_chrome::TableDrag>,
    /// True while the mouse button that started a table drag is still held.
    /// Survives a modifier cancel of the visual drag so later chords cannot
    /// edit the note until that press ends (mouse-up anywhere, next mouse-down,
    /// a move with no button, a MouseExited with no button, focus loss,
    /// Escape, right-click, or a non-drag chrome/checkbox click). A leave
    /// that still has a button (X11, no grab) does not end the press.
    pub table_drag_press_held: bool,
    /// Byte the open table menu should edit. Empty means the caret.
    pub table_action_anchor: Option<usize>,
    title_drag_armed: Rc<Cell<bool>>,
}

impl LiteMarkApp {
    pub fn new(initial_file: Option<PathBuf>, cx: &mut Context<Self>) -> Self {
        let settings = EditorSettings::load();
        let notes_dir = get_or_create_notes_dir();
        seed_welcome_note(&notes_dir);
        let workspace = scan_workspace_dir(&notes_dir, settings.show_code_files);

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

        let mut app = Self {
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
            content_snapshot: None,
            editor_scroll: ScrollHandle::new(),
            caret_followed: None,
            caret_follow_waits: 0,
            scroll_momentum: Default::default(),
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
            update_check_running: false,
            available_update: None,
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
            activation_subscription: None,
            table_picker_size: (2, 2),
            editor_context_menu: None,
            table_hover: None,
            table_hot: None,
            table_pressed: false,
            table_drag: None,
            table_drag_press_held: false,
            table_action_anchor: None,
            title_drag_armed: Rc::new(Cell::new(false)),
        };
        app.start_update_check(cx);
        app
    }

    #[cfg(test)]
    fn dummy_line_field(cx: &mut Context<Self>, id: &'static str) -> Entity<LineField> {
        cx.new(|cx| {
            LineField::new(
                cx,
                id,
                "",
                Arc::new(|_, _| {}),
                Arc::new(|_, _, _| {}),
                Arc::new(|_| {}),
            )
        })
    }

    /// Headless app for drag-press tests. Does not touch the notes directory.
    #[cfg(test)]
    pub(crate) fn test_new(content: &str, cx: &mut Context<Self>) -> Self {
        let mut tab_ids = TabIds::new();
        let first_id = tab_ids.alloc();
        let tab = DocumentTab::new(
            first_id.clone(),
            "note.md".into(),
            content.to_string(),
            None,
            None,
        );
        Self {
            tabs: vec![tab],
            active_tab_id: first_id,
            selected_target: None,
            multi_selection: None,
            workspace: WorkspaceDirectory {
                folder_name: "test".into(),
                folder_path: PathBuf::from("/tmp/litemark-table-drag-test"),
                is_linked_to_local_disk: false,
                folders: Vec::new(),
                files: Vec::new(),
            },
            settings: EditorSettings {
                check_for_updates: false,
                ..EditorSettings::default()
            },
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
            error_message: None,
            prompt: None,
            prompt_selected_ix: 0,
            save_as_tab: None,
            pending_after_save: None,
            editor_paint: Rc::new(RefCell::new(EditorPaint::default())),
            content_snapshot: None,
            editor_scroll: ScrollHandle::new(),
            caret_followed: None,
            caret_follow_waits: 0,
            scroll_momentum: Default::default(),
            find_field: Self::dummy_line_field(cx, "find-field"),
            replace_field: Self::dummy_line_field(cx, "replace-field"),
            find_hits: Vec::new(),
            find_index: None,
            applied_focus: None,
            close_hook_installed: false,
            allow_window_close: false,
            selecting: false,
            zoom_epoch: 0,
            toast_epoch: 0,
            update_check_running: false,
            available_update: None,
            pending_find_focus: false,
            pending_rename_focus: false,
            sidebar_menu: None,
            workspace_menu_open: false,
            rename_target: None,
            rename_value: String::new(),
            rename_field: Self::dummy_line_field(cx, "rename-field"),
            create_draft: None,
            create_field: Self::dummy_line_field(cx, "create-field"),
            search_field: Self::dummy_line_field(cx, "search-field"),
            pending_create_focus: false,
            field_owns_keys: false,
            field_focus: TextInput::Editor,
            bounds_subscription: None,
            activation_subscription: None,
            table_picker_size: (2, 2),
            editor_context_menu: None,
            table_hover: None,
            table_hot: None,
            table_pressed: false,
            table_drag: None,
            table_drag_press_held: false,
            table_action_anchor: None,
            title_drag_armed: Rc::new(Cell::new(false)),
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

    /// For a failed write: save, export, create, rename, move, copy or
    /// trash. On Windows an access-denied error gets the Controlled Folder
    /// Access hint. Reads don't use this, since that feature only blocks
    /// writes.
    fn show_file_error(&mut self, message: impl Into<String>) {
        self.show_error(crate::services::fs_service::with_access_denied_hint(
            message.into(),
        ));
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

}

fn planned_title_fallback() -> String {
    "Saved".into()
}

#[cfg(test)]
#[path = "table_drag_handler_tests.rs"]
mod table_drag_handler_tests;

#[cfg(test)]
#[path = "viewport_culling_tests.rs"]
mod viewport_culling_tests;

#[cfg(test)]
#[path = "note_drop_tests.rs"]
mod note_drop_tests;

#[cfg(test)]
#[path = "typing_tests.rs"]
mod typing_tests;
