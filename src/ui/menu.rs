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
                    let submenu_items = match item.kind {
                        MenuItemKind::Submenu(subs) => Some(subs),
                        _ => None,
                    };
                    let is_sub_open = open_submenu_ix == Some(ix);

                    let mut row = div()
                        .id(("menu-item", ix))
                        .relative()
                        .flex()
                        .items_center()
                        .justify_between()
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
                                .text_color(if is_selected { theme.text_strong } else { theme.text_primary })
                                .when(is_selected, |sel| sel.bg(theme.border_subtle))
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
                            s.child(render_icon("chevron-right", 12.0, theme.text_muted))
                        });

                    row = row.child(right_side);

                    // Submenu popup if open
                    if is_sub_open {
                        if let Some(sub_items) = submenu_items {
                            let sub_w = px(180.0);
                            let sub_cls = cls.clone();
                            let theme_clone = *theme;
                            row = row.child(
                                deferred(
                                    anchored()
                                        .position(Point::new(width - px(4.0), px(ix as f32 * 32.0)))
                                        .anchor(Corner::TopLeft)
                                        .snap_to_window()
                                        .child(render_menu_panel(
                                            ("sub-menu", ix).into(),
                                            sub_w,
                                            sub_items,
                                            None,
                                            None,
                                            sub_cls,
                                            on_set_submenu.clone(),
                                            &theme_clone,
                                        ))
                                )
                                .with_priority(2)
                            );
                        }
                    }

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

    deferred(
        anchored()
            .position(point)
            .anchor(anchor_corner)
            .snap_to_window()
            .child(
                div()
                    .track_focus(&focus_handle)
                    .key_context("Menu")
                    .on_mouse_down_out(move |_, _, cx| {
                        cls(cx);
                    })
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
}
