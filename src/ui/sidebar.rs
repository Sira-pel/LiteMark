use gpui::prelude::*;
use gpui::*;
use crate::model::{WorkspaceDirectory, WorkspaceFile};
use crate::theme::Theme;
use super::primitives::{icon_button, render_icon};

#[allow(dead_code)]
pub struct SidebarProps<FSelect, FCreateFile, FCreateFolder, FDeleteFile, FDeleteFolder, FChooseDir, FClose>
where
    FSelect: Fn(WorkspaceFile, &mut App) + 'static + Clone,
    FCreateFile: Fn(Option<String>, String, &mut App) + 'static + Clone,
    FCreateFolder: Fn(Option<String>, String, &mut App) + 'static + Clone,
    FDeleteFile: Fn(String, &mut App) + 'static + Clone,
    FDeleteFolder: Fn(String, &mut App) + 'static + Clone,
    FChooseDir: Fn(&mut App) + 'static + Clone,
    FClose: Fn(&mut App) + 'static + Clone,
{
    pub workspace: WorkspaceDirectory,
    pub active_file_id: Option<String>,
    pub open_file_ids: Vec<String>,
    pub search_query: String,
    pub creating_in_folder: Option<Option<String>>, // Some(folder_id) when creating a note
    pub on_select_file: FSelect,
    pub on_create_file: FCreateFile,
    pub on_create_folder: FCreateFolder,
    pub on_delete_file: FDeleteFile,
    pub on_delete_folder: FDeleteFolder,
    pub on_choose_dir: FChooseDir,
    pub on_close: FClose,
}

pub fn render_sidebar<FSelect, FCreateFile, FCreateFolder, FDeleteFile, FDeleteFolder, FChooseDir, FClose>(
    props: SidebarProps<FSelect, FCreateFile, FCreateFolder, FDeleteFile, FDeleteFolder, FChooseDir, FClose>,
    theme: &Theme,
) -> impl IntoElement
where
    FSelect: Fn(WorkspaceFile, &mut App) + 'static + Clone,
    FCreateFile: Fn(Option<String>, String, &mut App) + 'static + Clone,
    FCreateFolder: Fn(Option<String>, String, &mut App) + 'static + Clone,
    FDeleteFile: Fn(String, &mut App) + 'static + Clone,
    FDeleteFolder: Fn(String, &mut App) + 'static + Clone,
    FChooseDir: Fn(&mut App) + 'static + Clone,
    FClose: Fn(&mut App) + 'static + Clone,
{
    let on_select = props.on_select_file;
    let on_create_file = props.on_create_file;
    let on_create_folder = props.on_create_folder;
    let on_delete_file = props.on_delete_file;
    let _on_delete_folder = props.on_delete_folder;
    let on_choose_dir = props.on_choose_dir;
    let on_close = props.on_close;
    let active_id = props.active_file_id;
    let open_ids = props.open_file_ids;

    let total_notes = props.workspace.files.len();
    let query = props.search_query.to_lowercase();
    let files: Vec<WorkspaceFile> = if query.is_empty() {
        props.workspace.files.clone()
    } else {
        props.workspace.files
            .iter()
            .filter(|f| f.name.to_lowercase().contains(&query))
            .cloned()
            .collect()
    };

    div()
        .flex()
        .flex_col()
        .w(px(260.0))
        .h_full()
        .bg(theme.bg_sidebar)
        .border_r_1()
        .border_color(theme.border_subtle)
        .flex_shrink_0()
        // Header
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .px(px(10.0))
                .py(px(8.0))
                .bg(theme.bg_sidebar_header)
                .border_b_1()
                .border_color(theme.border_subtle)
                .child(
                    div()
                        .id("sb-header-dir")
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .cursor_pointer()
                        .on_click({
                            let cd = on_choose_dir.clone();
                            move |_, _, cx| cd(cx)
                        })
                        .child(
                            div()
                                .p(px(4.0))
                                .rounded(px(4.0))
                                .bg(rgba(0xe9542026))
                                .child(render_icon("folder-tree", 16.0, theme.accent_ubuntu))
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .child(
                                    div()
                                        .text_size(px(12.0))
                                        .font_weight(FontWeight::BOLD)
                                        .text_color(theme.text_primary)
                                        .child(props.workspace.folder_name.clone())
                                )
                                .child(
                                    div()
                                        .text_size(px(10.0))
                                        .text_color(theme.accent_emerald)
                                        .child("● Local Disk Linked")
                                )
                        )
                )
                // Action icons
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(2.0))
                        .child({
                            let cf = on_create_file.clone();
                            icon_button("sb-add-file", "file-plus", "New Note", false, theme, move |_, _, cx| cf(None, "New Note.md".to_string(), cx))
                        })
                        .child({
                            let cdir = on_create_folder.clone();
                            icon_button("sb-add-folder", "folder-plus", "New Folder", false, theme, move |_, _, cx| cdir(None, "New Folder".to_string(), cx))
                        })
                        .child({
                            let pick = on_choose_dir.clone();
                            icon_button("sb-pick-dir", "hard-drive", "Choose Folder on Disk", false, theme, move |_, _, cx| pick(cx))
                        })
                        .child({
                            let cls = on_close.clone();
                            icon_button("sb-close", "x", "Close Sidebar", false, theme, move |_, _, cx| cls(cx))
                        })
                )
        )
        // Search bar
        .child(
            div()
                .px(px(8.0))
                .py(px(6.0))
                .border_b_1()
                .border_color(theme.border_subtle)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .px(px(8.0))
                        .py(px(4.0))
                        .rounded(px(4.0))
                        .bg(theme.bg_input)
                        .border_1()
                        .border_color(theme.border_primary)
                        .child(render_icon("search", 14.0, theme.text_muted))
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(theme.text_muted)
                                .child("Search notes in directory...")
                        )
                )
        )
        // Files list
        .child(
            div()
                .id("sb-files-scroll")
                .flex_1()
                .overflow_y_scroll()
                .p(px(4.0))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .children(files.into_iter().map(|file| {
                            let is_active = active_id.as_deref() == Some(&file.id);
                            let is_open = open_ids.contains(&file.id);
                            let file_clone = file.clone();
                            let sel = on_select.clone();
                            let del = on_delete_file.clone();
                            let fid = file.id.clone();

                            div()
                                .id(SharedString::from(file.id.clone()))
                                .flex()
                                .items_center()
                                .justify_between()
                                .px(px(8.0))
                                .py(px(6.0))
                                .rounded(px(4.0))
                                .cursor_pointer()
                                .bg(if is_active { rgba(0xe9542033) } else { rgba(0x00000000) })
                                .border_l_2()
                                .border_color(if is_active { theme.accent_ubuntu } else { rgba(0x00000000) })
                                .text_color(if is_active { theme.text_primary } else { theme.text_muted })
                                .hover(|h| h.bg(theme.bg_hover).text_color(theme.text_primary))
                                .on_click(move |_, _, cx| sel(file_clone.clone(), cx))
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(6.0))
                                        .child(render_icon("file-text", 14.0, if is_active { theme.accent_ubuntu } else { theme.text_muted }))
                                        .child(
                                            div()
                                                .text_size(px(12.0))
                                                .font_weight(if is_active { FontWeight::MEDIUM } else { FontWeight::NORMAL })
                                                .child(file.name)
                                        )
                                )
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(4.0))
                                        .when(is_open, |s| {
                                            s.child(
                                                div()
                                                    .w(px(6.0))
                                                    .h(px(6.0))
                                                    .rounded_full()
                                                    .bg(theme.accent_emerald)
                                            )
                                        })
                                        .child({
                                            div()
                                                .id(SharedString::from(format!("sb-del-{}", fid)))
                                                .p(px(2.0))
                                                .rounded(px(2.0))
                                                .cursor_pointer()
                                                .hover(|h| h.text_color(theme.accent_danger))
                                                .on_click(move |_, _, cx| {
                                                    del(fid.clone(), cx);
                                                })
                                                .child(render_icon("trash-2", 12.0, theme.text_muted))
                                        })
                                )
                        }))
                )
        )
        // Footer stats
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .px(px(10.0))
                .py(px(6.0))
                .bg(theme.bg_sidebar_header)
                .border_t_1()
                .border_color(theme.border_subtle)
                .text_size(px(11.0))
                .text_color(theme.text_muted)
                .child(format!("{} notes", total_notes))
                .child(
                    div()
                        .id("sb-footer-change-dir")
                        .cursor_pointer()
                        .text_color(theme.accent_ubuntu)
                        .hover(|h| h.underline())
                        .on_click(move |_, _, cx| on_choose_dir(cx))
                        .child("Change Folder")
                )
        )
}
