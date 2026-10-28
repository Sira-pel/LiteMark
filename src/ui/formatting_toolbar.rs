use gpui::prelude::*;
use gpui::*;
use crate::model::ActiveFormats;
use crate::theme::{sz, Theme};
use super::primitives::{divider, icon_button, render_icon, DIVIDER_TOTAL_W};
use super::menu::MenuItem;

pub struct FormattingToolbarProps<FFormat, FTable, FLink, FToggleFind, FToggleOverflow, FToggleHeading>
where
    FFormat: Fn(&'static str, &mut App) + 'static + Clone,
    FTable: Fn(&mut App) + 'static + Clone,
    FLink: Fn(&mut App) + 'static + Clone,
    FToggleFind: Fn(&mut App) + 'static + Clone,
    FToggleOverflow: Fn(&mut App) + 'static + Clone,
    FToggleHeading: Fn(&mut App) + 'static + Clone,
{
    pub active_formats: ActiveFormats,
    pub show_overflow_menu: bool,
    pub show_table_menu: bool,
    pub show_heading_menu: bool,
    pub is_narrow: bool,
    pub menu_bar_element: AnyElement,
    pub settings_gear_element: AnyElement,
    pub on_format: FFormat,
    pub on_insert_table: FTable,
    pub on_insert_link: FLink,
    pub on_toggle_find: FToggleFind,
    pub on_toggle_overflow: FToggleOverflow,
    pub on_toggle_heading: FToggleHeading,
}

pub fn render_formatting_toolbar<
    FFormat,
    FTable,
    FLink,
    FToggleFind,
    FToggleOverflow,
    FToggleHeading,
>(
    props: FormattingToolbarProps<FFormat, FTable, FLink, FToggleFind, FToggleOverflow, FToggleHeading>,
    theme: &Theme,
) -> impl IntoElement
where
    FFormat: Fn(&'static str, &mut App) + 'static + Clone,
    FTable: Fn(&mut App) + 'static + Clone,
    FLink: Fn(&mut App) + 'static + Clone,
    FToggleFind: Fn(&mut App) + 'static + Clone,
    FToggleOverflow: Fn(&mut App) + 'static + Clone,
    FToggleHeading: Fn(&mut App) + 'static + Clone,
{
    let on_format = props.on_format;
    let on_table = props.on_insert_table;
    let on_link = props.on_insert_link;
    let on_find = props.on_toggle_find;
    let on_overflow = props.on_toggle_overflow;
    let on_heading = props.on_toggle_heading;
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
            .px(px(TOOLBAR_PX_PAD))
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
                    .gap(px(TOOLBAR_ICON_GAP))
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
                    // 3: Headings dropdown (H1 / H2 / H3)
                    .child({
                        let h = on_heading.clone();
                        heading_button(theme, formats, props.show_heading_menu, move |cx| h(cx))
                    })
                    .child(divider(theme))
                    // 6, 7, 8, 9, 10: Bold, Italic, Underline (Ctrl+U), Link, Search
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
                        let s = on_find.clone();
                        icon_button("tb-search", "search", "Find & Replace (Ctrl+F)", false, theme, move |_, _, cx| s(cx))
                    })
                    .child(divider(theme))
                    // 11, 12, 13, 14: Bullet, Numbered, Checklist, Table
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
                    .child({
                        let t = on_table.clone();
                        icon_button("tb-table", "table", "Table", props.show_table_menu || formats.table, theme, move |_, _, cx| t(cx))
                    })
                    // 15: More (⋯)
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
            .px(px(TOOLBAR_PX_PAD))
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
                    .gap(px(TOOLBAR_ICON_GAP))
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

/// Width of the headings dropdown button: an icon plus a small chevron.
pub const HEADING_BTN_W: f32 = 44.0;

/// Headings dropdown button. It shows the active level (H1-H3) when the caret
/// is in a heading and a plain "H" otherwise, with a chevron that says it
/// opens a menu.
fn heading_button(
    theme: &Theme,
    formats: ActiveFormats,
    is_open: bool,
    on_toggle: impl Fn(&mut App) + 'static,
) -> impl IntoElement {
    let active = formats.h1 || formats.h2 || formats.h3;
    let icon = if formats.h1 {
        "heading-1"
    } else if formats.h2 {
        "heading-2"
    } else if formats.h3 {
        "heading-3"
    } else {
        "heading"
    };
    let color = if active || is_open {
        theme.text_strong
    } else {
        theme.text_muted
    };
    let th = *theme;
    div()
        .id("tb-heading")
        .flex()
        .items_center()
        .justify_center()
        .gap(px(2.0))
        .w(px(HEADING_BTN_W))
        .h(sz::BTN)
        .rounded(sz::R_MD)
        .cursor_pointer()
        .bg(if active || is_open {
            theme.row_active
        } else {
            rgba(0x00000000)
        })
        .when(!active && !is_open, |s| {
            s.hover(|h| h.bg(theme.bg_hover).text_color(theme.text_primary))
                .active(|a| a.bg(theme.bg_active))
        })
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(move |_, _, cx| on_toggle(cx))
        .tooltip(move |_, cx| {
            cx.new(|_| super::primitives::SimpleTooltip::new("Headings", th)).into()
        })
        .child(render_icon(icon, 16.0, color))
        .child(render_icon("chevron-down", 10.0, color))
}

/// Left edge of the headings button for a window `viewport_w` wide. The
/// centered button group starts `wide_center_width() / 2` left of the middle;
/// undo, redo and a divider come first.
pub fn heading_button_left(viewport_w: f32) -> f32 {
    let btn_w = f32::from(sz::BTN);
    viewport_w / 2.0 - wide_center_width() / 2.0
        + (btn_w * 2.0)
        + (TOOLBAR_ICON_GAP * 3.0)
        + DIVIDER_TOTAL_W
}

/// Width of the centered button group in the wide (>= 640px) layout.
pub fn wide_center_width() -> f32 {
    let btn_w = f32::from(sz::BTN);
    // Undo, Redo, Bold, Italic, Underline, Link, Search, Bullet, Numbered,
    // Checklist, Table, More, plus the headings dropdown.
    let plain_buttons = 12.0;
    let dividers = 3.0;
    let items = plain_buttons + 1.0 + dividers;
    (plain_buttons * btn_w) + HEADING_BTN_W + (dividers * DIVIDER_TOTAL_W)
        + ((items - 1.0) * TOOLBAR_ICON_GAP)
}

/// Entries of the headings dropdown.
pub fn build_heading_menu_items(
    on_format: impl Fn(&'static str, &mut App) + 'static + Clone,
    on_close: impl Fn(&mut App) + 'static + Clone,
) -> Vec<MenuItem> {
    [
        ("Heading 1", "heading-1", "Ctrl+1", "h1"),
        ("Heading 2", "heading-2", "Ctrl+2", "h2"),
        ("Heading 3", "heading-3", "Ctrl+3", "h3"),
    ]
    .into_iter()
    .map(|(label, icon, shortcut, cmd)| {
        let f = on_format.clone();
        let c = on_close.clone();
        MenuItem::action(label, Some(icon), Some(shortcut), move |_, cx| {
            f(cmd, cx);
            c(cx);
        })
    })
    .collect()
}

pub const TOOLBAR_PX_PAD: f32 = 8.0;
pub const TOOLBAR_ICON_GAP: f32 = 2.0;

#[allow(dead_code)]
pub fn calculate_narrow_toolbar_min_width() -> f32 {
    let btn_w = f32::from(sz::BTN);
    // Left zone: hamburger menu button
    let left_w = btn_w;
    // Right zone: settings gear button
    let right_w = btn_w;
    // Center zone format icons:
    // 7 buttons: Undo, Redo, Bold, Italic, Underline, Link, More (⋯)
    // 1 divider
    let center_buttons_count = 7.0;
    let center_dividers_count = 1.0;
    let total_center_items = center_buttons_count + center_dividers_count;
    let gaps = (total_center_items - 1.0) * TOOLBAR_ICON_GAP;
    let center_w = (center_buttons_count * btn_w) + (center_dividers_count * DIVIDER_TOTAL_W) + gaps;

    // Outer container padding (left + right)
    let container_pad = TOOLBAR_PX_PAD * 2.0;

    left_w + center_w + right_w + container_pad
}

#[allow(dead_code)]
pub fn calculate_wide_toolbar_min_width() -> f32 {
    let btn_w = f32::from(sz::BTN);
    // Menu bar element (File, Edit, View text buttons ~ 140px minimum)
    let menu_bar_w = 140.0;
    // Right zone: settings gear button
    let right_w = btn_w;
    // Center zone format icons:
    let center_w = wide_center_width();

    let container_pad = TOOLBAR_PX_PAD * 2.0;

    menu_bar_w + center_w + right_w + container_pad
}

pub fn build_overflow_menu_items(
    is_narrow: bool,
    on_format: impl Fn(&'static str, &mut App) + 'static + Clone,
    on_insert_table: impl Fn(&mut App) + 'static + Clone,
    on_toggle_find: impl Fn(&mut App) + 'static + Clone,
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
        let f_find = on_toggle_find.clone();
        let c_find = on_close.clone();
        items.push(MenuItem::action("Find & Replace", Some("search"), Some("Ctrl+F"), move |_, cx| {
            f_find(cx);
            c_find(cx);
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
        c_t(cx);
        t1(cx);
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
    use super::{
        build_overflow_menu_items, calculate_narrow_toolbar_min_width,
        calculate_wide_toolbar_min_width,
    };
    use crate::theme::sz;

    #[test]
    fn test_overflow_menu_contents_at_breakpoints() {
        // In big window mode (!is_narrow): no headings, no lists, no Find (Find is already on wide toolbar)
        let wide_items = build_overflow_menu_items(
            false,
            |_, _| {},
            |_| {},
            |_| {},
            |_| {},
        );
        let labels_wide: Vec<&str> = wide_items.iter().map(|it| it.label).collect();
        assert!(!labels_wide.contains(&"Underline"));
        assert!(!labels_wide.contains(&"Heading 1"));
        assert!(!labels_wide.contains(&"Bullet list"));
        assert!(!labels_wide.contains(&"Find & Replace"), "Find must not be duplicated on wide windows");
        assert!(labels_wide.contains(&"Strikethrough"));
        assert!(labels_wide.contains(&"Insert Table"));

        // In super small mode (is_narrow): headings, lists, and Find are prepended
        let narrow_items = build_overflow_menu_items(
            true,
            |_, _| {},
            |_| {},
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
        assert!(labels_narrow.contains(&"Find & Replace"), "Find must be in overflow menu on narrow windows");
        assert!(labels_narrow.contains(&"Strikethrough"));
    }

    #[test]
    fn test_toolbar_narrow_fits_small_screens() {
        let narrow_min_w = calculate_narrow_toolbar_min_width();
        let btn_w = f32::from(sz::BTN);

        // Narrow toolbar must fit within standard small viewport (360px)
        assert!(
            narrow_min_w <= 360.0,
            "Narrow toolbar width ({narrow_min_w}px) must fit within 360px viewport"
        );
        // It must account for at least 9 buttons (32px each = 288px) plus padding
        assert!(
            narrow_min_w >= 9.0 * btn_w,
            "Narrow toolbar width ({narrow_min_w}px) must be at least 9 button widths ({})",
            9.0 * btn_w
        );
        // Must genuinely require > 320px, so it would overflow an ultra-narrow 320px viewport
        assert!(
            narrow_min_w > 320.0,
            "Narrow toolbar requires > 320px to display all 9 buttons and dividers"
        );
        assert!(
            narrow_min_w < 640.0,
            "Narrow toolbar width ({narrow_min_w}px) must be well under the 640px breakpoint"
        );
    }

    #[test]
    fn test_toolbar_wide_exceeds_narrow_and_has_all_elements() {
        let wide_min_w = calculate_wide_toolbar_min_width();
        let narrow_min_w = calculate_narrow_toolbar_min_width();
        let btn_w = f32::from(sz::BTN);

        // Wide toolbar has 16 buttons (15 format + 1 settings) + menu bar (~140px)
        assert!(
            wide_min_w > narrow_min_w,
            "Wide toolbar ({wide_min_w}px) must be larger than narrow toolbar ({narrow_min_w}px)"
        );
        assert!(
            wide_min_w >= (16.0 * btn_w) + 140.0,
            "Wide toolbar must account for 16 buttons and menu bar"
        );
    }
}
