use gpui::prelude::*;
use gpui::*;
use crate::theme::{sz, Theme};
use super::primitives::kbd_badge;

pub struct MenuItem {
    pub label: &'static str,
    pub icon: Option<&'static str>,
    pub shortcut: Option<&'static str>,
    pub is_divider: bool,
    pub action: Box<dyn Fn(&mut Window, &mut App) + 'static>,
}

impl MenuItem {
    pub fn action(
        label: &'static str,
        icon: Option<&'static str>,
        shortcut: Option<&'static str>,
        action: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            label,
            icon,
            shortcut,
            is_divider: false,
            action: Box::new(action),
        }
    }

    pub fn divider() -> Self {
        Self {
            label: "",
            icon: None,
            shortcut: None,
            is_divider: true,
            action: Box::new(|_, _| {}),
        }
    }
}

pub fn render_dropdown_menu(
    id: impl Into<ElementId>,
    items: Vec<MenuItem>,
    theme: &Theme,
) -> impl IntoElement {
    div()
        .id(id)
        .flex()
        .flex_col()
        .w(px(232.0))
        .p(px(4.0))
        .rounded(sz::R_LG)
        .bg(theme.menu_bg)
        .border_1()
        .border_color(theme.border_surface)
        .shadow_2xl()
        .children(items.into_iter().enumerate().map(|(ix, item)| {
            if item.is_divider {
                div()
                    .my(px(4.0))
                    .mx(px(6.0))
                    .h(px(1.0))
                    .bg(theme.border_subtle)
                    .into_any_element()
            } else {
                let act = item.action;
                div()
                    .id(("menu-item", ix))
                    .flex()
                    .items_center()
                    .justify_between()
                    .h(sz::ROW_H)
                    .px(px(10.0))
                    .rounded(px(5.0))
                    .cursor_pointer()
                    .text_size(px(13.0))
                    .text_color(theme.text_primary)
                    .hover(|h| h.bg(theme.border_subtle).text_color(theme.text_strong))
                    .on_click(move |_, window, cx| {
                        (act)(window, cx);
                    })
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .when_some(item.icon, |s, icon| {
                                s.child(super::primitives::render_icon(icon, 14.0, theme.text_muted))
                            })
                            .child(item.label)
                    )
                    .when_some(item.shortcut, |s, sc| {
                        s.child(kbd_badge(sc, theme))
                    })
                    .into_any_element()
            }
        }))
}
