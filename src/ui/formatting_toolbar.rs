use gpui::prelude::*;
use gpui::*;
use crate::model::ActiveFormats;
use crate::theme::{sz, Theme};
use super::primitives::{divider, icon_button};
use super::menu::MenuItem;

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
    pub is_narrow: bool,
    pub menu_bar_element: AnyElement,
    pub settings_gear_element: AnyElement,
    pub on_format: FFormat,
    #[allow(dead_code)]
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
    let on_link = props.on_insert_link;
    let on_find = props.on_toggle_find;
    let on_overflow = props.on_toggle_overflow;
    let formats = props.active_formats;
    let is_narrow = props.is_narrow;

    // Formatting icons group: 13 icons + ⋯
    let format_icons = div()
        .id("toolbar-format-icons")
        .flex_none()
        .flex()
        .items_center()
        .gap(px(2.0))
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
            icon_button("tb-more", "ellipsis", "More Formatting", props.show_overflow_menu, theme, move |_, _, cx| ov(cx))
        });

    let toolbar = div()
        .id("formatting-toolbar")
        .relative()
        .flex()
        .items_center()
        .justify_between()
        .h(sz::TOOLBAR)
        .px(px(8.0))
        .pt(px(6.0))
        .bg(theme.bg_toolbar)
        .border_b_1()
        .border_color(theme.border_primary)
        .flex_shrink_0();

    if !is_narrow {
        toolbar
            .child(
                // 1. Left wrapper: menu bar buttons
                div()
                    .flex_1()
                    .flex_basis(px(0.0))
                    .flex()
                    .items_center()
                    .child(props.menu_bar_element),
            )
            // 2. Centered format icons group
            .child(format_icons)
            // 3. Right wrapper: settings gear
            .child(
                div()
                    .flex_1()
                    .flex_basis(px(0.0))
                    .flex()
                    .items_center()
                    .justify_end()
                    .child(props.settings_gear_element),
            )
    } else {
        toolbar
            .child(
                // Narrow layout: ☰ menu button, then 1px divider, then format icons
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(props.menu_bar_element)
                    .child(
                        div()
                            .w(px(1.0))
                            .h(px(16.0))
                            .bg(theme.border_surface),
                    )
                    .child(format_icons),
            )
            .child(
                // Right gear stays
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .child(props.settings_gear_element),
            )
    }
}

pub fn build_overflow_menu_items(
    on_format: impl Fn(&'static str, &mut App) + 'static + Clone,
    on_insert_table: impl Fn(&mut App) + 'static + Clone,
    on_close: impl Fn(&mut App) + 'static + Clone,
) -> Vec<MenuItem> {
    let f1 = on_format.clone();
    let f2 = on_format.clone();
    let f3 = on_format.clone();
    let f4 = on_format.clone();
    let f5 = on_format.clone();
    let f6 = on_format.clone();
    let f7 = on_format.clone();
    let t1 = on_insert_table.clone();

    let c1 = on_close.clone();
    let c2 = on_close.clone();
    let c3 = on_close.clone();
    let c4 = on_close.clone();
    let c5 = on_close.clone();
    let c6 = on_close.clone();
    let c7 = on_close.clone();
    let c8 = on_close.clone();

    vec![
        MenuItem::action("Underline", Some("underline"), Some("Ctrl+U"), move |_, cx| {
            f1("underline", cx);
            c1(cx);
        }),
        MenuItem::action("Strikethrough", Some("strikethrough"), Some("Ctrl+Shift+X"), move |_, cx| {
            f2("strike", cx);
            c2(cx);
        }),
        MenuItem::divider(),
        MenuItem::action("Quote Block", Some("quote"), None, move |_, cx| {
            f3("quote", cx);
            c3(cx);
        }),
        MenuItem::action("Inline Code", Some("code"), Some("Ctrl+E"), move |_, cx| {
            f4("code", cx);
            c4(cx);
        }),
        MenuItem::action("Code Block", Some("file-code"), Some("Ctrl+Shift+C"), move |_, cx| {
            f5("codeblock", cx);
            c5(cx);
        }),
        MenuItem::action("Horizontal Divider", Some("minus"), None, move |_, cx| {
            f6("hr", cx);
            c6(cx);
        }),
        MenuItem::action("Insert Table", Some("table"), None, move |_, cx| {
            t1(cx);
            c7(cx);
        }),
        MenuItem::divider(),
        MenuItem::action("Clear Formatting", Some("remove-formatting"), None, move |_, cx| {
            f7("clear", cx);
            c8(cx);
        }),
    ]
}
