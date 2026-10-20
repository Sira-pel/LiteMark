use super::primitives::{icon_button, render_icon};
use crate::model::{WorkspaceDirectory, WorkspaceFile};
use crate::theme::{sz, Theme};
use gpui::prelude::*;
use gpui::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SidebarTarget {
    File(String),
    Folder(String),
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
}

pub fn build_sidebar_tree(
    folders: &[crate::model::WorkspaceFolder],
    files: &[crate::model::WorkspaceFile],
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
        out: &mut Vec<SidebarTreeItem>,
    ) {
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
        level_folders.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

        for folder in level_folders {
            let is_open = folder.is_open;
            let fid = folder.id.clone();
            out.push(SidebarTreeItem::Folder {
                folder: folder.clone(),
                depth,
            });
            if is_open {
                collect_level(Some(&fid), depth + 1, folders, files, folder_ids, out);
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
        level_files.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

        for file in level_files {
            out.push(SidebarTreeItem::File {
                file: file.clone(),
                depth,
            });
        }
    }

    collect_level(None, 0, folders, files, &folder_ids, &mut items);
    items
}

#[allow(dead_code)]
pub struct SidebarProps<
    FSelect,
    FCreateFile,
    FCreateFolder,
    FDeleteFile,
    FDeleteFolder,
    FToggleFolder,
    FChooseDir,
    FClose,
    FContext,
> where
    FSelect: Fn(WorkspaceFile, &mut App) + 'static + Clone,
    FCreateFile: Fn(Option<String>, String, &mut App) + 'static + Clone,
    FCreateFolder: Fn(Option<String>, String, &mut App) + 'static + Clone,
    FDeleteFile: Fn(String, &mut App) + 'static + Clone,
    FDeleteFolder: Fn(String, &mut App) + 'static + Clone,
    FToggleFolder: Fn(String, &mut App) + 'static + Clone,
    FChooseDir: Fn(&mut App) + 'static + Clone,
    FClose: Fn(&mut App) + 'static + Clone,
    FContext: Fn(SidebarTarget, Point<Pixels>, &mut App) + 'static + Clone,
{
    pub workspace: WorkspaceDirectory,
    pub active_file_id: Option<String>,
    pub open_file_ids: Vec<String>,
    pub search_query: String,
    pub creating_in_folder: Option<Option<String>>,
    pub on_select_file: FSelect,
    pub on_create_file: FCreateFile,
    pub on_create_folder: FCreateFolder,
    pub on_delete_file: FDeleteFile,
    pub on_delete_folder: FDeleteFolder,
    pub on_toggle_folder: FToggleFolder,
    pub on_choose_dir: FChooseDir,
    pub on_close: FClose,
    pub on_context_menu: FContext,
}

pub fn render_sidebar<
    FSelect,
    FCreateFile,
    FCreateFolder,
    FDeleteFile,
    FDeleteFolder,
    FToggleFolder,
    FChooseDir,
    FClose,
    FContext,
>(
    props: SidebarProps<
        FSelect,
        FCreateFile,
        FCreateFolder,
        FDeleteFile,
        FDeleteFolder,
        FToggleFolder,
        FChooseDir,
        FClose,
        FContext,
    >,
    theme: &Theme,
) -> impl IntoElement
where
    FSelect: Fn(WorkspaceFile, &mut App) + 'static + Clone,
    FCreateFile: Fn(Option<String>, String, &mut App) + 'static + Clone,
    FCreateFolder: Fn(Option<String>, String, &mut App) + 'static + Clone,
    FDeleteFile: Fn(String, &mut App) + 'static + Clone,
    FDeleteFolder: Fn(String, &mut App) + 'static + Clone,
    FToggleFolder: Fn(String, &mut App) + 'static + Clone,
    FChooseDir: Fn(&mut App) + 'static + Clone,
    FClose: Fn(&mut App) + 'static + Clone,
    FContext: Fn(SidebarTarget, Point<Pixels>, &mut App) + 'static + Clone,
{
    let on_select = props.on_select_file;
    let on_create_file = props.on_create_file;
    let on_create_folder = props.on_create_folder;
    let on_delete_file = props.on_delete_file;
    let on_delete_folder = props.on_delete_folder;
    let on_toggle_folder = props.on_toggle_folder;
    let on_choose_dir = props.on_choose_dir;
    let on_close = props.on_close;
    let on_context = props.on_context_menu;
    let active_id = props.active_file_id;
    let open_ids = props.open_file_ids;

    let total_notes = props.workspace.files.len();
    let query = props.search_query.to_lowercase();
    let is_searching = !query.is_empty();

    let tree_items: Vec<SidebarTreeItem> = if is_searching {
        props
            .workspace
            .files
            .iter()
            .filter(|f| f.name.to_lowercase().contains(&query))
            .cloned()
            .map(|file| SidebarTreeItem::File { file, depth: 0 })
            .collect()
    } else {
        build_sidebar_tree(&props.workspace.folders, &props.workspace.files)
    };

    let show_empty_notes =
        !is_searching && props.workspace.folders.is_empty() && props.workspace.files.is_empty();
    let show_empty_search = is_searching && tree_items.is_empty();

    div()
        .flex()
        .flex_col()
        .w(sz::SIDEBAR_W)
        .h_full()
        .bg(theme.bg_sidebar)
        .border_r_1()
        .border_color(theme.border_primary)
        .flex_shrink_0()
        // Header (56px)
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .overflow_hidden()
                .h(px(56.0))
                .px(px(12.0))
                .bg(theme.bg_sidebar_header)
                .border_b_1()
                .border_color(theme.border_subtle)
                .child(
                    div()
                        .id("sb-header-dir")
                        .flex()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .items_center()
                        .mr(px(8.0))
                        .cursor_pointer()
                        .on_click({
                            let cd = on_choose_dir.clone();
                            move |_, _, cx| cd(cx)
                        })
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .child(
                                    div()
                                        .text_size(px(14.0))
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(theme.text_hi)
                                        .truncate()
                                        .child(props.workspace.folder_name.clone()),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(6.0))
                                        .child(
                                            div()
                                                .flex_shrink_0()
                                                .w(px(6.0))
                                                .h(px(6.0))
                                                .rounded_full()
                                                .bg(theme.text_faint),
                                        )
                                        .child(
                                            div()
                                                .text_size(px(11.0))
                                                .text_color(theme.text_muted)
                                                .truncate()
                                                .child(
                                                    if props.workspace.is_linked_to_local_disk {
                                                        "Local Disk Linked"
                                                    } else {
                                                        "No folder linked"
                                                    },
                                                ),
                                        ),
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
                            let cf = on_create_file.clone();
                            icon_button(
                                "sb-add-file",
                                "file-plus",
                                "New Note (Ctrl+N)",
                                false,
                                theme,
                                move |_, _, cx| cf(None, "New Note.md".to_string(), cx),
                            )
                        })
                        .child({
                            let cdir = on_create_folder.clone();
                            icon_button(
                                "sb-add-folder",
                                "folder-plus",
                                "New Folder",
                                false,
                                theme,
                                move |_, _, cx| cdir(None, "New Folder".to_string(), cx),
                            )
                        })
                        .child({
                            let pick = on_choose_dir.clone();
                            icon_button(
                                "sb-pick-dir",
                                "hard-drive",
                                "Choose Folder on Disk",
                                false,
                                theme,
                                move |_, _, cx| pick(cx),
                            )
                        })
                        .child({
                            let cls = on_close.clone();
                            icon_button(
                                "sb-close",
                                "x",
                                "Close Sidebar",
                                false,
                                theme,
                                move |_, _, cx| cls(cx),
                            )
                        }),
                ),
        )
        // Search bar (32px)
        .child(
            div()
                .px(px(10.0))
                .py(px(8.0))
                .border_b_1()
                .border_color(theme.border_subtle)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .h(px(32.0))
                        .px(px(10.0))
                        .rounded(sz::R_MD)
                        .bg(theme.bg_input)
                        .border_1()
                        .border_color(theme.border_subtle)
                        .child(render_icon("search", 14.0, theme.text_muted))
                        .child(
                            div()
                                .text_size(px(12.0))
                                .text_color(theme.placeholder)
                                .child("Search notes in directory..."),
                        ),
                ),
        )
        // Files list
        .child(
            div()
                .id("sb-files-scroll")
                .flex_1()
                .overflow_y_scroll()
                .p(px(6.0))
                .when(show_empty_notes, |s| {
                    let cf = on_create_file.clone();
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
                                        cf(None, "New Note.md".to_string(), cx)
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
                                SidebarTreeItem::Folder { folder, depth } => {
                                    let folder_id = folder.id.clone();
                                    let is_open = folder.is_open;
                                    let create = on_create_file.clone();
                                    let create_dir = on_create_folder.clone();
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
                                        .hover(|h| {
                                            h.bg(theme.row_hover).text_color(theme.text_primary)
                                        })
                                        .on_click({
                                            let fid = folder_id.clone();
                                            move |_, _, cx| toggle(fid.clone(), cx)
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
                                                .child(render_icon(
                                                    if is_open {
                                                        "folder-open"
                                                    } else {
                                                        "folder"
                                                    },
                                                    14.0,
                                                    theme.text_muted,
                                                ))
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
                                                            create(
                                                                Some(id.clone()),
                                                                "New Note.md".into(),
                                                                cx,
                                                            )
                                                        })
                                                        .child(render_icon(
                                                            "file-plus",
                                                            13.0,
                                                            theme.text_muted,
                                                        ))
                                                })
                                                .child({
                                                    let id = folder_id.clone();
                                                    let create_dir = create_dir.clone();
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
                                                            create_dir(
                                                                Some(id.clone()),
                                                                "New Folder".into(),
                                                                cx,
                                                            )
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
                                    let is_active = active_id.as_deref() == Some(&file.id);
                                    let is_open = open_ids.contains(&file.id);
                                    let file_clone = file.clone();
                                    let sel = on_select.clone();
                                    let del = on_delete_file.clone();
                                    let on_context = on_context.clone();
                                    let fid = file.id.clone();

                                    div()
                                        .id(SharedString::from(file.id.clone()))
                                        .group("note-row")
                                        .relative()
                                        .flex()
                                        .items_center()
                                        .justify_between()
                                        .h(sz::ROW_H)
                                        .pl(px(8.0 + depth as f32 * 14.0))
                                        .pr(px(6.0))
                                        .rounded(sz::R_MD)
                                        .cursor_pointer()
                                        .bg(if is_active {
                                            theme.row_active
                                        } else {
                                            rgba(0x00000000)
                                        })
                                        .text_color(if is_active {
                                            theme.text_strong
                                        } else {
                                            theme.row_text
                                        })
                                        .when(!is_active, |s| {
                                            s.hover(|h| {
                                                h.bg(theme.row_hover).text_color(theme.text_primary)
                                            })
                                        })
                                        .on_click(move |_, _, cx| sel(file_clone.clone(), cx))
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
                                        // Active 3px indicator bar
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
                                                // 12px blank spacer to align with folder chevron
                                                .child(
                                                    div()
                                                        .w(px(12.0))
                                                        .h(px(12.0))
                                                        .flex_shrink_0(),
                                                )
                                                .child(render_icon(
                                                    "file-text",
                                                    14.0,
                                                    if is_active {
                                                        rgb(0xc8c8c8)
                                                    } else {
                                                        theme.text_muted
                                                    },
                                                ))
                                                .child(
                                                    div()
                                                        .text_size(px(13.0))
                                                        .font_weight(if is_active {
                                                            FontWeight::MEDIUM
                                                        } else {
                                                            FontWeight::NORMAL
                                                        })
                                                        .truncate()
                                                        .child(file.name),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .flex()
                                                .flex_shrink_0()
                                                .items_center()
                                                .gap(px(6.0))
                                                .when(is_open, |s| {
                                                    s.child(
                                                        div()
                                                            .w(px(6.0))
                                                            .h(px(6.0))
                                                            .rounded_full()
                                                            .bg(theme.text_faint),
                                                    )
                                                })
                                                .child({
                                                    div()
                                                        .id(SharedString::from(format!(
                                                            "sb-del-{}",
                                                            fid
                                                        )))
                                                        .w(px(24.0))
                                                        .h(px(24.0))
                                                        .rounded(sz::R_SM)
                                                        .flex()
                                                        .items_center()
                                                        .justify_center()
                                                        .cursor_pointer()
                                                        .opacity(0.0)
                                                        .group_hover("note-row", |s| {
                                                            s.opacity(1.0)
                                                        })
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
        // Footer stats (40px)
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .h(px(40.0))
                .px(px(12.0))
                .bg(theme.bg_sidebar_header)
                .border_t_1()
                .border_color(theme.border_subtle)
                .text_size(px(12.0))
                .text_color(theme.text_muted)
                .child(if total_notes == 1 {
                    "1 note".to_string()
                } else {
                    format!("{} notes", total_notes)
                })
                .child(
                    div()
                        .id("sb-footer-change-dir")
                        .px(px(6.0))
                        .py(px(3.0))
                        .rounded(sz::R_SM)
                        .cursor_pointer()
                        .text_color(theme.pill_text)
                        .hover(|h| h.bg(theme.bg_hover).text_color(theme.text_primary))
                        .on_click(move |_, _, cx| on_choose_dir(cx))
                        .child("Change Folder"),
                ),
        )
}


pub fn render_sidebar_context_menu<FRename, FDelete, FDismiss>(
    menu: &SidebarMenu,
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
                        .text_color(theme.text_primary)
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
                        .text_color(theme.text_primary)
                        .hover(|style| style.bg(theme.bg_hover))
                        .on_click(move |_, _, cx| on_delete(cx))
                        .child("Delete")
                }),
        )
}

