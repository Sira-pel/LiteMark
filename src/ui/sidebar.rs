use super::line_field::LineField;
use super::primitives::{icon_button, render_icon};
use crate::model::{WorkspaceDirectory, WorkspaceFile};
use crate::theme::{sz, Theme};
use gpui::prelude::*;
use gpui::*;
use crate::model::{sidebar_folder_search_matches, sidebar_search_matches};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SidebarTarget {
    File(String),
    Folder(String),
}

/// Drag payload for a row: the whole multi-selection when the row is part of
/// it, otherwise just the row.
fn drag_payload(
    target: SidebarTarget,
    name: String,
    in_multi: bool,
    multi_targets: &[SidebarTarget],
) -> SidebarDrag {
    if in_multi && multi_targets.len() > 1 {
        SidebarDrag {
            targets: multi_targets.to_vec(),
            name: format!("{} items", multi_targets.len()),
        }
    } else {
        SidebarDrag {
            targets: vec![target],
            name,
        }
    }
}

/// Ctrl on Linux and Windows, Cmd on macOS (Super+click is the window
/// manager's on many Linux desktops).
fn toggle_modifier(m: Modifiers) -> bool {
    if cfg!(target_os = "macos") { m.platform } else { m.control }
}

/// Thin vertical guide line that joins nested rows under their folder.
/// `left` is measured from the row's own left edge.
fn tree_guide(theme: &Theme, left: f32) -> Div {
    div()
        .absolute()
        .left(px(left))
        .top(px(-1.0))
        .bottom(px(-1.0))
        .w(px(1.0))
        .bg(theme.border_subtle)
}

/// Pending inline creation in the sidebar tree (`None` parent means workspace root).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SidebarCreate {
    pub parent: Option<String>,
    pub is_folder: bool,
}

/// Payload carried while a note or folder row is being dragged.
#[derive(Clone, Debug)]
pub struct SidebarDrag {
    /// The dragged row, or every row of the multi-selection it belongs to.
    pub targets: Vec<SidebarTarget>,
    pub name: String,
}

/// Floating chip rendered under the cursor while dragging.
pub struct DragItemGhost {
    theme: Theme,
    is_folder: bool,
    name: SharedString,
    /// Where the row was grabbed, relative to its origin. GPUI places the
    /// ghost at cursor minus this, so it's added back to keep the chip by the pointer.
    grab_offset: Point<Pixels>,
}

impl Render for DragItemGhost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        div()
            .pl(self.grab_offset.x + px(12.0))
            .pt(self.grab_offset.y + px(12.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .px(px(10.0))
                    .py(px(5.0))
                    .rounded(sz::R_MD)
                    .bg(theme.menu_bg)
                    .border_1()
                    .border_color(theme.border_surface)
                    .shadow_lg()
                    .opacity(0.9)
                    .child(render_icon(
                        if self.is_folder { "folder" } else { "file-text" },
                        13.0,
                        theme.text_muted,
                    ))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(theme.text_primary)
                            .truncate()
                            .child(self.name.clone()),
                    ),
            )
    }
}

#[derive(Clone, Debug)]
pub struct SidebarMenu {
    pub target: SidebarTarget,
    pub position: Point<Pixels>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SidebarTreeItem {
    Folder {
        folder: crate::model::WorkspaceFolder,
        depth: usize,
    },
    File {
        file: crate::model::WorkspaceFile,
        depth: usize,
    },
    /// Inline name editor shown while creating a note or folder.
    CreateInput {
        depth: usize,
        is_folder: bool,
    },
}

pub fn build_sidebar_tree(
    folders: &[crate::model::WorkspaceFolder],
    files: &[crate::model::WorkspaceFile],
    creating: Option<&SidebarCreate>,
) -> Vec<SidebarTreeItem> {
    let mut items = Vec::new();
    let folder_ids: std::collections::HashSet<&str> =
        folders.iter().map(|f| f.id.as_str()).collect();

    fn collect_level(
        parent_id: Option<&str>,
        depth: usize,
        folders: &[crate::model::WorkspaceFolder],
        files: &[crate::model::WorkspaceFile],
        folder_ids: &std::collections::HashSet<&str>,
        creating: Option<(Option<&str>, bool)>,
        out: &mut Vec<SidebarTreeItem>,
    ) {
        // The inline create editor sits at the top of the level it targets.
        if let Some((create_parent, is_folder)) = creating {
            if create_parent == parent_id {
                out.push(SidebarTreeItem::CreateInput { depth, is_folder });
            }
        }
        let mut level_folders: Vec<&crate::model::WorkspaceFolder> = folders
            .iter()
            .filter(|f| {
                if let Some(pid) = parent_id {
                    f.parent_id.as_deref() == Some(pid)
                } else {
                    f.parent_id.is_none()
                        || f.parent_id.as_deref().is_some_and(|p| !folder_ids.contains(p))
                }
            })
            .collect();
        level_folders.sort_by_cached_key(|a| a.name.to_lowercase());

        for folder in level_folders {
            let is_open = folder.is_open;
            let fid = folder.id.clone();
            out.push(SidebarTreeItem::Folder {
                folder: folder.clone(),
                depth,
            });
            if is_open {
                collect_level(Some(&fid), depth + 1, folders, files, folder_ids, creating, out);
            }
        }

        let mut level_files: Vec<&crate::model::WorkspaceFile> = files
            .iter()
            .filter(|f| {
                if let Some(pid) = parent_id {
                    f.folder_id.as_deref() == Some(pid)
                } else {
                    f.folder_id.is_none()
                        || f.folder_id.as_deref().is_some_and(|p| !folder_ids.contains(p))
                }
            })
            .collect();
        level_files.sort_by_cached_key(|a| a.name.to_lowercase());

        for file in level_files {
            out.push(SidebarTreeItem::File {
                file: file.clone(),
                depth,
            });
        }
    }

    collect_level(
        None,
        0,
        folders,
        files,
        &folder_ids,
        creating.map(|c| (c.parent.as_deref(), c.is_folder)),
        &mut items,
    );
    items
}

#[allow(dead_code)]
pub struct SidebarProps<
    FSelect,
    FBeginCreate,
    FDeleteFile,
    FDeleteFolder,
    FToggleFolder,
    FWorkspaceMenu,
    FClose,
    FContext,
    FDropItem,
    FRange,
    FToggle,
    FBlank,
> where
    FSelect: Fn(WorkspaceFile, &mut App) + 'static + Clone,
    FBeginCreate: Fn(Option<String>, bool, &mut App) + 'static + Clone,
    FDeleteFile: Fn(String, &mut App) + 'static + Clone,
    FDeleteFolder: Fn(String, &mut App) + 'static + Clone,
    FToggleFolder: Fn(String, &mut App) + 'static + Clone,
    FWorkspaceMenu: Fn(&mut App) + 'static + Clone,
    FClose: Fn(&mut App) + 'static + Clone,
    FContext: Fn(SidebarTarget, Point<Pixels>, &mut App) + 'static + Clone,
    FDropItem: Fn(Vec<SidebarTarget>, Option<String>, bool, &mut App) + 'static + Clone,
    FRange: Fn(SidebarTarget, &mut App) + 'static + Clone,
    FToggle: Fn(SidebarTarget, &mut App) + 'static + Clone,
    FBlank: Fn(&mut App) + 'static + Clone,
{
    pub workspace: WorkspaceDirectory,
    pub width: Pixels,
    pub selected_file_id: Option<String>,
    /// Note open in the editor; gets a quieter mark than the selection.
    pub open_file_id: Option<String>,
    pub selected_folder_id: Option<String>,
    pub focus_handle: FocusHandle,
    /// Workspace ids of tabs with unsaved changes (drives the dot).
    pub modified_file_ids: Vec<String>,
    pub search_query: String,
    pub creating: Option<SidebarCreate>,
    pub create_field: Entity<LineField>,
    pub search_field: Entity<LineField>,
    pub dragging: bool,
    pub on_select_file: FSelect,
    pub on_begin_create: FBeginCreate,
    pub on_delete_file: FDeleteFile,
    pub on_delete_folder: FDeleteFolder,
    pub on_toggle_folder: FToggleFolder,
    pub on_workspace_menu: FWorkspaceMenu,
    /// Drives the pressed look and chevron direction of the title button.
    pub workspace_menu_open: bool,
    pub on_close: FClose,
    pub on_context_menu: FContext,
    pub on_drop_item: FDropItem,
    /// Shift+click: extend the selection from the anchor to this row.
    pub on_range_select: FRange,
    /// Ctrl/Cmd+click: add or remove one row without touching the rest.
    pub on_toggle_select: FToggle,
    /// Click on empty space in the list: drops the selection.
    pub on_blank_click: FBlank,
    /// Ids of every row in a multi-selection (empty for a single selection).
    pub multi_targets: Vec<SidebarTarget>,
}

pub fn render_sidebar<
    FSelect,
    FBeginCreate,
    FDeleteFile,
    FDeleteFolder,
    FToggleFolder,
    FWorkspaceMenu,
    FClose,
    FContext,
    FDropItem,
    FRange,
    FToggle,
    FBlank,
>(
    props: SidebarProps<
        FSelect,
        FBeginCreate,
        FDeleteFile,
        FDeleteFolder,
        FToggleFolder,
        FWorkspaceMenu,
        FClose,
        FContext,
        FDropItem,
        FRange,
        FToggle,
        FBlank,
    >,
    theme: &Theme,
) -> impl IntoElement
where
    FSelect: Fn(WorkspaceFile, &mut App) + 'static + Clone,
    FBeginCreate: Fn(Option<String>, bool, &mut App) + 'static + Clone,
    FDeleteFile: Fn(String, &mut App) + 'static + Clone,
    FDeleteFolder: Fn(String, &mut App) + 'static + Clone,
    FToggleFolder: Fn(String, &mut App) + 'static + Clone,
    FWorkspaceMenu: Fn(&mut App) + 'static + Clone,
    FClose: Fn(&mut App) + 'static + Clone,
    FContext: Fn(SidebarTarget, Point<Pixels>, &mut App) + 'static + Clone,
    FDropItem: Fn(Vec<SidebarTarget>, Option<String>, bool, &mut App) + 'static + Clone,
    FRange: Fn(SidebarTarget, &mut App) + 'static + Clone,
    FToggle: Fn(SidebarTarget, &mut App) + 'static + Clone,
    FBlank: Fn(&mut App) + 'static + Clone,
{
    let on_select = props.on_select_file;
    let on_begin_create = props.on_begin_create;
    let on_delete_file = props.on_delete_file;
    let on_delete_folder = props.on_delete_folder;
    let on_toggle_folder = props.on_toggle_folder;
    let on_workspace_menu = props.on_workspace_menu;
    let workspace_menu_open = props.workspace_menu_open;
    let on_context = props.on_context_menu;
    let on_drop_item = props.on_drop_item;
    let on_range_select = props.on_range_select;
    let on_toggle_select = props.on_toggle_select;
    let on_blank_click = props.on_blank_click;
    let multi_ids: std::collections::HashSet<String> = props
        .multi_targets
        .iter()
        .map(|t| match t {
            SidebarTarget::File(id) | SidebarTarget::Folder(id) => id.clone(),
        })
        .collect();
    let multi_targets = props.multi_targets;
    let creating = props.creating.clone();
    let create_field = props.create_field.clone();
    let search_field = props.search_field.clone();
    let dragging = props.dragging;
    let focus_handle = props.focus_handle;
    let selected_file_id = props.selected_file_id;
    let open_file_id = props.open_file_id;
    let selected_folder_id = props.selected_folder_id;
    let modified_ids = props.modified_file_ids;

    let total_notes = props.workspace.files.len();
    let is_searching = !props.search_query.trim().is_empty();

    let tree_items: Vec<SidebarTreeItem> = if is_searching {
        let folders = sidebar_folder_search_matches(&props.workspace.folders, &props.search_query)
            .into_iter()
            .cloned()
            .map(|folder| SidebarTreeItem::Folder { folder, depth: 0 });
        let files = sidebar_search_matches(&props.workspace.files, &props.search_query)
            .into_iter()
            .cloned()
            .map(|file| SidebarTreeItem::File { file, depth: 0 });
        folders.chain(files).collect()
    } else {
        build_sidebar_tree(
            &props.workspace.folders,
            &props.workspace.files,
            creating.as_ref(),
        )
    };

    let show_empty_notes = creating.is_none()
        && !is_searching
        && props.workspace.folders.is_empty()
        && props.workspace.files.is_empty();
    let show_empty_search = is_searching && tree_items.is_empty();
    let workspace_title = props
        .workspace
        .folder_name
        .trim_start_matches('\u{1F4C1}')
        .trim()
        .to_string();
    let header_subtitle = if !multi_ids.is_empty() {
        format!("{} selected", multi_ids.len())
    } else if !props.workspace.is_linked_to_local_disk {
        "No folder linked".to_string()
    } else if total_notes == 1 {
        "1 note".to_string()
    } else {
        format!("{total_notes} notes")
    };

    div()
        .id("sidebar-root")
        .key_context("Sidebar")
        .track_focus(&focus_handle)
        .flex()
        .flex_col()
        .w(props.width)
        .h_full()
        .bg(theme.bg_sidebar)
        .border_r_1()
        .border_color(theme.border_primary)
        .flex_shrink_0()
        // Header: workspace name (click to switch folder) and new note / folder
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .overflow_hidden()
                .h(px(56.0))
                .px(px(12.0))
                .bg(theme.bg_sidebar_header)
                .child(
                    div()
                        .id("sb-header-dir")
                        .flex()
                        .flex_shrink()
                        .min_w_0()
                        .overflow_hidden()
                        .items_center()
                        .mr(px(8.0))
                        .ml(px(-8.0))
                        .px(px(8.0))
                        .py(px(4.0))
                        .rounded(px(6.0))
                        .cursor_pointer()
                        .when(workspace_menu_open, |d| d.bg(theme.bg_active))
                        .hover(|h| h.bg(theme.bg_hover))
                        .active(|a| a.bg(theme.bg_active))
                        // Mouse-down (not click) so the window's own
                        // click-away handler doesn't close the menu first.
                        .on_mouse_down(MouseButton::Left, {
                            let toggle = on_workspace_menu.clone();
                            move |_, _, cx| {
                                cx.stop_propagation();
                                toggle(cx)
                            }
                        })
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .min_w_0()
                                .overflow_hidden()
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(4.0))
                                        .child(
                                            div()
                                                .text_size(px(14.0))
                                                .font_weight(FontWeight::MEDIUM)
                                                .text_color(theme.text_hi)
                                                .truncate()
                                                .child(workspace_title.clone()),
                                        )
                                        .child(render_icon(
                                            if workspace_menu_open { "chevron-up" } else { "chevron-down" },
                                            12.0,
                                            theme.text_primary,
                                        )),
                                )
                                .child(
                                    div()
                                        .text_size(px(11.0))
                                        .text_color(theme.text_muted)
                                        .truncate()
                                        .child(header_subtitle.clone()),
                                ),
                        ),
                )
                // Action icons (32x32, gap 0)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .flex_shrink_0()
                        .gap_0()
                        .child({
                            let bc = on_begin_create.clone();
                            icon_button(
                                "sb-add-file",
                                "file-plus",
                                "New Note (Ctrl+N)",
                                false,
                                theme,
                                move |_, _, cx| bc(None, false, cx),
                            )
                        })
                        .child({
                            let bc = on_begin_create.clone();
                            icon_button(
                                "sb-add-folder",
                                "folder-plus",
                                "New Folder (Ctrl+Shift+N)",
                                false,
                                theme,
                                move |_, _, cx| bc(None, true, cx),
                            )
                        })
                ),
        )
        // Search
        .child(
            div()
                .px(px(10.0))
                .pb(px(8.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .h(px(30.0))
                        .px(px(8.0))
                        .rounded(sz::R_MD)
                        .bg(theme.bg_input)
                        .child(render_icon("search", 14.0, theme.text_muted))
                        .child(div().flex_1().min_w_0().child(search_field.clone())),
                ),
        )
        // Files list
        .child(
            div()
                .id("sb-files-scroll")
                .on_click(move |_, _, cx| on_blank_click(cx))
                .flex_1()
                .overflow_y_scroll()
                .p(px(6.0))
                // Dropping a note on the list (outside any folder row) moves
                // it back to the workspace root; folder rows consume their
                // own drops first.
                .on_drop({
                    let drop_item = on_drop_item.clone();
                    move |drag: &SidebarDrag, window, cx| {
                        let copy = window.modifiers().control;
                        drop_item(drag.targets.clone(), None, copy, cx);
                    }
                })
                .when(show_empty_notes, |s| {
                    let bc = on_begin_create.clone();
                    s.child(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .justify_center()
                            .py(px(40.0))
                            .gap(px(8.0))
                            .child(render_icon("file-text", 24.0, theme.text_muted))
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .text_color(theme.text_muted)
                                    .child(if !props.workspace.is_linked_to_local_disk {
                                        "No folder linked"
                                    } else {
                                        "No notes yet"
                                    }),
                            )
                            .child(
                                div()
                                    .id("sb-empty-new-note")
                                    .cursor_pointer()
                                    .text_size(px(12.0))
                                    .text_color(theme.text_hi)
                                    .hover(|h| h.underline())
                                    .on_click(move |_, _, cx| {
                                        bc(None, false, cx)
                                    })
                                    .child("New note (Ctrl+N)"),
                            ),
                    )
                })
                .when(show_empty_search, |s| {
                    s.child(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .justify_center()
                            .py(px(40.0))
                            .gap(px(8.0))
                            .child(render_icon("search", 24.0, theme.text_muted))
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .text_color(theme.text_muted)
                                    .child("No matching notes"),
                            ),
                    )
                })
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .children(tree_items.into_iter().map(|item| {
                            match item {
                                SidebarTreeItem::CreateInput { depth, is_folder } => {
                                    let inset = if depth == 0 {
                                        0.0
                                    } else {
                                        21.0 + depth as f32 * 14.0
                                    };
                                    div()
                                        .id("sb-create-row")
                                        .group("create-row")
                                        .flex()
                                        .items_center()
                                        .gap(px(6.0))
                                        .h(sz::ROW_H)
                                        .ml(px(inset))
                                        .pl(px(5.0))
                                        .pr(px(6.0))
                                        .rounded(sz::R_MD)
                                        .bg(theme.row_active)
                                        .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                            cx.stop_propagation();
                                        })
                                        .child(render_icon(
                                            if is_folder { "folder-plus" } else { "file-plus" },
                                            14.0,
                                            theme.text_hi,
                                        ))
                                        .child(create_field.clone())
                                }
                                SidebarTreeItem::Folder { folder, depth } => {
                                    let folder_id = folder.id.clone();
                                    let is_open = folder.is_open;
                                    let in_multi = multi_ids.contains(&folder_id);
                                    let is_selected = !in_multi
                                        && selected_folder_id.as_deref() == Some(&folder_id);
                                    let range = on_range_select.clone();
                                    let toggle_sel = on_toggle_select.clone();
                                    let create = on_begin_create.clone();
                                    let delete = on_delete_folder.clone();
                                    let on_context = on_context.clone();
                                    let toggle = on_toggle_folder.clone();
                                    let name = folder.name.clone();

                                    div()
                                        .id(SharedString::from(format!("folder-row-{}", folder_id)))
                                        .group("folder-row")
                                        .flex()
                                        .items_center()
                                        .justify_between()
                                        .h(sz::ROW_H)
                                        .pl(px(8.0 + depth as f32 * 14.0))
                                        .pr(px(6.0))
                                        .rounded(sz::R_MD)
                                        .cursor_pointer()
                                        .relative()
                                        .bg(if in_multi {
                                            theme.row_multi
                                        } else if is_selected {
                                            theme.row_active
                                        } else {
                                            rgba(0x00000000)
                                        })
                                        .text_color(if is_selected || in_multi {
                                            theme.text_strong
                                        } else {
                                            theme.row_text
                                        })
                                        .hover(|h| {
                                            h.bg(if in_multi {
                                                theme.row_multi
                                            } else {
                                                theme.row_hover
                                            })
                                            .text_color(theme.text_primary)
                                        })
                                        .when(depth > 0, |s| s.child(tree_guide(theme, 14.0 + (depth as f32 - 1.0) * 14.0)))
                                        .on_click({
                                            let fid = folder_id.clone();
                                            move |_, window, cx| {
                                                cx.stop_propagation();
                                                let m = window.modifiers();
                                                if m.shift {
                                                    range(SidebarTarget::Folder(fid.clone()), cx);
                                                } else if toggle_modifier(m) {
                                                    toggle_sel(SidebarTarget::Folder(fid.clone()), cx);
                                                } else {
                                                    toggle(fid.clone(), cx);
                                                }
                                            }
                                        })
                                        .on_mouse_down(MouseButton::Right, {
                                            let id = folder_id.clone();
                                            move |event, _, cx| {
                                                cx.stop_propagation();
                                                on_context(
                                                    SidebarTarget::Folder(id.clone()),
                                                    event.position,
                                                    cx,
                                                );
                                            }
                                        })
                                        // Drop-target affordance while a note or
                                        // folder is dragged.
                                        .border_1()
                                        .border_color(if dragging {
                                            theme.text_faint
                                        } else if in_multi {
                                            theme.row_multi_border
                                        } else {
                                            rgba(0x00000000)
                                        })
                                        .on_drag(
                                            drag_payload(
                                                SidebarTarget::Folder(folder_id.clone()),
                                                name.clone(),
                                                in_multi,
                                                &multi_targets,
                                            ),
                                            {
                                                let theme = *theme;
                                                move |value, point, _window, cx| {
                                                    cx.new(|_| DragItemGhost {
                                                        theme,
                                                        is_folder: matches!(
                                                            value.targets.as_slice(),
                                                            [SidebarTarget::Folder(_)]
                                                        ),
                                                        name: value.name.clone().into(),
                                                        grab_offset: point,
                                                    })
                                                }
                                            },
                                        )
                                        .on_drop({
                                            let fid = folder_id.clone();
                                            let drop_item = on_drop_item.clone();
                                            move |drag: &SidebarDrag, window, cx| {
                                                let copy = window.modifiers().control;
                                                drop_item(
                                                    drag.targets.clone(),
                                                    Some(fid.clone()),
                                                    copy,
                                                    cx,
                                                );
                                            }
                                        })
                                        .child(
                                            div()
                                                .flex()
                                                .flex_1()
                                                .min_w_0()
                                                .items_center()
                                                .gap(px(6.0))
                                                .child(
                                                    div()
                                                        .w(px(12.0))
                                                        .h(px(12.0))
                                                        .flex()
                                                        .items_center()
                                                        .justify_center()
                                                        .flex_shrink_0()
                                                        .child(render_icon(
                                                            if is_open {
                                                                "chevron-down"
                                                            } else {
                                                                "chevron-right"
                                                            },
                                                            12.0,
                                                            theme.text_muted,
                                                        )),
                                                )
                                                .child(
                                                    div()
                                                        .text_size(px(13.0))
                                                        .text_color(theme.row_text)
                                                        .truncate()
                                                        .child(name),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .flex()
                                                .flex_shrink_0()
                                                .items_center()
                                                .gap(px(2.0))
                                                .opacity(0.0)
                                                .group_hover("folder-row", |s| s.opacity(1.0))
                                                .child({
                                                    let id = folder_id.clone();
                                                    let create = create.clone();
                                                    div()
                                                        .id(SharedString::from(format!(
                                                            "folder-add-{}",
                                                            id
                                                        )))
                                                        .cursor_pointer()
                                                        .p(px(4.0))
                                                        .rounded(sz::R_SM)
                                                        .hover(|h| h.bg(theme.bg_hover))
                                                        .on_click(move |_, _, cx| {
                                                            cx.stop_propagation();
                                                            create(Some(id.clone()), false, cx)
                                                        })
                                                        .child(render_icon(
                                                            "file-plus",
                                                            13.0,
                                                            theme.text_muted,
                                                        ))
                                                })
                                                .child({
                                                    let id = folder_id.clone();
                                                    let create = create.clone();
                                                    div()
                                                        .id(SharedString::from(format!(
                                                            "folder-mkdir-{}",
                                                            id
                                                        )))
                                                        .cursor_pointer()
                                                        .p(px(4.0))
                                                        .rounded(sz::R_SM)
                                                        .hover(|h| h.bg(theme.bg_hover))
                                                        .on_click(move |_, _, cx| {
                                                            cx.stop_propagation();
                                                            create(Some(id.clone()), true, cx)
                                                        })
                                                        .child(render_icon(
                                                            "folder-plus",
                                                            13.0,
                                                            theme.text_muted,
                                                        ))
                                                })
                                                .child({
                                                    let id = folder_id.clone();
                                                    div()
                                                        .id(SharedString::from(format!(
                                                            "folder-del-{}",
                                                            id
                                                        )))
                                                        .cursor_pointer()
                                                        .p(px(4.0))
                                                        .rounded(sz::R_SM)
                                                        .hover(|h| h.bg(theme.bg_hover))
                                                        .on_click(move |_, _, cx| {
                                                            cx.stop_propagation();
                                                            delete(id.clone(), cx);
                                                        })
                                                        .child(render_icon(
                                                            "trash-2",
                                                            13.0,
                                                            theme.text_muted,
                                                        ))
                                                }),
                                        )
                                }
                                SidebarTreeItem::File { file, depth } => {
                                    let in_multi = multi_ids.contains(&file.id);
                                    let is_active = !in_multi
                                        && selected_file_id.as_deref() == Some(&file.id);
                                    let is_open = !in_multi
                                        && !is_active
                                        && open_file_id.as_deref() == Some(&file.id);
                                    let display_name = file
                                        .name
                                        .strip_suffix(".md")
                                        .unwrap_or(file.name.as_str())
                                        .to_string();
                                    let range = on_range_select.clone();
                                    let toggle_sel = on_toggle_select.clone();
                                    let is_modified =
                                        modified_ids.contains(&file.id);
                                    let file_clone = file.clone();
                                    let sel = on_select.clone();
                                    let del = on_delete_file.clone();
                                    let on_context = on_context.clone();
                                    let fid = file.id.clone();
                                    // Root notes sit flush left (same edge as the
                                    // folder rows themselves). Nested notes are
                                    // inset so the highlight starts at the
                                    // indicator bar. Either way the icon + text
                                    // hug the bar (2px gap).
                                    let inset = if depth == 0 {
                                        0.0
                                    } else {
                                        12.0 + depth as f32 * 14.0
                                    };

                                    div()
                                        .id(SharedString::from(file.id.clone()))
                                        .group("note-row")
                                        .relative()
                                        .flex()
                                        .items_center()
                                        .justify_between()
                                        .h(sz::ROW_H)
                                        .ml(px(inset))
                                        .pl(px(5.0))
                                        .pr(px(6.0))
                                        .rounded(sz::R_MD)
                                        .cursor_pointer()
                                        .border_1()
                                        .border_color(if in_multi {
                                            theme.row_multi_border
                                        } else {
                                            rgba(0x00000000)
                                        })
                                        .bg(if in_multi {
                                            theme.row_multi
                                        } else if is_active {
                                            theme.row_active
                                        } else if is_open {
                                            theme.row_hover
                                        } else {
                                            rgba(0x00000000)
                                        })
                                        .text_color(if is_active || is_open || in_multi {
                                            theme.text_strong
                                        } else {
                                            theme.row_text
                                        })
                                        .when(!is_active && !in_multi, |s| {
                                            s.hover(|h| {
                                                h.bg(theme.row_hover).text_color(theme.text_primary)
                                            })
                                        })
                                        .when(depth > 0, |s| {
                                            s.child(tree_guide(theme, 14.0 + (depth as f32 - 1.0) * 14.0 - inset))
                                        })
                                        .on_click(move |_, window, cx| {
                                            cx.stop_propagation();
                                            let m = window.modifiers();
                                            if m.shift {
                                                range(SidebarTarget::File(file_clone.id.clone()), cx);
                                            } else if toggle_modifier(m) {
                                                toggle_sel(SidebarTarget::File(file_clone.id.clone()), cx);
                                            } else {
                                                sel(file_clone.clone(), cx);
                                            }
                                        })
                                        .on_mouse_down(MouseButton::Right, {
                                            let id = fid.clone();
                                            move |event, _, cx| {
                                                cx.stop_propagation();
                                                on_context(
                                                    SidebarTarget::File(id.clone()),
                                                    event.position,
                                                    cx,
                                                );
                                            }
                                        })
                                        .on_drag(
                                            drag_payload(
                                                SidebarTarget::File(fid.clone()),
                                                file.name.clone(),
                                                in_multi,
                                                &multi_targets,
                                            ),
                                            {
                                                let theme = *theme;
                                                move |value, point, _window, cx| {
                                                    cx.new(|_| DragItemGhost {
                                                        theme,
                                                        is_folder: matches!(
                                                            value.targets.as_slice(),
                                                            [SidebarTarget::Folder(_)]
                                                        ),
                                                        name: value.name.clone().into(),
                                                        grab_offset: point,
                                                    })
                                                }
                                            },
                                        )
                                        // Active 3px indicator bar, right before
                                        // the note icon at the row's inset edge.
                                        .when(is_active, |s| {
                                            s.child(
                                                div()
                                                    .absolute()
                                                    .left_0()
                                                    .top(px(4.0))
                                                    .bottom(px(4.0))
                                                    .w(px(3.0))
                                                    .rounded_r(px(2.0))
                                                    .bg(theme.text_hi),
                                            )
                                        })
                                        .child(
                                            div()
                                                .flex()
                                                .flex_1()
                                                .min_w_0()
                                                .items_center()
                                                .gap(px(6.0))
                                                .child(render_icon(
                                                    "file-text",
                                                    14.0,
                                                    if is_active {
                                                        theme.row_icon_active
                                                    } else {
                                                        theme.text_muted
                                                    },
                                                ))
                                                .child(
                                                    div()
                                                        .text_size(px(13.0))
                                                        .font_weight(if is_active || is_open {
                                                            FontWeight::MEDIUM
                                                        } else {
                                                            FontWeight::NORMAL
                                                        })
                                                        .truncate()
                                                        .child(display_name),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .flex()
                                                .flex_shrink_0()
                                                .items_center()
                                                .gap(px(6.0))
                                                .when(is_modified, |s| {
                                                    s.child(
                                                        div()
                                                            .w(px(6.0))
                                                            .h(px(6.0))
                                                            .rounded_full()
                                                            .bg(theme.text_hi),
                                                    )
                                                })
                                                .child({
                                                    div()
                                                        .id(SharedString::from(format!(
                                                            "sb-del-{}",
                                                            fid
                                                        )))
                                                        .opacity(0.0)
                                                        .group_hover("note-row", |s| s.opacity(1.0))
                                                        .w(px(24.0))
                                                        .h(px(24.0))
                                                        .rounded(sz::R_SM)
                                                        .flex()
                                                        .items_center()
                                                        .justify_center()
                                                        .cursor_pointer()
                                                        .hover(|h| {
                                                            h.bg(theme.bg_hover)
                                                                .text_color(theme.text_primary)
                                                        })
                                                        .on_click(move |_, _, cx| {
                                                            cx.stop_propagation();
                                                            del(fid.clone(), cx);
                                                        })
                                                        .child(render_icon(
                                                            "trash-2",
                                                            13.0,
                                                            theme.text_muted,
                                                        ))
                                                }),
                                        )
                                }
                            }
                        })),
                ),
        )
}

/// Dropdown under the sidebar header: current folder, recent folders and a
/// "Choose folder" entry. Click outside to dismiss.
pub fn render_workspace_menu<FPick, FChoose, FDismiss>(
    current_path: String,
    recents: Vec<(String, std::path::PathBuf)>,
    on_pick: FPick,
    on_choose: FChoose,
    on_dismiss: FDismiss,
    theme: &Theme,
) -> impl IntoElement
where
    FPick: Fn(std::path::PathBuf, &mut App) + 'static + Clone,
    FChoose: Fn(&mut App) + 'static + Clone,
    FDismiss: Fn(&mut App) + 'static + Clone,
{
    let dismiss_left = on_dismiss.clone();
    let dismiss_right = on_dismiss.clone();
    let row = |id: SharedString| {
        div()
            .id(id)
            .flex()
            .items_center()
            .gap(px(8.0))
            .px(px(10.0))
            .h(px(32.0))
            .rounded(sz::R_SM)
            .cursor_pointer()
            .text_size(px(13.0))
            .text_color(theme.text_primary)
            .hover(|h| h.bg(theme.bg_hover))
    };
    div()
        .id("workspace-menu-overlay")
        .absolute()
        .size_full()
        .top_0()
        .left_0()
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            dismiss_left(cx);
            cx.stop_propagation();
        })
        .on_mouse_down(MouseButton::Right, move |_, _, cx| {
            dismiss_right(cx);
            cx.stop_propagation();
        })
        .child(
            div()
                .absolute()
                .left(px(10.0))
                .top(px(54.0))
                .w(px(284.0))
                .p(px(6.0))
                .rounded(sz::R_MD)
                .bg(theme.menu_bg)
                .border_1()
                .border_color(theme.border_surface)
                .shadow_lg()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
                .child(
                    div()
                        .px(px(10.0))
                        .pt(px(4.0))
                        .pb(px(8.0))
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(theme.text_muted)
                                .child("Current folder"),
                        )
                        .child(
                            div()
                                .text_size(px(12.0))
                                .text_color(theme.text_primary)
                                .truncate()
                                .child(current_path),
                        ),
                )
                .when(!recents.is_empty(), |s| {
                    s.child(div().h(px(1.0)).bg(theme.border_subtle).mb(px(4.0)))
                        .child(
                            div()
                                .px(px(10.0))
                                .py(px(2.0))
                                .text_size(px(11.0))
                                .text_color(theme.text_muted)
                                .child("Recent"),
                        )
                        .children(recents.into_iter().enumerate().map(|(ix, (name, path))| {
                            let pick = on_pick.clone();
                            let target = path.clone();
                            row(SharedString::from(format!("workspace-recent-{ix}")))
                                .on_click(move |_, _, cx| pick(target.clone(), cx))
                                .child(render_icon("folder", 14.0, theme.text_muted))
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .min_w_0()
                                        .child(div().truncate().child(name))
                                        .child(
                                            div()
                                                .text_size(px(11.0))
                                                .text_color(theme.text_muted)
                                                .truncate()
                                                .child(path.display().to_string()),
                                        ),
                                )
                        }))
                })
                .child(div().h(px(1.0)).bg(theme.border_subtle).my(px(4.0)))
                .child(
                    row(SharedString::from("workspace-choose"))
                        .on_click(move |_, _, cx| on_choose(cx))
                        .child(render_icon("folder-open", 14.0, theme.text_muted))
                        .child("Choose folder..."),
                ),
        )
}

pub fn render_sidebar_context_menu<FRename, FDelete, FDismiss>(
    menu: &SidebarMenu,
    selected_ix: usize,
    focus_handle: FocusHandle,
    on_rename: FRename,
    on_delete: FDelete,
    on_dismiss: FDismiss,
    theme: &Theme,
) -> impl IntoElement
where
    FRename: Fn(&mut App) + 'static + Clone,
    FDelete: Fn(&mut App) + 'static + Clone,
    FDismiss: Fn(&mut App) + 'static + Clone,
{
    let dismiss_left = on_dismiss.clone();
    let dismiss_right = on_dismiss.clone();
    div()
        .id("sidebar-context-overlay")
        .key_context("PromptModal")
        .track_focus(&focus_handle)
        .absolute()
        .size_full()
        .top_0()
        .left_0()
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            dismiss_left(cx);
            cx.stop_propagation();
        })
        .on_mouse_down(MouseButton::Right, move |_, _, cx| {
            dismiss_right(cx);
            cx.stop_propagation();
        })
        .child(
            div()
                .absolute()
                .left(menu.position.x)
                .top(menu.position.y)
                .w(px(168.0))
                .py(px(4.0))
                .rounded(sz::R_MD)
                .bg(theme.menu_bg)
                .border_1()
                .border_color(theme.border_surface)
                .shadow_lg()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
                .child({
                    let on_rename = on_rename.clone();
                    div()
                        .id("sidebar-menu-rename")
                        .px(px(12.0))
                        .py(px(6.0))
                        .cursor_pointer()
                        .text_size(px(13.0))
                        .text_color(if selected_ix == 0 {
                            theme.text_hi
                        } else {
                            theme.text_primary
                        })
                        .bg(if selected_ix == 0 {
                            theme.bg_hover
                        } else {
                            rgba(0x00000000)
                        })
                        .hover(|style| style.bg(theme.bg_hover))
                        .on_click(move |_, _, cx| on_rename(cx))
                        .child("Rename")
                })
                .child({
                    let on_delete = on_delete.clone();
                    div()
                        .id("sidebar-menu-delete")
                        .px(px(12.0))
                        .py(px(6.0))
                        .cursor_pointer()
                        .text_size(px(13.0))
                        .text_color(if selected_ix == 1 {
                            theme.text_hi
                        } else {
                            theme.text_primary
                        })
                        .bg(if selected_ix == 1 {
                            theme.bg_hover
                        } else {
                            rgba(0x00000000)
                        })
                        .hover(|style| style.bg(theme.bg_hover))
                        .on_click(move |_, _, cx| on_delete(cx))
                        .child("Delete")
                }),
        )
}


