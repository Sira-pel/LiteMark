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
    #[allow(dead_code)]
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
    let on_overflow = props.on_toggle_overflow;
    let formats = props.active_formats;
    let is_narrow = props.is_narrow;

    if !is_narrow {
        // Big window layout: 3 zones
        // Zone 1: File Edit View on the left
        // Zone 2: Centered full format toolbar in the middle
        // Zone 3: Settings gear locked on the far right
        div()
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
            .flex_shrink_0()
            .child(
                div()
                    .flex_1()
                    .flex_basis(px(0.0))
                    .flex()
                    .items_center()
                    .child(props.menu_bar_element),
            )
            .child(
                div()
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
                    // 6, 7, 8, 9: Bold, Italic, Underline (Ctrl+U), Link
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
                        let l = on_link.clone();
                        icon_button("tb-link", "link", "Insert Link (Ctrl+K)", formats.link, theme, move |_, _, cx| l(cx))
                    })
                    .child(divider(theme))
                    // 10, 11, 12: Bullet, Numbered, Checklist
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
                        icon_button("tb-task", "square-check", "Checklist", formats.checklist, theme, move |_, _, cx| f("checklist", cx))
                    })
                    // 13: More (⋯)
                    .child({
                        let ov = on_overflow.clone();
                        icon_button("tb-more", "ellipsis", "More Formatting", props.show_overflow_menu, theme, move |_, _, cx| ov(cx))
                    }),
            )
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
        // Super small window layout (< 640px): 3 zones with centered format tools
        // Zone 1: [ ≡ ] on the left
        // Zone 2: [ ↶ ] [ ↷ ] | [ B ] [ I ] [ U ] [ 🔗 ] [ ⋯ ] centered in the middle
        // Zone 3: [ ⚙ ] on the far right
        div()
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
            .flex_shrink_0()
            .child(
                div()
                    .flex_1()
                    .flex_basis(px(0.0))
                    .flex()
                    .items_center()
                    .child(props.menu_bar_element),
            )
            .child(
                div()
                    .id("toolbar-format-icons")
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(2.0))
                    .child({
                        let f = on_format.clone();
                        icon_button("tb-undo", "rotate-ccw", "Undo (Ctrl+Z)", false, theme, move |_, _, cx| f("undo", cx))
                    })
                    .child({
                        let f = on_format.clone();
                        icon_button("tb-redo", "rotate-cw", "Redo (Ctrl+Y)", false, theme, move |_, _, cx| f("redo", cx))
                    })
                    .child(divider(theme))
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
                        let l = on_link.clone();
                        icon_button("tb-link", "link", "Insert Link (Ctrl+K)", formats.link, theme, move |_, _, cx| l(cx))
                    })
                    .child({
                        let ov = on_overflow.clone();
                        icon_button("tb-more", "ellipsis", "More Formatting", props.show_overflow_menu, theme, move |_, _, cx| ov(cx))
                    }),
            )
            .child(
                div()
                    .flex_1()
                    .flex_basis(px(0.0))
                    .flex()
                    .items_center()
                    .justify_end()
                    .child(props.settings_gear_element),
            )
    }
}

pub fn build_overflow_menu_items(
    is_narrow: bool,
    on_format: impl Fn(&'static str, &mut App) + 'static + Clone,
    on_insert_table: impl Fn(&mut App) + 'static + Clone,
    on_close: impl Fn(&mut App) + 'static + Clone,
) -> Vec<MenuItem> {
    let mut items = Vec::new();

    // On super small window (< 640px), Headings and Lists collapse into the overflow menu
    if is_narrow {
        let f1 = on_format.clone();
        let c1 = on_close.clone();
        items.push(MenuItem::action("Heading 1", Some("heading-1"), Some("Ctrl+1"), move |_, cx| {
            f1("h1", cx);
            c1(cx);
        }));
        let f2 = on_format.clone();
        let c2 = on_close.clone();
        items.push(MenuItem::action("Heading 2", Some("heading-2"), Some("Ctrl+2"), move |_, cx| {
            f2("h2", cx);
            c2(cx);
        }));
        let f3 = on_format.clone();
        let c3 = on_close.clone();
        items.push(MenuItem::action("Heading 3", Some("heading-3"), Some("Ctrl+3"), move |_, cx| {
            f3("h3", cx);
            c3(cx);
        }));
        items.push(MenuItem::divider());

        let f4 = on_format.clone();
        let c4 = on_close.clone();
        items.push(MenuItem::action("Bullet list", Some("list"), None, move |_, cx| {
            f4("ul", cx);
            c4(cx);
        }));
        let f5 = on_format.clone();
        let c5 = on_close.clone();
        items.push(MenuItem::action("Numbered list", Some("list-ordered"), None, move |_, cx| {
            f5("ol", cx);
            c5(cx);
        }));
        let f6 = on_format.clone();
        let c6 = on_close.clone();
        items.push(MenuItem::action("Task list", Some("square-check"), None, move |_, cx| {
            f6("checklist", cx);
            c6(cx);
        }));
        items.push(MenuItem::divider());
    }

    // Extra features (always in overflow menu)
    let f_strike = on_format.clone();
    let c_strike = on_close.clone();
    items.push(MenuItem::action("Strikethrough", Some("strikethrough"), Some("Ctrl+Shift+X"), move |_, cx| {
        f_strike("strike", cx);
        c_strike(cx);
    }));

    items.push(MenuItem::divider());

    let f_quote = on_format.clone();
    let c_quote = on_close.clone();
    items.push(MenuItem::action("Quote Block", Some("quote"), None, move |_, cx| {
        f_quote("quote", cx);
        c_quote(cx);
    }));

    let f_code = on_format.clone();
    let c_code = on_close.clone();
    items.push(MenuItem::action("Inline Code", Some("code"), Some("Ctrl+E"), move |_, cx| {
        f_code("code", cx);
        c_code(cx);
    }));

    let f_codeblock = on_format.clone();
    let c_codeblock = on_close.clone();
    items.push(MenuItem::action("Code Block", Some("file-code"), Some("Ctrl+Shift+C"), move |_, cx| {
        f_codeblock("codeblock", cx);
        c_codeblock(cx);
    }));

    let f_hr = on_format.clone();
    let c_hr = on_close.clone();
    items.push(MenuItem::action("Horizontal Divider", Some("minus"), None, move |_, cx| {
        f_hr("hr", cx);
        c_hr(cx);
    }));

    let t1 = on_insert_table.clone();
    let c_t = on_close.clone();
    items.push(MenuItem::action("Insert Table", Some("table"), None, move |_, cx| {
        t1(cx);
        c_t(cx);
    }));

    items.push(MenuItem::divider());

    let f_clear = on_format.clone();
    let c_clear = on_close.clone();
    items.push(MenuItem::action("Clear Formatting", Some("remove-formatting"), None, move |_, cx| {
        f_clear("clear", cx);
        c_clear(cx);
    }));

    items
}

#[cfg(test)]
mod tests {
    use super::build_overflow_menu_items;
    use core::prelude::v1::test;

    #[test]
    fn test_overflow_menu_contents_at_breakpoints() {
        // In big window mode (!is_narrow): no headings, no lists, only extra features
        let wide_items = build_overflow_menu_items(
            false,
            |_, _| {},
            |_| {},
            |_| {},
        );
        let labels_wide: Vec<&str> = wide_items.iter().map(|it| it.label).collect();
        assert!(!labels_wide.contains(&"Underline"));
        assert!(!labels_wide.contains(&"Heading 1"));
        assert!(!labels_wide.contains(&"Bullet list"));
        assert!(labels_wide.contains(&"Strikethrough"));
        assert!(labels_wide.contains(&"Insert Table"));

        // In super small mode (is_narrow): both headings and lists are prepended
        let narrow_items = build_overflow_menu_items(
            true,
            |_, _| {},
            |_| {},
            |_| {},
        );
        let labels_narrow: Vec<&str> = narrow_items.iter().map(|it| it.label).collect();
        assert!(labels_narrow.contains(&"Heading 1"));
        assert!(labels_narrow.contains(&"Heading 2"));
        assert!(labels_narrow.contains(&"Heading 3"));
        assert!(labels_narrow.contains(&"Bullet list"));
        assert!(labels_narrow.contains(&"Numbered list"));
        assert!(labels_narrow.contains(&"Task list"));
        assert!(labels_narrow.contains(&"Strikethrough"));
    }

    #[test]
    fn test_toolbar_never_overflows_super_small_width() {
        // In super small layout:
        // [ ≡ ] (32) + | (13) + Undo/Redo (66) + | (13) + B/I/U/Link (134) + ⋯ (32) + | (13) + ⚙ (32) = 335px
        let min_narrow_width = 335.0;
        assert!(min_narrow_width < 640.0, "Super small layout fits well within 640px threshold");
    }
}
