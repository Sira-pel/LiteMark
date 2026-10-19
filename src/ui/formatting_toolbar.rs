use gpui::prelude::*;
use gpui::*;
use crate::model::ActiveFormats;
use crate::theme::{sz, Theme};
use super::primitives::{divider, icon_button};
use super::menu::{render_dropdown_menu, MenuItem};

pub struct FormattingToolbarProps<FFormat, FTable, FLink, FToggleFind, FToggleOverflow>
where
    FFormat: Fn(&'static str, &mut App) + 'static + Clone,
    FTable: Fn(&mut App) + 'static + Clone,
    FLink: Fn(&mut App) + 'static + Clone,
    FToggleFind: Fn(&mut App) + 'static + Clone,
    FToggleOverflow: Fn(&mut App) + 'static + Clone,
{
    pub active_formats: ActiveFormats,
    pub show_overflow_menu: bool,
    pub on_format: FFormat,
    pub on_insert_table: FTable,
    pub on_insert_link: FLink,
    pub on_toggle_find: FToggleFind,
    pub on_toggle_overflow: FToggleOverflow,
}

pub fn render_formatting_toolbar<FFormat, FTable, FLink, FToggleFind, FToggleOverflow>(
    props: FormattingToolbarProps<FFormat, FTable, FLink, FToggleFind, FToggleOverflow>,
    theme: &Theme,
) -> impl IntoElement
where
    FFormat: Fn(&'static str, &mut App) + 'static + Clone,
    FTable: Fn(&mut App) + 'static + Clone,
    FLink: Fn(&mut App) + 'static + Clone,
    FToggleFind: Fn(&mut App) + 'static + Clone,
    FToggleOverflow: Fn(&mut App) + 'static + Clone,
{
    let on_format = props.on_format;
    let on_table = props.on_insert_table;
    let on_link = props.on_insert_link;
    let on_find = props.on_toggle_find;
    let on_overflow = props.on_toggle_overflow;
    let formats = props.active_formats;

    div()
        .relative()
        .flex()
        .items_center()
        .justify_center()
        .gap(px(2.0))
        .h(sz::TOOLBAR)
        .px(px(8.0))
        .bg(theme.bg_toolbar)
        .border_b_1()
        .border_color(theme.border_primary)
        .flex_shrink_0()
        // 1 & 2: Undo / Redo
        .child({
            let f = on_format.clone();
            icon_button("tb-undo", "rotate-ccw", "Undo (Ctrl+Z)", false, theme, move |_, _, cx| f("undo", cx))
        })
        .child({
            let f = on_format.clone();
            icon_button("tb-redo", "rotate-cw", "Redo (Ctrl+Y)", false, theme, move |_, _, cx| f("redo", cx))
        })
        .child(divider(theme))
        // 3, 4, 5: Headings
        .child({
            let f = on_format.clone();
            icon_button("tb-h1", "heading-1", "Heading 1 (Ctrl+1)", formats.h1, theme, move |_, _, cx| f("h1", cx))
        })
        .child({
            let f = on_format.clone();
            icon_button("tb-h2", "heading-2", "Heading 2 (Ctrl+2)", formats.h2, theme, move |_, _, cx| f("h2", cx))
        })
        .child({
            let f = on_format.clone();
            icon_button("tb-h3", "heading-3", "Heading 3 (Ctrl+3)", formats.h3, theme, move |_, _, cx| f("h3", cx))
        })
        .child(divider(theme))
        // 6, 7, 8: Bold, Italic, Link
        .child({
            let f = on_format.clone();
            icon_button("tb-bold", "bold", "Bold (Ctrl+B)", formats.bold, theme, move |_, _, cx| f("bold", cx))
        })
        .child({
            let f = on_format.clone();
            icon_button("tb-italic", "italic", "Italic (Ctrl+I)", formats.italic, theme, move |_, _, cx| f("italic", cx))
        })
        .child({
            let l = on_link.clone();
            icon_button("tb-link", "link", "Insert Link (Ctrl+K)", false, theme, move |_, _, cx| l(cx))
        })
        .child(divider(theme))
        // 9, 10, 11: Bullet, Numbered, Checklist
        .child({
            let f = on_format.clone();
            icon_button("tb-ul", "list", "Bullet List", formats.ul, theme, move |_, _, cx| f("ul", cx))
        })
        .child({
            let f = on_format.clone();
            icon_button("tb-ol", "list-ordered", "Numbered List", formats.ol, theme, move |_, _, cx| f("ol", cx))
        })
        .child({
            let f = on_format.clone();
            icon_button("tb-task", "square-check", "Checklist", false, theme, move |_, _, cx| f("checklist", cx))
        })
        .child(divider(theme))
        // 12: Search
        .child({
            let s = on_find.clone();
            icon_button("tb-search", "search", "Find & Replace (Ctrl+F)", false, theme, move |_, _, cx| s(cx))
        })
        // 13: More (⋯)
        .child({
            let ov = on_overflow.clone();
            div()
                .relative()
                .child(icon_button("tb-more", "ellipsis", "More Formatting", props.show_overflow_menu, theme, move |_, _, cx| ov(cx)))
                .when(props.show_overflow_menu, |s| {
                    let f = on_format.clone();
                    let t = on_table.clone();
                    let ov_close = on_overflow.clone();

                    let items = vec![
                        MenuItem::action("Underline", Some("underline"), Some("Ctrl+U"), {
                            let f = f.clone();
                            let ov_c = ov_close.clone();
                            move |_, cx| { f("underline", cx); ov_c(cx); }
                        }),
                        MenuItem::action("Strikethrough", Some("strikethrough"), Some("Ctrl+Shift+X"), {
                            let f = f.clone();
                            let ov_c = ov_close.clone();
                            move |_, cx| { f("strike", cx); ov_c(cx); }
                        }),
                        MenuItem::divider(),
                        MenuItem::action("Quote Block", Some("quote"), None, {
                            let f = f.clone();
                            let ov_c = ov_close.clone();
                            move |_, cx| { f("quote", cx); ov_c(cx); }
                        }),
                        MenuItem::action("Inline Code", Some("code"), Some("Ctrl+E"), {
                            let f = f.clone();
                            let ov_c = ov_close.clone();
                            move |_, cx| { f("code", cx); ov_c(cx); }
                        }),
                        MenuItem::action("Code Block", Some("file-code"), Some("Ctrl+Shift+C"), {
                            let f = f.clone();
                            let ov_c = ov_close.clone();
                            move |_, cx| { f("codeblock", cx); ov_c(cx); }
                        }),
                        MenuItem::action("Horizontal Divider", Some("minus"), None, {
                            let f = f.clone();
                            let ov_c = ov_close.clone();
                            move |_, cx| { f("hr", cx); ov_c(cx); }
                        }),
                        MenuItem::action("Insert Table", Some("table"), None, {
                            let t = t.clone();
                            let ov_c = ov_close.clone();
                            move |_, cx| { t(cx); ov_c(cx); }
                        }),
                        MenuItem::divider(),
                        MenuItem::action("Clear Formatting", Some("remove-formatting"), None, {
                            let f = f.clone();
                            let ov_c = ov_close.clone();
                            move |_, cx| { f("clear", cx); ov_c(cx); }
                        }),
                    ];

                    s.child(
                        div()
                            .absolute()
                            .top(px(36.0))
                            .right_0()
                            .child(render_dropdown_menu("tb-overflow-menu", items, theme))
                    )
                })
        })
}
