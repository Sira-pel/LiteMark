use gpui::prelude::*;
use gpui::*;
use crate::model::ActiveFormats;
use crate::theme::Theme;
use super::primitives::{divider, icon_button};

pub struct FormattingToolbarProps<FFormat, FTable, FLink, FToggleFind>
where
    FFormat: Fn(&'static str, &mut App) + 'static + Clone,
    FTable: Fn(&mut App) + 'static + Clone,
    FLink: Fn(&mut App) + 'static + Clone,
    FToggleFind: Fn(&mut App) + 'static + Clone,
{
    pub active_formats: ActiveFormats,
    pub on_format: FFormat,
    pub on_insert_table: FTable,
    pub on_insert_link: FLink,
    pub on_toggle_find: FToggleFind,
}

pub fn render_formatting_toolbar<FFormat, FTable, FLink, FToggleFind>(
    props: FormattingToolbarProps<FFormat, FTable, FLink, FToggleFind>,
    theme: &Theme,
) -> impl IntoElement
where
    FFormat: Fn(&'static str, &mut App) + 'static + Clone,
    FTable: Fn(&mut App) + 'static + Clone,
    FLink: Fn(&mut App) + 'static + Clone,
    FToggleFind: Fn(&mut App) + 'static + Clone,
{
    let on_format = props.on_format;
    let on_table = props.on_insert_table;
    let on_link = props.on_insert_link;
    let on_find = props.on_toggle_find;
    let formats = props.active_formats;

    div()
        .flex()
        .items_center()
        .justify_center()
        .gap(px(2.0))
        .px(px(8.0))
        .py(px(4.0))
        .bg(theme.bg_toolbar)
        .border_b_1()
        .border_color(theme.border_primary)
        .overflow_x_hidden()
        .flex_shrink_0()
        // Undo / Redo
        .child({
            let f = on_format.clone();
            icon_button("tb-undo", "rotate-ccw", "Undo (Ctrl+Z)", false, theme, move |_, _, cx| f("undo", cx))
        })
        .child({
            let f = on_format.clone();
            icon_button("tb-redo", "rotate-cw", "Redo (Ctrl+Y)", false, theme, move |_, _, cx| f("redo", cx))
        })
        .child(divider(theme))
        // Headings
        .child({
            let f = on_format.clone();
            icon_button("tb-h1", "heading-1", "Heading 1", formats.h1, theme, move |_, _, cx| f("h1", cx))
        })
        .child({
            let f = on_format.clone();
            icon_button("tb-h2", "heading-2", "Heading 2", formats.h2, theme, move |_, _, cx| f("h2", cx))
        })
        .child({
            let f = on_format.clone();
            icon_button("tb-h3", "heading-3", "Heading 3", formats.h3, theme, move |_, _, cx| f("h3", cx))
        })
        .child(divider(theme))
        // Basic Formatting
        .child({
            let f = on_format.clone();
            icon_button("tb-bold", "bold", "Bold (Ctrl+B)", formats.bold, theme, move |_, _, cx| f("bold", cx))
        })
        .child({
            let f = on_format.clone();
            icon_button("tb-italic", "italic", "Italic (Ctrl+I)", formats.italic, theme, move |_, _, cx| f("italic", cx))
        })
        .child({
            let f = on_format.clone();
            icon_button("tb-underline", "underline", "Underline (Ctrl+U)", formats.underline, theme, move |_, _, cx| f("underline", cx))
        })
        .child({
            let f = on_format.clone();
            icon_button("tb-strike", "strikethrough", "Strikethrough", formats.strike, theme, move |_, _, cx| f("strike", cx))
        })
        .child(divider(theme))
        // Lists & Quotes
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
            icon_button("tb-task", "square-check", "Checklist / Task List", false, theme, move |_, _, cx| f("checklist", cx))
        })
        .child({
            let f = on_format.clone();
            icon_button("tb-quote", "quote", "Quote Block", formats.quote, theme, move |_, _, cx| f("quote", cx))
        })
        .child(divider(theme))
        // Code & Rule
        .child({
            let f = on_format.clone();
            icon_button("tb-code", "code", "Inline Code", formats.code, theme, move |_, _, cx| f("code", cx))
        })
        .child({
            let f = on_format.clone();
            icon_button("tb-codeblock", "file-code", "Code Block", false, theme, move |_, _, cx| f("codeblock", cx))
        })
        .child({
            let f = on_format.clone();
            icon_button("tb-hr", "minus", "Horizontal Divider", false, theme, move |_, _, cx| f("hr", cx))
        })
        .child(divider(theme))
        // Inserts & Clear
        .child({
            let t = on_table.clone();
            icon_button("tb-table", "table", "Insert Markdown Table", false, theme, move |_, _, cx| t(cx))
        })
        .child({
            let l = on_link.clone();
            icon_button("tb-link", "link", "Insert Hyperlink", false, theme, move |_, _, cx| l(cx))
        })
        .child({
            let f = on_format.clone();
            icon_button("tb-clear", "remove-formatting", "Clear Formatting", false, theme, move |_, _, cx| f("clear", cx))
        })
        .child(divider(theme))
        // Search trigger
        .child({
            let s = on_find.clone();
            icon_button("tb-search", "search", "Find & Replace (Ctrl+F)", false, theme, move |_, _, cx| s(cx))
        })
}
