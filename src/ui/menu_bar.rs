use super::menu::MenuItem;
use super::primitives::render_icon;
use crate::model::settings::ContentAlignment;
use crate::model::ViewMode;
use crate::theme::{sz, Theme};
use gpui::prelude::*;
use gpui::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuId {
    File,
    Edit,
    View,
    Settings,
    Overflow,
    Table,
    Hamburger,
}

pub struct MenuBarProps<FOpenMenu, FCloseMenu>
where
    FOpenMenu: Fn(MenuId, &mut App) + 'static + Clone,
    FCloseMenu: Fn(&mut App) + 'static + Clone,
{
    pub open_menu: Option<MenuId>,
    pub is_narrow: bool, // width < 640px
    pub on_open_menu: FOpenMenu,
    pub on_close_menu: FCloseMenu,
}

pub fn render_menu_bar<FOpenMenu, FCloseMenu>(
    props: MenuBarProps<FOpenMenu, FCloseMenu>,
    theme: &Theme,
) -> impl IntoElement
where
    FOpenMenu: Fn(MenuId, &mut App) + 'static + Clone,
    FCloseMenu: Fn(&mut App) + 'static + Clone,
{
    let open_menu = props.open_menu;
    let on_open = props.on_open_menu;
    let on_close = props.on_close_menu;
    let is_any_open = open_menu.is_some();

    if props.is_narrow {
        // Under 640px: single ☰ button
        let is_open = open_menu == Some(MenuId::Hamburger);
        let op = on_open.clone();
        let cl = on_close.clone();

        div()
            .id("menu-btn-hamburger")
            .flex()
            .items_center()
            .justify_center()
            .w(sz::BTN)
            .h(sz::TB_TILE_H)
            .rounded(sz::R_MD)
            .cursor_pointer()
            .bg(if is_open {
                theme.border_subtle
            } else {
                rgba(0x00000000)
            })
            .text_color(if is_open {
                theme.text_strong
            } else {
                theme.text_muted
            })
            .hover(|h| h.bg(theme.tile_hover).text_color(theme.text_hi))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(move |_, _, cx| {
                if is_open {
                    cl(cx);
                } else {
                    op(MenuId::Hamburger, cx);
                }
            })
            .child(render_icon(
                "menu",
                16.0,
                if is_open {
                    theme.text_strong
                } else {
                    theme.text_muted
                },
            ))
    } else {
        // Normal layout: File  Edit  View
        let menus = [
            ("File", MenuId::File, "menu-btn-file"),
            ("Edit", MenuId::Edit, "menu-btn-edit"),
            ("View", MenuId::View, "menu-btn-view"),
        ];

        div()
            .id("menu-bar-buttons")
            .flex()
            .items_center()
            .gap(px(2.0))
            .children(menus.into_iter().map(|(label, id, elem_id)| {
                let is_open = open_menu == Some(id);
                let op = on_open.clone();
                let cl = on_close.clone();
                let op_hover = on_open.clone();

                div()
                    .id(elem_id)
                    .flex()
                    .items_center()
                    .justify_center()
                    .h(sz::TB_TILE_H)
                    .px(px(8.0))
                    .rounded(sz::R_MD)
                    .cursor_pointer()
                    .text_size(px(13.0))
                    .font_weight(FontWeight::NORMAL)
                    .bg(if is_open {
                        theme.border_subtle
                    } else {
                        rgba(0x00000000)
                    })
                    .text_color(if is_open {
                        theme.text_strong
                    } else {
                        theme.menu_text
                    })
                    .hover(|h| h.bg(theme.tile_hover).text_color(theme.text_hi))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_mouse_move(move |_, _, cx| {
                        if is_any_open && !is_open {
                            op_hover(id, cx);
                        }
                    })
                    .on_click(move |_, _, cx| {
                        if is_open {
                            cl(cx);
                        } else {
                            op(id, cx);
                        }
                    })
                    .child(label)
            }))
    }
}

#[allow(clippy::too_many_arguments)]
pub fn build_file_menu_items(
    has_pdf: bool,
    on_new_note: impl Fn(&mut App) + 'static + Clone,
    on_new_folder: impl Fn(&mut App) + 'static + Clone,
    on_open_file: impl Fn(&mut App) + 'static + Clone,
    on_choose_folder: impl Fn(&mut App) + 'static + Clone,
    on_save: impl Fn(&mut App) + 'static + Clone,
    on_save_as: impl Fn(&mut App) + 'static + Clone,
    on_export_html: impl Fn(&mut App) + 'static + Clone,
    on_export_md: impl Fn(&mut App) + 'static + Clone,
    on_export_txt: impl Fn(&mut App) + 'static + Clone,
    on_view_pdf: impl Fn(&mut App) + 'static + Clone,
    on_close_tab: impl Fn(&mut App) + 'static + Clone,
) -> Vec<MenuItem> {
    let mut items = vec![
        MenuItem::action("New note", None, Some("Ctrl+N"), {
            let f = on_new_note.clone();
            move |_, cx| f(cx)
        }),
        MenuItem::action("New folder", None, None, {
            let f = on_new_folder.clone();
            move |_, cx| f(cx)
        }),
        MenuItem::action("Open…", None, Some("Ctrl+O"), {
            let f = on_open_file.clone();
            move |_, cx| f(cx)
        }),
        MenuItem::action("Change folder…", None, None, {
            let f = on_choose_folder.clone();
            move |_, cx| f(cx)
        }),
        MenuItem::divider(),
        MenuItem::action("Save", None, Some("Ctrl+S"), {
            let f = on_save.clone();
            move |_, cx| f(cx)
        }),
        MenuItem::action("Save as…", None, Some("Ctrl+Shift+S"), {
            let f = on_save_as.clone();
            move |_, cx| f(cx)
        }),
        MenuItem::action("Export as HTML", None, None, {
            let f = on_export_html.clone();
            move |_, cx| f(cx)
        }),
        MenuItem::action("Export as Markdown", None, None, {
            let f = on_export_md.clone();
            move |_, cx| f(cx)
        }),
        MenuItem::action("Export as Plain Text", None, None, {
            let f = on_export_txt.clone();
            move |_, cx| f(cx)
        }),
        MenuItem::disabled_action("Export as PDF", None, Some("Soon")),
    ];

    if has_pdf {
        items.push(MenuItem::action("Original PDF", None, None, {
            let f = on_view_pdf.clone();
            move |_, cx| f(cx)
        }));
    }

    items.push(MenuItem::divider());
    items.push(MenuItem::action("Close tab", None, Some("Ctrl+W"), {
        let f = on_close_tab.clone();
        move |_, cx| f(cx)
    }));

    items
}

#[allow(clippy::too_many_arguments)]
pub fn build_edit_menu_items(
    on_undo: impl Fn(&mut App) + 'static + Clone,
    on_redo: impl Fn(&mut App) + 'static + Clone,
    on_cut: impl Fn(&mut App) + 'static + Clone,
    on_copy: impl Fn(&mut App) + 'static + Clone,
    on_paste: impl Fn(&mut App) + 'static + Clone,
    on_select_all: impl Fn(&mut App) + 'static + Clone,
    on_find: impl Fn(&mut App) + 'static + Clone,
    on_replace: impl Fn(&mut App) + 'static + Clone,
) -> Vec<MenuItem> {
    vec![
        MenuItem::action("Undo", None, Some("Ctrl+Z"), {
            let f = on_undo.clone();
            move |_, cx| f(cx)
        }),
        MenuItem::action("Redo", None, Some("Ctrl+Y"), {
            let f = on_redo.clone();
            move |_, cx| f(cx)
        }),
        MenuItem::divider(),
        MenuItem::action("Cut", None, Some("Ctrl+X"), {
            let f = on_cut.clone();
            move |_, cx| f(cx)
        }),
        MenuItem::action("Copy", None, Some("Ctrl+C"), {
            let f = on_copy.clone();
            move |_, cx| f(cx)
        }),
        MenuItem::action("Paste", None, Some("Ctrl+V"), {
            let f = on_paste.clone();
            move |_, cx| f(cx)
        }),
        MenuItem::action("Select all", None, Some("Ctrl+A"), {
            let f = on_select_all.clone();
            move |_, cx| f(cx)
        }),
        MenuItem::divider(),
        MenuItem::action("Find", None, Some("Ctrl+F"), {
            let f = on_find.clone();
            move |_, cx| f(cx)
        }),
        MenuItem::action("Replace", None, Some("Ctrl+H"), {
            let f = on_replace.clone();
            move |_, cx| f(cx)
        }),
    ]
}

#[allow(clippy::too_many_arguments)]
pub fn build_view_menu_items(
    view_mode: ViewMode,
    show_line_numbers: bool,
    word_wrap: bool,
    show_sidebar: bool,
    content_alignment: ContentAlignment,
    on_set_view_mode: impl Fn(ViewMode, &mut App) + 'static + Clone,
    on_toggle_line_numbers: impl Fn(&mut App) + 'static + Clone,
    on_toggle_word_wrap: impl Fn(&mut App) + 'static + Clone,
    on_set_content_alignment: impl Fn(ContentAlignment, &mut App) + 'static + Clone,
    on_zoom_in: impl Fn(&mut App) + 'static + Clone,
    on_zoom_out: impl Fn(&mut App) + 'static + Clone,
    on_reset_zoom: impl Fn(&mut App) + 'static + Clone,
    on_toggle_sidebar: impl Fn(&mut App) + 'static + Clone,
    on_toggle_zen: impl Fn(&mut App) + 'static + Clone,
) -> Vec<MenuItem> {
    let is_wysiwyg = view_mode == ViewMode::Wysiwyg;

    vec![
        MenuItem::radio("WYSIWYG", is_wysiwyg, Some("Ctrl+Shift+M"), {
            let f = on_set_view_mode.clone();
            move |_, cx| f(ViewMode::Wysiwyg, cx)
        }),
        MenuItem::radio("Raw", !is_wysiwyg, None, {
            let f = on_set_view_mode.clone();
            move |_, cx| f(ViewMode::Raw, cx)
        }),
        MenuItem::divider(),
        if is_wysiwyg {
            MenuItem::disabled_action("Line numbers", Some("Ctrl+Shift+L"), Some("Raw only"))
        } else {
            MenuItem::check("Line numbers", show_line_numbers, Some("Ctrl+Shift+L"), {
                let f = on_toggle_line_numbers.clone();
                move |_, cx| f(cx)
            })
        },
        MenuItem::check("Word wrap", word_wrap, None, {
            let f = on_toggle_word_wrap.clone();
            move |_, cx| f(cx)
        }),
        MenuItem::divider(),
        MenuItem::radio(
            "Centered text",
            content_alignment == ContentAlignment::Centered,
            None,
            {
                let f = on_set_content_alignment.clone();
                move |_, cx| f(ContentAlignment::Centered, cx)
            },
        ),
        MenuItem::radio(
            "Left-aligned text",
            content_alignment == ContentAlignment::Left,
            None,
            {
                let f = on_set_content_alignment.clone();
                move |_, cx| f(ContentAlignment::Left, cx)
            },
        ),
        MenuItem::divider(),
        MenuItem::action("Zoom in", None, Some("Ctrl+="), {
            let f = on_zoom_in.clone();
            move |_, cx| f(cx)
        }),
        MenuItem::action("Zoom out", None, Some("Ctrl+-"), {
            let f = on_zoom_out.clone();
            move |_, cx| f(cx)
        }),
        MenuItem::action("Reset zoom", None, Some("Ctrl+0"), {
            let f = on_reset_zoom.clone();
            move |_, cx| f(cx)
        }),
        MenuItem::divider(),
        MenuItem::check("Sidebar", show_sidebar, Some("Ctrl+\\"), {
            let f = on_toggle_sidebar.clone();
            move |_, cx| f(cx)
        }),
        MenuItem::action("Zen mode", None, Some("F11"), {
            let f = on_toggle_zen.clone();
            move |_, cx| f(cx)
        }),
    ]
}
