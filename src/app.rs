use crate::actions::*;
use crate::editor_input::{
    dispatch_editor_input, swallow_table_drag_input, DispatchOutcome, EditorInput,
};
use crate::markdown::{empty_parsed_document, export_stem, render_export, TableColumnAlign};
use crate::model::clipboard::{clipboard_image, clipboard_item, paste_text};
use crate::model::table_ops::TableOp;
use crate::model::text_ops::{range_from_utf16, TAB_DISPLAY_COLUMNS};
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
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use std::time::Duration;

mod commands;
mod drag_drop;
mod editor_events;
mod file_ops;
mod find;
mod input_handler;
mod key_bindings;
mod menus;
mod note_assets;
mod outline;
mod quick_open;
mod render;
mod scroll;
mod sidebar;
mod sidebar_clipboard;
mod tabs;

pub(crate) use quick_open::QuickOpen;

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
    CloseTab {
        id: String,
    },
    CloseWindow,
    DeleteFile {
        id: String,
    },
    DeleteFolder {
        id: String,
    },
    TrashDirtyFile {
        id: String,
    },
    TrashDirtyFolder {
        id: String,
    },
    /// Several sidebar rows selected with Shift, trashed together.
    DeleteMany {
        targets: Vec<SidebarTarget>,
    },
    /// A new workspace folder was picked; ask whether to copy the current
    /// note into it before switching.
    SwitchWorkspace {
        new_dir: PathBuf,
    },
    /// Ctrl+S on a tab whose file another program changed (or deleted)
    /// since the tab read it.
    ExternalChange {
        id: String,
        deleted: bool,
    },
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

/// Headings of one tab revision: `((tab id, revision), headings)`.
type OutlineCache = ((String, u64), Arc<Vec<crate::model::outline::OutlineItem>>);

pub struct LiteMarkApp {
    pub tabs: Vec<DocumentTab>,
    pub active_tab_id: String,
    pub workspace: Tracked<WorkspaceDirectory>,
    /// The sidebar tree for a workspace generation and pending create row,
    /// so a render (each keystroke, hover or scroll) doesn't rebuild it.
    /// Theme colours the input fields were last given.
    field_colors: Option<[Rgba; 4]>,
    /// Bumped by every rescan; a background scan applies only if unchanged.
    scan_generation: u64,
    rescan_requested: bool,
    sidebar_tree: RefCell<Option<SidebarTreeCache>>,
    /// Search results for one `(workspace generation, query)`.
    #[allow(clippy::type_complexity)]
    sidebar_search: RefCell<Option<(u64, String, std::rc::Rc<Vec<SidebarTreeItem>>)>>,
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
    /// Notes and folders cut or copied with Ctrl+X / Ctrl+C in the sidebar,
    /// by path, for Ctrl+V to paste. Kept apart from the system clipboard,
    /// like the file trees of other editors.
    pub sidebar_clipboard: Vec<PathBuf>,
    /// The sidebar clipboard came from Ctrl+X: pasting moves instead of copies.
    pub sidebar_clipboard_cut: bool,
    /// True while the sidebar tree owns arrow-key navigation.
    pub sidebar_keyboard: bool,
    pub sidebar_focus_handle: FocusHandle,
    /// Scroll position of the sidebar's virtualized list.
    pub sidebar_scroll: UniformListScrollHandle,
    /// Set when the keyboard moves the sidebar selection; the next frame
    /// scrolls the selected row into view, from this side.
    pub sidebar_scroll_to_selected: Option<ScrollStrategy>,
    pub zen_mode: bool,
    pub zen_notice: bool,
    /// The pointer is over the outline strip or card.
    pub outline_hover: bool,
    /// The card was opened by clicking the strip; it stays until closed.
    pub outline_open: bool,
    pub outline_cache: Option<OutlineCache>,
    pub saved_toast: Option<String>,
    pub show_zoom_hud: bool,
    pub search_query: String,
    pub find_term: String,
    pub replace_term: String,
    pub match_case: bool,
    pub not_found: bool,
    pub focus_handle: FocusHandle,
    pub menu_focus_handle: FocusHandle,
    pub tab_ids: TabIds,
    pub error_message: Option<String>,
    /// The last settings save failed; the error was shown once already.
    settings_save_failed: bool,
    settings_writer: crate::model::settings::SettingsWriter,
    pub prompt: Option<Prompt>,
    pub prompt_focus_handle: FocusHandle,
    pub prompt_selected_ix: usize,
    pub save_as_tab: Option<String>,
    pub pending_after_save: Option<PendingAfterSave>,
    pub editor_paint: Rc<RefCell<EditorPaint>>,
    pub note_image_cache: Entity<crate::ui::note_images::NoteImageCache>,
    /// Tabs closed since the last frame (any path: tab strip, Ctrl+W,
    /// discard, workspace delete/move). The next render tells
    /// `note_image_cache` to release pictures no open tab references.
    pub(crate) closed_tab_ids: Vec<String>,
    /// Word count of the selection, keyed by tab id, revision and range, so
    /// a frame (caret blink, hover) doesn't rescan a large selection.
    selection_words: Option<(String, u64, Range<usize>, Option<usize>)>,
    pub editor_scroll: ScrollHandle,
    /// Tab, view mode, caret and length the editor last scrolled to follow, so
    /// only a real edit, mode switch, or caret move scrolls (not the wheel or a repaint).
    caret_followed: Option<(String, ViewMode, Range<usize>, usize)>,
    caret_follow_waits: u8,
    scroll_momentum: crate::ui::editor::ScrollMomentum,
    /// Clock for touchpad momentum. `None` is the real clock; tests set and
    /// advance it so the coast doesn't depend on how fast the machine runs.
    pub(crate) scroll_clock: Option<std::time::Instant>,
    pub find_field: Entity<LineField>,
    pub replace_field: Entity<LineField>,
    /// Shared with the editor element, so a frame borrows the hits
    /// instead of copying every match.
    pub find_hits: Rc<[Range<usize>]>,
    pub find_index: Option<usize>,
    /// `(tab_id, revision, term, match_case)` that `find_hits` was computed for.
    find_hits_key: Option<(String, u64, String, bool)>,
    pub applied_focus: Option<FocusSlot>,
    pub close_hook_installed: bool,
    pub allow_window_close: bool,
    /// Tabs whose Ctrl+S is writing on a background thread.
    saves_in_flight: std::collections::HashSet<String>,
    /// Tabs that got another Ctrl+S while saving: write again afterwards.
    resave_after: std::collections::HashSet<String>,
    /// The user asked to close while a save was still writing.
    close_when_saved: bool,
    pub selecting: bool,
    pub zoom_epoch: u64,
    pub toast_epoch: u64,
    /// An update check is running; further checks are ignored until it ends.
    update_check_running: bool,
    /// The PDF whose text is being extracted in the background, if any.
    pdf_opening: Option<PathBuf>,
    pub available_update: Option<ReleaseInfo>,
    pub pending_find_focus: bool,
    pub pending_rename_focus: bool,
    pub sidebar_menu: Option<SidebarMenu>,
    pub workspace_menu_open: bool,
    /// Recent folders that still exist, checked when the workspace menu
    /// opens rather than on every frame it is open.
    workspace_menu_recents: Vec<PathBuf>,
    /// The active tab's file last added to the recent notes, so it is added
    /// once per switch (and "Clear recent" isn't undone by the next frame).
    recent_seen: Option<PathBuf>,
    /// Recent notes that still exist, checked when the File menu opens
    /// rather than on every frame it is open.
    recent_menu_cache: Option<Vec<PathBuf>>,
    /// The open tabs or recent notes changed since settings.json was last
    /// written. They are written a moment later, or when the window closes,
    /// so switching tabs quickly doesn't write the file each time.
    tab_settings_dirty: bool,
    /// A delayed write of the tab settings is already scheduled.
    tab_settings_save_pending: bool,
    pub rename_target: Option<SidebarTarget>,
    pub rename_value: String,
    pub rename_field: Entity<LineField>,
    pub create_draft: Option<SidebarCreate>,
    pub create_field: Entity<LineField>,
    pub search_field: Entity<LineField>,
    /// The Ctrl+P pop-up, while it is open, and its text field.
    pub(crate) quick_open: Option<QuickOpen>,
    pub quick_open_field: Entity<LineField>,
    /// Number of the content search that may still report back.
    quick_open_epoch: Arc<AtomicU64>,
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
    /// A picture being resized with its corner handle.
    pub image_drag: Option<crate::ui::note_images::ImageDrag>,
    /// The mouse pointer is hidden after typing until the mouse moves.
    pub pointer_hidden: bool,
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
        // A file opened from the file manager is for a quick read: no sidebar,
        // whatever the saved preference (which stays untouched).
        let show_sidebar = settings.show_sidebar && initial_file.is_none();
        // Told once here; save_settings stays quiet about it afterwards.
        let settings_save_failed = settings.save_blocked.is_some();
        let settings_warning = settings.save_blocked.clone();
        // Reopen the folder the user last switched to; fall back to the
        // default notes folder when it is gone.
        let notes_dir = match settings.workspace_folder.clone().filter(|p| p.is_dir()) {
            Some(dir) => dir,
            None => {
                let dir = get_or_create_notes_dir();
                seed_welcome_note(&dir);
                dir
            }
        };
        let workspace = scan_workspace_dir(&notes_dir, settings.show_code_files);

        let mut tab_ids = TabIds::new();
        let first_id = tab_ids.alloc();
        let mut error_message = settings_warning;
        let mut opened_initial = false;
        let initial_tab = if let Some(path) = initial_file {
            let name = path
                .file_name()
                .and_then(|f| f.to_str())
                .unwrap_or("Untitled.md")
                .to_string();
            match read_note(&path) {
                Ok(content) => {
                    let mut tab = DocumentTab::new(first_id.clone(), name, content, None, None);
                    let version = disk_version(&path, &tab.file_text_ref());
                    tab.disk_version = version;
                    tab.file_path = Some(path);
                    opened_initial = true;
                    tab
                }
                Err(err) => {
                    error_message = Some(err);
                    Self::welcome_tab(&workspace, first_id.clone())
                }
            }
        } else {
            Self::welcome_tab(&workspace, first_id.clone())
        };
        if workspace.scan_truncated && error_message.is_none() {
            error_message = Some(scan_truncated_message());
        }

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

        let quick_open_field = cx.new({
            let app_handle = app_handle.clone();
            move |cx| {
                LineField::new(
                    cx,
                    "quick-open-field",
                    "Search notes",
                    Arc::new({
                        let app_handle = app_handle.clone();
                        move |text, app| {
                            let text = text.to_string();
                            app_handle
                                .update(app, |this, cx| this.quick_open_query_changed(text, cx))
                                .ok();
                        }
                    }),
                    Arc::new({
                        let app_handle = app_handle.clone();
                        move |_text, _previous, app| {
                            // Enter arrives while the field is being updated, and
                            // activating clears that same field. Wait until the
                            // update is over.
                            let app_handle = app_handle.clone();
                            app.defer(move |app| {
                                app_handle
                                    .update(app, |this, cx| this.quick_open_activate(None, cx))
                                    .ok();
                            });
                        }
                    }),
                    Arc::new({
                        let app_handle = app_handle.clone();
                        move |app| {
                            app_handle
                                .update(app, |this, cx| this.claim_field(TextInput::QuickOpen, cx))
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
            sidebar_clipboard: Vec::new(),
            sidebar_clipboard_cut: false,
            workspace: workspace.into(),
            field_colors: None,
            scan_generation: 0,
            rescan_requested: false,
            sidebar_tree: RefCell::new(None),
            sidebar_search: RefCell::new(None),
            settings,
            show_sidebar,
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
            sidebar_scroll: UniformListScrollHandle::new(),
            sidebar_scroll_to_selected: None,
            zen_mode: false,
            zen_notice: false,
            outline_hover: false,
            outline_open: false,
            outline_cache: None,
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
            tab_ids,
            error_message,
            settings_save_failed,
            settings_writer: crate::model::settings::SettingsWriter::spawn(),
            prompt: None,
            prompt_selected_ix: 0,
            save_as_tab: None,
            pending_after_save: None,
            editor_paint: Rc::new(RefCell::new(EditorPaint::default())),
            note_image_cache: crate::ui::note_images::NoteImageCache::new(cx),
            closed_tab_ids: Vec::new(),
            selection_words: None,
            editor_scroll: ScrollHandle::new(),
            caret_followed: None,
            caret_follow_waits: 0,
            scroll_momentum: Default::default(),
            scroll_clock: None,
            find_field,
            replace_field,
            find_hits: Vec::new().into(),
            find_index: None,
            find_hits_key: None,
            applied_focus: None,
            close_hook_installed: false,
            allow_window_close: false,
            saves_in_flight: Default::default(),
            resave_after: Default::default(),
            close_when_saved: false,
            selecting: false,
            zoom_epoch: 0,
            toast_epoch: 0,
            update_check_running: false,
            pdf_opening: None,
            available_update: None,
            pending_find_focus: false,
            pending_rename_focus: false,
            sidebar_menu: None,
            workspace_menu_open: false,
            workspace_menu_recents: Vec::new(),
            recent_seen: None,
            recent_menu_cache: None,
            tab_settings_dirty: false,
            tab_settings_save_pending: false,
            rename_target: None,
            rename_value: String::new(),
            rename_field,
            create_draft: None,
            create_field,
            search_field,
            quick_open: None,
            quick_open_field,
            quick_open_epoch: Arc::new(AtomicU64::new(0)),
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
            image_drag: None,
            pointer_hidden: false,
            table_drag_press_held: false,
            table_action_anchor: None,
            title_drag_armed: Rc::new(Cell::new(false)),
        };
        let reopened = app.restore_saved_tabs(opened_initial);
        if reopened > 0 {
            let notes = if reopened == 1 { "note" } else { "notes" };
            app.show_toast(format!("Reopened {reopened} {notes} from last time"), cx);
        }
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
            sidebar_clipboard: Vec::new(),
            sidebar_clipboard_cut: false,
            workspace: WorkspaceDirectory {
                folder_name: "test".into(),
                folder_path: PathBuf::from("/tmp/litemark-table-drag-test"),
                is_linked_to_local_disk: false,
                folders: Vec::new(),
                files: Vec::new(),
                scan_truncated: false,
            }
            .into(),
            field_colors: None,
            scan_generation: 0,
            rescan_requested: false,
            sidebar_tree: RefCell::new(None),
            sidebar_search: RefCell::new(None),
            // Never touch the real settings file from a test.
            settings: EditorSettings {
                check_for_updates: false,
                save_blocked: Some("test".into()),
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
            sidebar_scroll: UniformListScrollHandle::new(),
            sidebar_scroll_to_selected: None,
            zen_mode: false,
            zen_notice: false,
            outline_hover: false,
            outline_open: false,
            outline_cache: None,
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
            tab_ids,
            error_message: None,
            settings_save_failed: true,
            settings_writer: crate::model::settings::SettingsWriter::spawn(),
            prompt: None,
            prompt_selected_ix: 0,
            save_as_tab: None,
            pending_after_save: None,
            editor_paint: Rc::new(RefCell::new(EditorPaint::default())),
            note_image_cache: crate::ui::note_images::NoteImageCache::new(cx),
            closed_tab_ids: Vec::new(),
            selection_words: None,
            editor_scroll: ScrollHandle::new(),
            caret_followed: None,
            caret_follow_waits: 0,
            scroll_momentum: Default::default(),
            scroll_clock: None,
            find_field: Self::dummy_line_field(cx, "find-field"),
            replace_field: Self::dummy_line_field(cx, "replace-field"),
            find_hits: Vec::new().into(),
            find_index: None,
            find_hits_key: None,
            applied_focus: None,
            close_hook_installed: false,
            allow_window_close: false,
            saves_in_flight: Default::default(),
            resave_after: Default::default(),
            close_when_saved: false,
            selecting: false,
            zoom_epoch: 0,
            toast_epoch: 0,
            update_check_running: false,
            pdf_opening: None,
            available_update: None,
            pending_find_focus: false,
            pending_rename_focus: false,
            sidebar_menu: None,
            workspace_menu_open: false,
            workspace_menu_recents: Vec::new(),
            recent_seen: None,
            recent_menu_cache: None,
            tab_settings_dirty: false,
            tab_settings_save_pending: false,
            rename_target: None,
            rename_value: String::new(),
            rename_field: Self::dummy_line_field(cx, "rename-field"),
            create_draft: None,
            create_field: Self::dummy_line_field(cx, "create-field"),
            search_field: Self::dummy_line_field(cx, "search-field"),
            quick_open: None,
            quick_open_field: Self::dummy_line_field(cx, "quick-open-field"),
            quick_open_epoch: Arc::new(AtomicU64::new(0)),
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
            image_drag: None,
            pointer_hidden: false,
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
                let mut tab = DocumentTab::new(
                    id,
                    file.name.clone(),
                    content,
                    Some(file.path.clone()),
                    Some(file.id.clone()),
                );
                let version = disk_version(&file.path, &tab.file_text_ref());
                tab.disk_version = version;
                return tab;
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
        self.prompt.is_some() || self.quick_open.is_some()
    }

    fn text_field_focused(&self, window: &Window, cx: &App) -> bool {
        self.find_field.read(cx).is_focused(window)
            || self.replace_field.read(cx).is_focused(window)
            || self.rename_field.read(cx).is_focused(window)
            || self.create_field.read(cx).is_focused(window)
            || self.search_field.read(cx).is_focused(window)
            || self.quick_open_field.read(cx).is_focused(window)
    }

    fn show_error(&mut self, message: impl Into<String>) {
        self.error_message = Some(message.into());
    }

    /// Save settings, telling the user when it fails. A failure that keeps
    /// happening (full disk, read-only config folder) is shown only once.
    fn save_settings(&mut self) {
        // Every write includes the tab settings, so none are pending after it.
        self.tab_settings_dirty = false;
        // A blocked save fails at once without touching the disk.
        if let Some(reason) = self.settings.save_blocked.clone() {
            if !self.settings_save_failed {
                self.show_error(reason);
            }
            self.settings_save_failed = true;
            return;
        }
        self.report_settings_result();
        self.settings_writer.save(&self.settings);
    }

    /// Show the outcome of the last background settings write.
    pub(crate) fn report_settings_result(&mut self) {
        match self.settings_writer.take_result() {
            Some(Ok(())) => self.settings_save_failed = false,
            Some(Err(err)) => {
                if !self.settings_save_failed {
                    self.show_error(err);
                }
                self.settings_save_failed = true;
            }
            None => {}
        }
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
#[path = "outline_tests.rs"]
mod outline_tests;

#[cfg(test)]
#[path = "note_drop_tests.rs"]
mod note_drop_tests;

#[cfg(test)]
#[path = "note_image_tests.rs"]
mod note_image_tests;

#[cfg(test)]
#[path = "note_asset_tests.rs"]
mod note_asset_tests;

#[cfg(test)]
#[path = "disk_change_tests.rs"]
mod disk_change_tests;

#[cfg(test)]
#[path = "typing_tests.rs"]
mod typing_tests;

#[cfg(test)]
#[path = "find_panel_tests.rs"]
mod find_panel_tests;

impl crate::ui::text_editor::EditorContentSource for LiteMarkApp {
    fn editor_content(&self) -> &str {
        // With no active tab, render() paints a fallback Welcome tab built
        // from INITIAL_MARKDOWN; hand the editor that same text so its
        // zero-copy line slices aren't taken from an empty string.
        self.active_tab()
            .map(|tab| tab.content.as_str())
            .unwrap_or(INITIAL_MARKDOWN)
    }
}

#[cfg(test)]
#[path = "pdf_open_tests.rs"]
mod pdf_open_tests;

#[cfg(test)]
#[path = "async_save_tests.rs"]
mod async_save_tests;

#[cfg(test)]
#[path = "background_scan_tests.rs"]
mod background_scan_tests;

#[cfg(test)]
#[path = "sidebar_render_tests.rs"]
mod sidebar_render_tests;

#[cfg(test)]
#[path = "sidebar_clipboard_tests.rs"]
mod sidebar_clipboard_tests;

#[cfg(test)]
#[path = "tab_restore_tests.rs"]
mod tab_restore_tests;

#[cfg(test)]
#[path = "quick_open_tests.rs"]
mod quick_open_tests;
