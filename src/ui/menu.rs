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

/// Gap kept between a popup and the window edges; the bottom also clears the status bar.
const POPUP_MARGIN: Edges<Pixels> = Edges {
    top: px(8.0),
    right: px(8.0),
    bottom: px(36.0),
    left: px(8.0),
};

/// Upper bound for a content-sized menu; longer labels truncate.
const MENU_MAX_W: f32 = 360.0;

/// Width a menu needs to show its widest row without clipping: check/icon
/// column, label, hint chip and shortcut, plus row and panel padding.
pub fn content_width(items: &[MenuItem]) -> Pixels {
    let has_icons_or_checks = items.iter().any(|it| {
        it.icon.is_some() || matches!(it.kind, MenuItemKind::Check(_) | MenuItemKind::Radio(_))
    });
    let icon_col = if has_icons_or_checks { 24.0 } else { 0.0 };
    let widest = items
        .iter()
        .filter(|it| !matches!(it.kind, MenuItemKind::Divider | MenuItemKind::Heading))
        .map(|it| {
            let label = it.label.chars().count() as f32 * 7.5;
            let hint = it.hint.map_or(0.0, |h| h.chars().count() as f32 * 6.5 + 18.0);
            let shortcut = it.shortcut.map_or(0.0, |s| s.chars().count() as f32 * 7.0);
            let chevron = if matches!(it.kind, MenuItemKind::Submenu(_)) { 20.0 } else { 0.0 };
            let gaps = if hint > 0.0 && shortcut > 0.0 { 8.0 } else { 0.0 };
            // 24 = row px(12) * 2, 12 = gap between left and right sides
            label + hint + shortcut + chevron + gaps + 24.0 + 12.0
        })
        .fold(0.0_f32, f32::max);
    px((icon_col + widest + 8.0).min(MENU_MAX_W))
}

pub fn calculate_submenu_width(item: &MenuItem) -> Pixels {
    if let MenuItemKind::Submenu(ref sub_items) = item.kind {
        content_width(sub_items).max(px(160.0))
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
    /// Widest the menu may grow (the window width less a margin).
    pub max_width: Pixels,
    pub items: Vec<MenuItem>,
    pub focus_handle: FocusHandle,
    pub selected_index: Option<usize>,
    pub open_submenu_ix: Option<usize>,
    /// Keyboard selection inside the open submenu.
    pub submenu_selected_index: Option<usize>,
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
                        .min_w_0()
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
                        .child(div().truncate().child(item.label));

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
                                    .text_color(if is_selected { theme.menu_shortcut_active } else { theme.menu_shortcut })
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
    let items = props.items;
    let max_width = props.max_width;
    let width = props.width.max(content_width(&items)).min(max_width);
    let selected_index = props.selected_index;
    let open_submenu_ix = props.open_submenu_ix;
    let focus_handle = props.focus_handle;
    let id = props.id;
    let on_set_submenu = props.on_set_submenu;

    let submenu_element = if let Some(sub_ix) = open_submenu_ix {
        if let Some(item) = items.get(sub_ix) {
            if let MenuItemKind::Submenu(ref sub_items) = item.kind {
                let y_offset = calculate_submenu_y_offset(&items, sub_ix);
                let sub_w = calculate_submenu_width(item).min(max_width);
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
                        .snap_to_window_with_margin(POPUP_MARGIN)
                        .child(
                            div()
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                .child(render_menu_panel(
                                    ("sub-menu", sub_ix).into(),
                                    sub_w,
                                    sub_items.clone(),
                                    props.submenu_selected_index,
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
                    .snap_to_window_with_margin(POPUP_MARGIN)
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

pub struct TablePickerMenuProps<FHover, FPick, FEditAction, FClose, FSubmenu>
where
    FHover: Fn(usize, usize, &mut App) + 'static + Clone,
    FPick: Fn(usize, usize, &mut App) + 'static + Clone,
    FEditAction: Fn(&'static str, &mut App) + 'static + Clone,
    FClose: Fn(&mut App) + 'static + Clone,
    FSubmenu: Fn(Option<usize>, &mut App) + 'static + Clone,
{
    pub point: Point<Pixels>,
    pub hovered_size: (usize, usize),
    pub in_table: bool,
    pub open_submenu_ix: Option<usize>,
    pub focus_handle: FocusHandle,
    pub on_hover_size: FHover,
    pub on_pick_size: FPick,
    pub on_edit_action: FEditAction,
    pub on_close: FClose,
    pub on_set_submenu: FSubmenu,
}

pub fn build_edit_table_submenu_items(
    on_edit_action: impl Fn(&'static str, &mut App) + 'static + Clone,
) -> Vec<MenuItem> {
    let make = |label: &'static str, cmd: &'static str| {
        let f = on_edit_action.clone();
        MenuItem::action(label, None, None, move |_, cx| f(cmd, cx))
    };
    vec![
        make("Add row above", "row_above"),
        make("Add row below", "row_below"),
        MenuItem::divider(),
        make("Add column left", "col_left"),
        make("Add column right", "col_right"),
        MenuItem::divider(),
        make("Align left", "align_left"),
        make("Align center", "align_center"),
        make("Align right", "align_right"),
        MenuItem::divider(),
        make("Delete row", "delete_row"),
        make("Delete column", "delete_col"),
        make("Delete table", "delete_table"),
    ]
}

pub fn build_table_context_menu_items(
    has_selection: bool,
    on_cut: impl Fn(&mut App) + 'static + Clone,
    on_copy: impl Fn(&mut App) + 'static + Clone,
    on_paste: impl Fn(&mut App) + 'static + Clone,
    on_edit_action: impl Fn(&'static str, &mut App) + 'static + Clone,
) -> Vec<MenuItem> {
    let mut items = Vec::new();

    let make = |label: &'static str, cmd: &'static str| {
        let f = on_edit_action.clone();
        MenuItem::action(label, None, None, move |_, cx| f(cmd, cx))
    };

    items.push(make("Add row above", "row_above"));
    items.push(make("Add row below", "row_below"));
    items.push(MenuItem::divider());
    items.push(make("Add column left", "col_left"));
    items.push(make("Add column right", "col_right"));
    items.push(MenuItem::divider());
    items.push(make("Align left", "align_left"));
    items.push(make("Align center", "align_center"));
    items.push(make("Align right", "align_right"));
    items.push(MenuItem::divider());
    items.push(make("Delete row", "delete_row"));
    items.push(make("Delete column", "delete_col"));
    items.push(make("Delete table", "delete_table"));

    items.push(MenuItem::divider());

    if has_selection {
        let f_cut = on_cut.clone();
        items.push(MenuItem::action("Cut", None, Some("Ctrl+X"), move |_, cx| f_cut(cx)));
        let f_copy = on_copy.clone();
        items.push(MenuItem::action("Copy", None, Some("Ctrl+C"), move |_, cx| f_copy(cx)));
    }
    let f_paste = on_paste.clone();
    items.push(MenuItem::action("Paste", None, Some("Ctrl+V"), move |_, cx| f_paste(cx)));

    items
}

pub fn build_editor_context_menu_items(
    has_selection: bool,
    on_cut: impl Fn(&mut App) + 'static + Clone,
    on_copy: impl Fn(&mut App) + 'static + Clone,
    on_paste: impl Fn(&mut App) + 'static + Clone,
    on_select_all: impl Fn(&mut App) + 'static + Clone,
) -> Vec<MenuItem> {
    let mut items = Vec::new();

    if has_selection {
        let f_cut = on_cut.clone();
        items.push(MenuItem::action("Cut", None, Some("Ctrl+X"), move |_, cx| f_cut(cx)));
        let f_copy = on_copy.clone();
        items.push(MenuItem::action("Copy", None, Some("Ctrl+C"), move |_, cx| f_copy(cx)));
    }
    let f_paste = on_paste.clone();
    items.push(MenuItem::action("Paste", None, Some("Ctrl+V"), move |_, cx| f_paste(cx)));
    items.push(MenuItem::divider());
    let f_sel = on_select_all.clone();
    items.push(MenuItem::action("Select all", None, Some("Ctrl+A"), move |_, cx| f_sel(cx)));

    items
}

pub fn render_context_menu<FClose>(
    id: ElementId,
    position: Point<Pixels>,
    width: Pixels,
    items: Vec<MenuItem>,
    focus_handle: FocusHandle,
    on_close: FClose,
    theme: &Theme,
) -> impl IntoElement
where
    FClose: Fn(&mut App) + 'static + Clone,
{
    let cls1 = on_close.clone();
    let cls2 = on_close.clone();
    div()
        .id(id)
        .absolute()
        .inset_0()
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            cls1(cx);
            cx.stop_propagation();
        })
        .on_mouse_down(MouseButton::Right, move |_, _, cx| {
            cls2(cx);
            cx.stop_propagation();
        })
        .child(
            deferred(
                anchored()
                    .position(position)
                    .anchor(Corner::TopLeft)
                    .snap_to_window_with_margin(POPUP_MARGIN)
                    .child(
                        div()
                            .track_focus(&focus_handle)
                            .key_context("Menu")
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
                            .child(render_menu_panel(
                                "context-menu-panel".into(),
                                width,
                                items,
                                None,
                                None,
                                on_close,
                                |_, _: &mut App| {},
                                theme,
                            )),
                    ),
            ),
        )
}

pub fn render_table_picker_menu<FHover, FPick, FEditAction, FClose, FSubmenu>(
    props: TablePickerMenuProps<FHover, FPick, FEditAction, FClose, FSubmenu>,
    theme: &Theme,
) -> impl IntoElement
where
    FHover: Fn(usize, usize, &mut App) + 'static + Clone,
    FPick: Fn(usize, usize, &mut App) + 'static + Clone,
    FEditAction: Fn(&'static str, &mut App) + 'static + Clone,
    FClose: Fn(&mut App) + 'static + Clone,
    FSubmenu: Fn(Option<usize>, &mut App) + 'static + Clone,
{
    let (cols, rows) = (
        props.hovered_size.0.clamp(1, 5),
        props.hovered_size.1.clamp(1, 5),
    );
    let point = props.point;
    let in_table = props.in_table;
    let is_sub_open = in_table && props.open_submenu_ix == Some(1);
    let cls = props.on_close.clone();
    let on_hover = props.on_hover_size.clone();
    let on_pick = props.on_pick_size.clone();
    let on_sub = props.on_set_submenu.clone();
    let panel_w = px(188.0);

    let active_border = rgb(0x4a9eff);
    let active_bg = hsla(211.0 / 360.0, 0.85, 0.58, 0.16);

    let mut grid_rows: Vec<AnyElement> = Vec::with_capacity(5);
    for r in 1..=5usize {
        let mut row_cells: Vec<AnyElement> = Vec::with_capacity(5);
        for c in 1..=5usize {
            let is_highlighted = c <= cols && r <= rows;
            let hov = on_hover.clone();
            let sub_close = on_sub.clone();
            let pick = on_pick.clone();
            let close_fn = cls.clone();
            row_cells.push(
                div()
                    .id(("table-grid-cell", r * 10 + c))
                    .w(px(26.0))
                    .h(px(26.0))
                    .rounded(px(5.0))
                    .border_1()
                    .cursor_pointer()
                    .border_color(if is_highlighted {
                        active_border
                    } else {
                        theme.border_primary
                    })
                    .bg(if is_highlighted {
                        active_bg
                    } else {
                        hsla(0.0, 0.0, 0.0, 0.0)
                    })
                    .on_mouse_move(move |_, _, cx| {
                        sub_close(None, cx);
                        hov(c, r, cx);
                    })
                    .on_click(move |_, _, cx| {
                        pick(c, r, cx);
                        close_fn(cx);
                    })
                    .into_any_element(),
            );
        }
        grid_rows.push(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(6.0))
                .children(row_cells)
                .into_any_element(),
        );
    }

    let submenu_element = if is_sub_open {
        let sub_items = build_edit_table_submenu_items(props.on_edit_action.clone());
        let sub_point = Point::new(point.x + panel_w + SUBMENU_GAP, point.y + px(214.0));
        let sub_cls = cls.clone();
        let theme_clone = *theme;
        Some(
            anchored()
                .position(sub_point)
                .anchor(Corner::TopLeft)
                .snap_to_window_with_margin(POPUP_MARGIN)
                .child(
                    div()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(render_menu_panel(
                            "table-edit-submenu".into(),
                            px(184.0),
                            sub_items,
                            None,
                            None,
                            sub_cls,
                            |_, _: &mut App| {},
                            &theme_clone,
                        )),
                ),
        )
    } else {
        None
    };

    div()
        .id("dropdown-menu-overlay-root")
        .absolute()
        .inset_0()
        .child(
            div()
                .id("menu-dismiss-backdrop")
                .absolute()
                .inset_0()
                .on_mouse_down(MouseButton::Left, {
                    let cls = cls.clone();
                    move |_, _, cx| cls(cx)
                }),
        )
        .child(
            deferred(
                anchored()
                    .position(point)
                    .anchor(Corner::TopLeft)
                    .snap_to_window_with_margin(POPUP_MARGIN)
                    .child(
                        div()
                            .id("table-picker-menu")
                            .track_focus(&props.focus_handle)
                            .key_context("Menu")
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .w(panel_w)
                            .p(px(8.0))
                            .flex()
                            .flex_col()
                            .rounded(sz::R_LG)
                            .bg(theme.menu_bg)
                            .border_1()
                            .border_color(theme.border_surface)
                            .shadow_2xl()
                            .child(
                                div()
                                    .px(px(4.0))
                                    .pb(px(6.0))
                                    .text_size(px(12.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(theme.text_muted)
                                    .child(format!("{cols} x {rows}")),
                            )
                            .child(
                                div()
                                    .px(px(4.0))
                                    .pb(px(6.0))
                                    .flex()
                                    .flex_col()
                                    .gap(px(6.0))
                                    .children(grid_rows),
                            )
                            .child(
                                div()
                                    .my(px(4.0))
                                    .mx(px(4.0))
                                    .h(px(1.0))
                                    .bg(theme.border_surface),
                            )
                            .child({
                                let pick = on_pick.clone();
                                let close_fn = cls.clone();
                                let sub_close = on_sub.clone();
                                div()
                                    .id("table-menu-insert")
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .h(sz::ROW_H)
                                    .px(px(8.0))
                                    .rounded(px(5.0))
                                    .text_size(px(13.0))
                                    .text_color(theme.text_primary)
                                    .cursor_pointer()
                                    .hover(|h| h.bg(theme.border_subtle).text_color(theme.text_strong))
                                    .on_mouse_move(move |_, _, cx| sub_close(None, cx))
                                    .on_click(move |_, _, cx| {
                                        pick(cols, rows, cx);
                                        close_fn(cx);
                                    })
                                    .child("Insert table")
                            })
                            .child({
                                let sub_hov = on_sub.clone();
                                let sub_clk = on_sub.clone();
                                div()
                                    .id("table-menu-edit")
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .h(sz::ROW_H)
                                    .px(px(8.0))
                                    .rounded(px(5.0))
                                    .text_size(px(13.0))
                                    .when(!in_table, |s| s.text_color(theme.menu_disabled))
                                    .when(in_table, |s| {
                                        s.cursor_pointer()
                                            .text_color(if is_sub_open {
                                                theme.text_strong
                                            } else {
                                                theme.text_primary
                                            })
                                            .when(is_sub_open, |sel| sel.bg(theme.border_subtle))
                                            .hover(|h| {
                                                h.bg(theme.border_subtle).text_color(theme.text_strong)
                                            })
                                            .on_mouse_move(move |_, _, cx| sub_hov(Some(1), cx))
                                            .on_click(move |_, _, cx| {
                                                sub_clk(if is_sub_open { None } else { Some(1) }, cx);
                                            })
                                    })
                                    .child("Edit table")
                                    .child(render_icon(
                                        "chevron-right",
                                        12.0,
                                        if !in_table {
                                            theme.menu_disabled
                                        } else if is_sub_open {
                                            theme.text_strong
                                        } else {
                                            theme.text_muted
                                        },
                                    ))
                            }),
                    ),
            )
            .with_priority(1),
        )
        .when_some(submenu_element, |s, sub| {
            s.child(deferred(sub).with_priority(2))
        })
}

#[cfg(test)]
mod tests {
    use super::{
        calculate_submenu_width, calculate_submenu_y_offset, MenuItem, MENU_DIVIDER_H,
        MENU_HEADING_H, MENU_ITEM_H, MENU_PANEL_PAD,
    };
    use gpui::px;

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
        assert!(width <= px(360.0));
    }

    #[test]
    fn test_build_table_context_menu_items() {
        use super::build_table_context_menu_items;

        let items_without_sel = build_table_context_menu_items(
            false,
            |_| {},
            |_| {},
            |_| {},
            |_, _| {},
        );
        let labels: Vec<&str> = items_without_sel.iter().map(|it| it.label).collect();
        assert!(labels.contains(&"Add row above"));
        assert!(labels.contains(&"Add row below"));
        assert!(labels.contains(&"Add column left"));
        assert!(labels.contains(&"Add column right"));
        assert!(labels.contains(&"Align left"));
        assert!(labels.contains(&"Align center"));
        assert!(labels.contains(&"Align right"));
        assert!(labels.contains(&"Delete row"));
        assert!(labels.contains(&"Delete column"));
        assert!(labels.contains(&"Delete table"));
        assert!(labels.contains(&"Paste"));
        assert!(!labels.contains(&"Cut"));
        assert!(!labels.contains(&"Copy"));

        let items_with_sel = build_table_context_menu_items(
            true,
            |_| {},
            |_| {},
            |_| {},
            |_, _| {},
        );
        let labels_sel: Vec<&str> = items_with_sel.iter().map(|it| it.label).collect();
        assert!(labels_sel.contains(&"Cut"));
        assert!(labels_sel.contains(&"Copy"));
        assert!(labels_sel.contains(&"Paste"));
        assert!(labels_sel.contains(&"Add row above"));
    }

    #[test]
    fn test_build_editor_context_menu_items() {
        use super::build_editor_context_menu_items;

        let items = build_editor_context_menu_items(
            true,
            |_| {},
            |_| {},
            |_| {},
            |_| {},
        );
        let labels: Vec<&str> = items.iter().map(|it| it.label).collect();
        assert!(labels.contains(&"Cut"));
        assert!(labels.contains(&"Copy"));
        assert!(labels.contains(&"Paste"));
        assert!(labels.contains(&"Select all"));
    }

    #[test]
    fn test_content_width_fits_hint_and_shortcut_and_is_capped() {
        use super::content_width;
        let plain = vec![MenuItem::action("Copy", None, None, |_, _| {})];
        let rich = vec![MenuItem::disabled_action("Line numbers", Some("Ctrl+Shift+L"), Some("Raw only"))];
        assert!(content_width(&rich) > content_width(&plain));
        assert!(content_width(&rich) >= px(12.0 * 7.5 + 8.0 * 6.5 + 12.0 * 7.0));
        let long = vec![MenuItem::action(
            "An absurdly long label that keeps going and going and going and going",
            None,
            None,
            |_, _| {},
        )];
        assert_eq!(content_width(&long), px(360.0));
    }
}
