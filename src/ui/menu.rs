use gpui::prelude::*;
use gpui::*;
use std::sync::Arc;
use crate::theme::{sz, Theme};
use super::primitives::render_icon;

#[derive(Clone)]
pub enum MenuItemKind {
    Action,
    Check(bool),
    Radio(bool),
    Submenu(Vec<MenuItem>),
    Heading,
    Divider,
}

#[allow(clippy::type_complexity)]
pub struct MenuItem {
    pub label: &'static str,
    pub icon: Option<&'static str>,
    pub shortcut: Option<&'static str>,
    pub kind: MenuItemKind,
    pub disabled: bool,
    pub hint: Option<&'static str>,
    pub action: Arc<dyn Fn(&mut Window, &mut App) + 'static>,
}

impl Clone for MenuItem {
    fn clone(&self) -> Self {
        Self {
            label: self.label,
            icon: self.icon,
            shortcut: self.shortcut,
            kind: self.kind.clone(),
            disabled: self.disabled,
            hint: self.hint,
            action: self.action.clone(),
        }
    }
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
            kind: MenuItemKind::Action,
            disabled: false,
            hint: None,
            action: Arc::new(action),
        }
    }

    pub fn disabled_action(
        label: &'static str,
        shortcut: Option<&'static str>,
        hint: Option<&'static str>,
    ) -> Self {
        Self {
            label,
            icon: None,
            shortcut,
            kind: MenuItemKind::Action,
            disabled: true,
            hint,
            action: Arc::new(|_, _| {}),
        }
    }

    pub fn check(
        label: &'static str,
        checked: bool,
        shortcut: Option<&'static str>,
        action: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            label,
            icon: None,
            shortcut,
            kind: MenuItemKind::Check(checked),
            disabled: false,
            hint: None,
            action: Arc::new(action),
        }
    }

    pub fn radio(
        label: &'static str,
        selected: bool,
        shortcut: Option<&'static str>,
        action: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            label,
            icon: None,
            shortcut,
            kind: MenuItemKind::Radio(selected),
            disabled: false,
            hint: None,
            action: Arc::new(action),
        }
    }

    pub fn submenu(
        label: &'static str,
        icon: Option<&'static str>,
        items: Vec<MenuItem>,
    ) -> Self {
        Self {
            label,
            icon,
            shortcut: None,
            kind: MenuItemKind::Submenu(items),
            disabled: false,
            hint: None,
            action: Arc::new(|_, _| {}),
        }
    }

    pub fn heading(label: &'static str) -> Self {
        Self {
            label,
            icon: None,
            shortcut: None,
            kind: MenuItemKind::Heading,
            disabled: false,
            hint: None,
            action: Arc::new(|_, _| {}),
        }
    }

    pub fn divider() -> Self {
        Self {
            label: "",
            icon: None,
            shortcut: None,
            kind: MenuItemKind::Divider,
            disabled: false,
            hint: None,
            action: Arc::new(|_, _| {}),
        }
    }
}

pub const MENU_PANEL_PAD: Pixels = px(4.0);
pub const MENU_DIVIDER_H: Pixels = px(9.0);
pub const MENU_HEADING_H: Pixels = px(24.0);
pub const MENU_ITEM_H: Pixels = sz::ROW_H;
pub const SUBMENU_GAP: Pixels = px(4.0);

pub fn menu_item_height(kind: &MenuItemKind) -> Pixels {
    match kind {
        MenuItemKind::Divider => MENU_DIVIDER_H,
        MenuItemKind::Heading => MENU_HEADING_H,
        _ => MENU_ITEM_H,
    }
}

pub fn calculate_submenu_y_offset(items: &[MenuItem], sub_ix: usize) -> Pixels {
    let mut y_offset = MENU_PANEL_PAD;
    for prev in items.iter().take(sub_ix) {
        y_offset += menu_item_height(&prev.kind);
    }
    y_offset
}

pub fn calculate_submenu_width(item: &MenuItem) -> Pixels {
    if let MenuItemKind::Submenu(ref sub_items) = item.kind {
        let max_label_len = sub_items.iter().map(|it| it.label.len()).max().unwrap_or(0);
        let has_hints_or_shortcuts = sub_items.iter().any(|it| it.hint.is_some() || it.shortcut.is_some());
        let has_icons_or_checks = sub_items.iter().any(|it| it.icon.is_some() || matches!(it.kind, MenuItemKind::Check(_) | MenuItemKind::Radio(_)));

        let padding = px(24.0);
        let icon_col = if has_icons_or_checks { px(24.0) } else { px(0.0) };
        let text_w = px(max_label_len as f32 * 7.5);
        let extra_col = if has_hints_or_shortcuts { px(70.0) } else { px(0.0) };

        let calculated = padding + icon_col + text_w + extra_col;
        calculated.max(px(160.0)).min(px(320.0))
    } else {
        px(200.0)
    }
}

pub struct MenuProps<FClose, FSubmenu>
where
    FClose: Fn(&mut App) + 'static + Clone,
    FSubmenu: Fn(Option<usize>, &mut App) + 'static + Clone,
{
    pub id: ElementId,
    pub point: Point<Pixels>,
    pub anchor_corner: Corner,
    pub width: Pixels,
    pub items: Vec<MenuItem>,
    pub focus_handle: FocusHandle,
    pub selected_index: Option<usize>,
    pub open_submenu_ix: Option<usize>,
    pub on_close: FClose,
    pub on_set_submenu: FSubmenu,
}

#[allow(clippy::too_many_arguments)]
pub fn render_menu_panel<FClose, FSubmenu>(
    id: ElementId,
    width: Pixels,
    items: Vec<MenuItem>,
    selected_index: Option<usize>,
    open_submenu_ix: Option<usize>,
    on_close: FClose,
    on_set_submenu: FSubmenu,
    theme: &Theme,
) -> AnyElement
where
    FClose: Fn(&mut App) + 'static + Clone,
    FSubmenu: Fn(Option<usize>, &mut App) + 'static + Clone,
{
    let has_checks = items.iter().any(|it| matches!(it.kind, MenuItemKind::Check(_) | MenuItemKind::Radio(_)));

    let rows: Vec<AnyElement> = items.into_iter().enumerate().map(|(ix, item)| {
            match item.kind {
                MenuItemKind::Divider => {
                    div()
                        .my(px(4.0))
                        .mx(px(8.0))
                        .h(px(1.0))
                        .bg(theme.border_surface)
                        .into_any_element()
                }
                MenuItemKind::Heading => {
                    div()
                        .px(px(12.0))
                        .pt(px(6.0))
                        .pb(px(4.0))
                        .text_size(px(11.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.text_faint)
                        .child(item.label)
                        .into_any_element()
                }
                MenuItemKind::Action | MenuItemKind::Check(_) | MenuItemKind::Radio(_) | MenuItemKind::Submenu(_) => {
                    let act = item.action.clone();
                    let is_selected = selected_index == Some(ix);
                    let cls = on_close.clone();
                    let is_disabled = item.disabled;
                    let is_checked = match item.kind {
                        MenuItemKind::Check(c) => c,
                        MenuItemKind::Radio(r) => r,
                        _ => false,
                    };
                    let is_submenu = matches!(item.kind, MenuItemKind::Submenu(_));
                    let is_sub_open = open_submenu_ix == Some(ix);

                    let mut row = div()
                        .id(("menu-item", ix))
                        .relative()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(12.0))
                        .h(sz::ROW_H)
                        .px(px(12.0))
                        .rounded(px(5.0))
                        .text_size(px(13.0))
                        .when(is_disabled, |s| {
                            s.text_color(theme.menu_disabled)
                        })
                        .when(!is_disabled, |s| {
                            let sub = on_set_submenu.clone();
                            s.cursor_pointer()
                                .text_color(if is_selected || is_sub_open { theme.text_strong } else { theme.text_primary })
                                .when(is_selected || is_sub_open, |sel| sel.bg(theme.border_subtle))
                                .hover(|h| h.bg(theme.border_subtle).text_color(theme.text_strong))
                                .when(!is_submenu, |act_s| {
                                    let cls = cls.clone();
                                    let sub = sub.clone();
                                    act_s
                                        .on_mouse_move(move |_, _, cx| sub(None, cx))
                                        .on_click(move |_, window, cx| {
                                            (act)(window, cx);
                                            cls(cx);
                                        })
                                })
                                .when(is_submenu, |sub_s| {
                                    let sub_clk = sub.clone();
                                    let sub_hov = sub.clone();
                                    sub_s
                                        .on_mouse_move(move |_, _, cx| sub_hov(Some(ix), cx))
                                        .on_click(move |_, _, cx| {
                                            sub_clk(if is_sub_open { None } else { Some(ix) }, cx);
                                        })
                                })
                        });

                    // Left side: check column (if applicable) + optional icon + label
                    let left_side = div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .when(has_checks, |s| {
                            s.child(
                                div()
                                    .w(px(16.0))
                                    .flex()
                                    .items_center()
                                    .justify_start()
                                    .when(is_checked, |ck| {
                                        ck.child(render_icon("check", 14.0, theme.text_hi))
                                    })
                            )
                        })
                        .when_some(item.icon, |s, icon| {
                            s.child(render_icon(icon, 14.0, if is_disabled { theme.menu_disabled } else { theme.text_muted }))
                        })
                        .child(item.label);

                    row = row.child(left_side);

                    // Right side: hint chip + shortcut text / submenu chevron
                    let right_side = div()
                        .flex()
                        .items_center()
                        .flex_shrink_0()
                        .gap(px(8.0))
                        .when_some(item.hint, |s, hint| {
                            s.child(
                                div()
                                    .px(px(5.0))
                                    .py(px(1.0))
                                    .rounded(px(4.0))
                                    .border_1()
                                    .border_color(theme.border_surface)
                                    .text_size(px(11.0))
                                    .text_color(theme.menu_disabled)
                                    .child(hint)
                            )
                        })
                        .when_some(item.shortcut, |s, sc| {
                            s.child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(if is_selected { rgb(0xb4b4b4) } else { theme.menu_shortcut })
                                    .child(sc)
                            )
                        })
                        .when(is_submenu, |s| {
                            s.child(render_icon("chevron-right", 12.0, if is_sub_open { theme.text_strong } else { theme.text_muted }))
                        });

                    row = row.child(right_side);

                    row.into_any_element()
                }
            }
        })
        .collect();

    div()
        .id(id)
        .flex()
        .flex_col()
        .w(width)
        .max_h(px(480.0))
        .overflow_y_scroll()
        .p(px(4.0))
        .rounded(sz::R_LG)
        .bg(theme.menu_bg)
        .border_1()
        .border_color(theme.border_surface)
        .shadow_2xl()
        .children(rows)
        .into_any_element()
}

pub fn render_dropdown_menu<FClose, FSubmenu>(
    props: MenuProps<FClose, FSubmenu>,
    theme: &Theme,
) -> impl IntoElement
where
    FClose: Fn(&mut App) + 'static + Clone,
    FSubmenu: Fn(Option<usize>, &mut App) + 'static + Clone,
{
    let cls = props.on_close.clone();
    let point = props.point;
    let anchor_corner = props.anchor_corner;
    let width = props.width;
    let items = props.items;
    let selected_index = props.selected_index;
    let open_submenu_ix = props.open_submenu_ix;
    let focus_handle = props.focus_handle;
    let id = props.id;
    let on_set_submenu = props.on_set_submenu;

    let submenu_element = if let Some(sub_ix) = open_submenu_ix {
        if let Some(item) = items.get(sub_ix) {
            if let MenuItemKind::Submenu(ref sub_items) = item.kind {
                let y_offset = calculate_submenu_y_offset(&items, sub_ix);
                let sub_w = calculate_submenu_width(item);
                let is_right_anchored = matches!(
                    anchor_corner,
                    Corner::TopRight | Corner::BottomRight
                );
                let (sub_anchor, sub_point) = if is_right_anchored {
                    (Corner::TopRight, Point::new(point.x - width - SUBMENU_GAP, point.y + y_offset))
                } else {
                    (Corner::TopLeft, Point::new(point.x + width + SUBMENU_GAP, point.y + y_offset))
                };
                let sub_cls = cls.clone();
                let theme_clone = *theme;
                Some(
                    anchored()
                        .position(sub_point)
                        .anchor(sub_anchor)
                        .snap_to_window()
                        .child(
                            div()
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                .child(render_menu_panel(
                                    ("sub-menu", sub_ix).into(),
                                    sub_w,
                                    sub_items.clone(),
                                    None,
                                    None,
                                    sub_cls,
                                    |_, _: &mut App| {},
                                    &theme_clone,
                                ))
                        )
                )
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    div()
        .id("dropdown-menu-overlay-root")
        .absolute()
        .inset_0()
        .child(
            // Invisible dismissal backdrop
            div()
                .id("menu-dismiss-backdrop")
                .absolute()
                .inset_0()
                .on_mouse_down(MouseButton::Left, {
                    let cls = cls.clone();
                    move |_, _, cx| cls(cx)
                })
        )
        .child(
            deferred(
                anchored()
                    .position(point)
                    .anchor(anchor_corner)
                    .snap_to_window()
                    .child(
                        div()
                            .track_focus(&focus_handle)
                            .key_context("Menu")
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .child(render_menu_panel(
                                id,
                                width,
                                items,
                                selected_index,
                                open_submenu_ix,
                                props.on_close,
                                on_set_submenu,
                                theme,
                            ))
                    )
            )
            .with_priority(1)
        )
        .when_some(submenu_element, |s, sub| {
            s.child(
                deferred(sub).with_priority(2)
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    #[test]
    fn test_calculate_submenu_y_offset() {
        let items = vec![
            MenuItem::action("Item 1", None, None, |_, _| {}),
            MenuItem::divider(),
            MenuItem::heading("Section"),
            MenuItem::action("Item 2", None, None, |_, _| {}),
        ];

        assert_eq!(calculate_submenu_y_offset(&items, 0), MENU_PANEL_PAD);
        assert_eq!(calculate_submenu_y_offset(&items, 1), MENU_PANEL_PAD + MENU_ITEM_H);
        assert_eq!(calculate_submenu_y_offset(&items, 2), MENU_PANEL_PAD + MENU_ITEM_H + MENU_DIVIDER_H);
        assert_eq!(
            calculate_submenu_y_offset(&items, 3),
            MENU_PANEL_PAD + MENU_ITEM_H + MENU_DIVIDER_H + MENU_HEADING_H
        );
    }

    #[test]
    fn test_calculate_submenu_width() {
        let sub = MenuItem::submenu("Test", None, vec![
            MenuItem::action("Short", None, None, |_, _| {}),
            MenuItem::action("A somewhat longer item label", None, None, |_, _| {}),
        ]);
        let width = calculate_submenu_width(&sub);
        assert!(width >= px(160.0));
        assert!(width <= px(320.0));
    }
}

