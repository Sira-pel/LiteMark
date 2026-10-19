use super::primitives::render_icon;
use crate::model::DocumentTab;
use crate::theme::{sz, Theme};
use gpui::prelude::*;
use gpui::*;
use std::cell::Cell;
use std::rc::Rc;

pub struct HeaderBarProps<
    FSelectTab,
    FCloseTab,
    FNewTab,
    FToggleSidebar,
    FMinimize,
    FMaximize,
    FCloseWindow,
> where
    FSelectTab: Fn(String, &mut App) + 'static + Clone,
    FCloseTab: Fn(String, &mut App) + 'static + Clone,
    FNewTab: Fn(&mut App) + 'static + Clone,
    FToggleSidebar: Fn(&mut App) + 'static + Clone,
    FMinimize: Fn(&mut Window, &mut App) + 'static + Clone,
    FMaximize: Fn(&mut Window, &mut App) + 'static + Clone,
    FCloseWindow: Fn(&mut Window, &mut App) + 'static + Clone,
{
    pub tabs: Vec<DocumentTab>,
    pub active_tab_id: String,
    pub is_sidebar_open: bool,
    pub is_maximized: bool,
    pub on_select_tab: FSelectTab,
    pub on_close_tab: FCloseTab,
    pub on_new_tab: FNewTab,
    pub on_toggle_sidebar: FToggleSidebar,
    pub on_minimize: FMinimize,
    pub on_maximize: FMaximize,
    pub on_close_window: FCloseWindow,
}

pub fn render_header_bar<
    FSelectTab,
    FCloseTab,
    FNewTab,
    FToggleSidebar,
    FMinimize,
    FMaximize,
    FCloseWindow,
>(
    props: HeaderBarProps<
        FSelectTab,
        FCloseTab,
        FNewTab,
        FToggleSidebar,
        FMinimize,
        FMaximize,
        FCloseWindow,
    >,
    theme: &Theme,
) -> impl IntoElement
where
    FSelectTab: Fn(String, &mut App) + 'static + Clone,
    FCloseTab: Fn(String, &mut App) + 'static + Clone,
    FNewTab: Fn(&mut App) + 'static + Clone,
    FToggleSidebar: Fn(&mut App) + 'static + Clone,
    FMinimize: Fn(&mut Window, &mut App) + 'static + Clone,
    FMaximize: Fn(&mut Window, &mut App) + 'static + Clone,
    FCloseWindow: Fn(&mut Window, &mut App) + 'static + Clone,
{
    let on_select = props.on_select_tab;
    let on_close_tab = props.on_close_tab;
    let on_new_tab = props.on_new_tab;
    let on_toggle_sidebar = props.on_toggle_sidebar;
    let on_min = props.on_minimize;
    let on_max = props.on_maximize;
    let on_close_win = props.on_close_window;
    let active_id = props.active_tab_id;
    let tab_count = props.tabs.len();
    let press_origin: Rc<Cell<Option<Point<Pixels>>>> = Rc::new(Cell::new(None));

    div()
        .id("title-bar")
        .relative()
        .flex()
        .items_center()
        .justify_between()
        .h(sz::TITLE_BAR)
        .px(px(8.0))
        .bg(theme.bg_header)
        .flex_shrink_0()
        // Double-click zooms. A plain click only arms a drag so the move
        // grab does not swallow the second click of a double-click.
        .on_mouse_down(MouseButton::Left, {
            let press_origin = press_origin.clone();
            move |event, window, cx| {
                if title_bar_pointer(event.click_count) == TitleBarPointer::Zoom {
                    press_origin.set(None);
                    window.zoom_window();
                    window.refresh();
                } else {
                    press_origin.set(Some(event.position));
                }
                cx.stop_propagation();
            }
        })
        .on_mouse_up(MouseButton::Left, {
            let press_origin = press_origin.clone();
            move |_, _, _| {
                press_origin.set(None);
            }
        })
        .on_mouse_move({
            let press_origin = press_origin.clone();
            move |event, window, cx| {
                let Some(origin) = press_origin.get() else {
                    return;
                };
                if event.pressed_button != Some(MouseButton::Left) {
                    press_origin.set(None);
                    return;
                }
                let moved = (event.position.x - origin.x).abs() > px(4.0)
                    || (event.position.y - origin.y).abs() > px(4.0);
                if moved {
                    press_origin.set(None);
                    window.start_window_move();
                    cx.stop_propagation();
                }
            }
        })
        .on_mouse_down(MouseButton::Right, |e, window, cx| {
            window.show_window_menu(e.position);
            cx.stop_propagation();
        })
        // Hairline along the bottom (broken under active tab)
        .child(
            div()
                .absolute()
                .left_0()
                .right_0()
                .bottom_0()
                .h(px(1.0))
                .bg(theme.border_subtle),
        )
        // Left: Sidebar Toggle + Tabs + "+"
        .child(
            div()
                .flex()
                .items_end()
                .h_full()
                .child(
                    // Sidebar Toggle: 32x28 at y 6-34, 8px gap to tabs
                    div().flex().items_center().h_full().mr(px(8.0)).child({
                        let t_sb = on_toggle_sidebar.clone();
                        div()
                            .id("hb-toggle-sidebar")
                            .flex()
                            .items_center()
                            .justify_center()
                            .w(sz::BTN)
                            .h(sz::TB_TILE_H)
                            .rounded(sz::R_MD)
                            .cursor_pointer()
                            .bg(if props.is_sidebar_open {
                                theme.row_active
                            } else {
                                rgba(0x00000000)
                            })
                            .hover(|h| h.bg(theme.bg_hover))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(move |_, _, cx| t_sb(cx))
                            .child(render_icon(
                                "sidebar-toggle",
                                16.0,
                                if props.is_sidebar_open {
                                    theme.text_strong
                                } else {
                                    theme.text_muted
                                },
                            ))
                    }),
                )
                // Tabs strip: 32px tall, flush to bottom, active covers bottom line
                .child(
                    div()
                        .id("tabs-strip")
                        .flex()
                        .items_end()
                        .gap(px(4.0))
                        .h_full()
                        .children(props.tabs.into_iter().map(|tab| {
                            let is_active = tab.id == active_id;
                            let tab_id = tab.id.clone();
                            let sel = on_select.clone();
                            let cls = on_close_tab.clone();
                            let tid_for_close = tab.id.clone();

                            div()
                                .id(SharedString::from(format!("tab-{}", tab_id)))
                                .group("header-tab")
                                .relative()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .px(px(12.0))
                                .h(sz::TAB_H)
                                .rounded_t(sz::R_MD)
                                .cursor_pointer()
                                .bg(if is_active {
                                    theme.bg_toolbar
                                } else {
                                    rgba(0x00000000)
                                })
                                .text_color(if is_active {
                                    theme.text_hi
                                } else {
                                    theme.text_muted
                                })
                                .font_weight(FontWeight::NORMAL)
                                .text_size(px(13.0))
                                .when(!is_active, |s| {
                                    s.hover(|h| h.bg(theme.bg_hover).text_color(theme.text_primary))
                                })
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                .on_click(move |_, _, cx| sel(tab_id.clone(), cx))
                                .child(render_icon(
                                    "file-text",
                                    14.0,
                                    if is_active {
                                        theme.text_hi
                                    } else {
                                        theme.text_muted
                                    },
                                ))
                                .child(
                                    div()
                                        .max_w(px(200.0))
                                        .min_w(px(60.0))
                                        .truncate()
                                        .child(tab.title),
                                )
                                // Close slot (16px) with close button or modified dot
                                .child(
                                    div()
                                        .w(px(16.0))
                                        .h(px(16.0))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .child(
                                            // Close button on hover
                                            div()
                                                .id(SharedString::from(format!(
                                                    "close-tab-{}",
                                                    tid_for_close
                                                )))
                                                .w(px(16.0))
                                                .h(px(16.0))
                                                .rounded(sz::R_SM)
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .opacity(0.0)
                                                .when(tab_count > 1, |s| {
                                                    s.group_hover("header-tab", |h| h.opacity(1.0))
                                                })
                                                .hover(|h| {
                                                    h.bg(theme.border_subtle)
                                                        .text_color(theme.text_strong)
                                                })
                                                .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                                    cx.stop_propagation()
                                                })
                                                .on_click(move |_, _, cx| {
                                                    cls(tid_for_close.clone(), cx);
                                                })
                                                .child(render_icon("x", 12.0, theme.text_muted)),
                                        )
                                        .when(tab.is_modified, |s| {
                                            // Modified dot when not hovered
                                            s.child(
                                                div()
                                                    .w(px(6.0))
                                                    .h(px(6.0))
                                                    .rounded_full()
                                                    .bg(theme.text_hi)
                                                    .group_hover("header-tab", |h| h.invisible()),
                                            )
                                        }),
                                )
                        }))
                        // "+" button: 28x28 at y 6-34, 4px after last tab
                        .child({
                            let nt = on_new_tab.clone();
                            div().flex().items_center().h_full().child(
                                div()
                                    .id("hb-new-tab")
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .w(sz::PLUS)
                                    .h(sz::PLUS)
                                    .rounded(sz::R_MD)
                                    .cursor_pointer()
                                    .hover(|h| h.bg(theme.tile_hover).text_color(theme.text_hi))
                                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                        cx.stop_propagation()
                                    })
                                    .on_click(move |_, _, cx| nt(cx))
                                    .child(render_icon("plus", 14.0, theme.text_muted)),
                            )
                        }),
                ),
        )
        // Center spacer (flex_1): clicking here drags the window
        .child(div().flex_1().h_full())
        // Right: Window Controls (36x28, radius 6, 4px gaps, 8px from right edge)
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(4.0))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                // Minimize
                .child({
                    let m = on_min.clone();
                    div()
                        .id("wc-min-btn")
                        .flex()
                        .items_center()
                        .justify_center()
                        .w(sz::WC_W)
                        .h(sz::WC_H)
                        .rounded(sz::R_MD)
                        .cursor_pointer()
                        .hover(|h| h.bg(theme.tile_hover).text_color(theme.text_hi))
                        .on_click(move |_, window, cx| m(window, cx))
                        .child(render_icon("wc-min", 10.0, theme.wc_glyph))
                })
                // Maximize / Restore
                .child({
                    let m = on_max.clone();
                    let icon = if props.is_maximized {
                        "wc-restore"
                    } else {
                        "wc-max"
                    };
                    div()
                        .id("wc-max-btn")
                        .flex()
                        .items_center()
                        .justify_center()
                        .w(sz::WC_W)
                        .h(sz::WC_H)
                        .rounded(sz::R_MD)
                        .cursor_pointer()
                        .hover(|h| h.bg(theme.tile_hover).text_color(theme.text_hi))
                        .on_click(move |_, window, cx| m(window, cx))
                        .child(render_icon(icon, 10.0, theme.wc_glyph))
                })
                // Close (the only red in the app: close_hover with white glyph)
                .child({
                    let c = on_close_win.clone();
                    div()
                        .id("wc-close-btn")
                        .flex()
                        .items_center()
                        .justify_center()
                        .w(sz::WC_W)
                        .h(sz::WC_H)
                        .rounded(sz::R_MD)
                        .cursor_pointer()
                        .hover(|h| h.bg(theme.close_hover).text_color(rgb(0xffffff)))
                        .on_click(move |_, window, cx| c(window, cx))
                        .child(render_icon("wc-close", 10.0, theme.wc_glyph))
                }),
        )
}

const EDGE_PX: f32 = 6.0;
const CORNER_PX: f32 = 16.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TitleBarPointer {
    Zoom,
    /// A single click arms dragging. The move starts on the following drag
    /// so a double-click can still reach [`Window::zoom_window`].
    ArmDrag,
}

pub fn title_bar_pointer(click_count: usize) -> TitleBarPointer {
    if click_count >= 2 {
        TitleBarPointer::Zoom
    } else {
        TitleBarPointer::ArmDrag
    }
}

#[derive(Clone, Copy)]
pub struct ResizeHit {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub edge: f32,
    pub corner: f32,
    pub tile_top: bool,
    pub tile_right: bool,
    pub tile_bottom: bool,
    pub tile_left: bool,
}

/// Map a window-local point to a resize edge.
/// The right side is [`ResizeEdge::Right`] and the bottom-right corner is
/// [`ResizeEdge::BottomRight`]. Tiled edges are not resizable.
pub fn client_resize_edge(hit: ResizeHit) -> Option<ResizeEdge> {
    let ResizeHit {
        x,
        y,
        width,
        height,
        edge,
        corner,
        tile_top,
        tile_right,
        tile_bottom,
        tile_left,
    } = hit;
    if width <= 0.0 || height <= 0.0 || edge <= 0.0 || corner <= 0.0 {
        return None;
    }
    if x < 0.0 || y < 0.0 || x > width || y > height {
        return None;
    }
    let corner_x = corner.min(width);
    let corner_y = corner.min(height);
    let on_left = !tile_left && x < corner_x;
    let on_right = !tile_right && x >= width - corner_x;
    let on_top = !tile_top && y < corner_y;
    let on_bottom = !tile_bottom && y >= height - corner_y;
    if on_top && on_left {
        return Some(ResizeEdge::TopLeft);
    }
    if on_top && on_right {
        return Some(ResizeEdge::TopRight);
    }
    if on_bottom && on_left {
        return Some(ResizeEdge::BottomLeft);
    }
    if on_bottom && on_right {
        return Some(ResizeEdge::BottomRight);
    }
    let edge_x = edge.min(width);
    let edge_y = edge.min(height);
    if !tile_top && y < edge_y {
        Some(ResizeEdge::Top)
    } else if !tile_bottom && y >= height - edge_y {
        Some(ResizeEdge::Bottom)
    } else if !tile_left && x < edge_x {
        Some(ResizeEdge::Left)
    } else if !tile_right && x >= width - edge_x {
        Some(ResizeEdge::Right)
    } else {
        None
    }
}

fn resize_handle(id: &'static str, cursor: CursorStyle) -> Stateful<Div> {
    div()
        .id(id)
        .cursor(cursor)
        .on_mouse_move(|_, window, _| window.refresh())
        .on_mouse_down(MouseButton::Left, |event, window, cx| {
            let size = window.viewport_size();
            let (tile_top, tile_right, tile_bottom, tile_left) = match window.window_decorations() {
                Decorations::Client { tiling } => {
                    (tiling.top, tiling.right, tiling.bottom, tiling.left)
                }
                Decorations::Server => {
                    cx.stop_propagation();
                    return;
                }
            };
            if let Some(edge) = client_resize_edge(ResizeHit {
                x: f32::from(event.position.x),
                y: f32::from(event.position.y),
                width: f32::from(size.width),
                height: f32::from(size.height),
                edge: EDGE_PX,
                corner: CORNER_PX,
                tile_top,
                tile_right,
                tile_bottom,
                tile_left,
            }) {
                window.start_window_resize(edge);
                window.refresh();
            }
            cx.stop_propagation();
        })
}

/// Edge and corner hit targets for client-side window decorations.
/// The layer is window-sized and painted above the document so the edges
/// receive hits. The right edge starts a [`ResizeEdge::Right`] resize.
pub fn render_client_resize_borders(window: &Window) -> AnyElement {
    let Decorations::Client { tiling } = window.window_decorations() else {
        return div().into_any_element();
    };
    let edge = px(EDGE_PX);
    let corner = px(CORNER_PX);
    div()
        .id("client-resize-layer")
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .when(!tiling.top, |parent| {
            parent.child(
                resize_handle("resize-top", CursorStyle::ResizeUpDown)
                    .absolute()
                    .top_0()
                    .left(corner)
                    .right(corner)
                    .h(edge),
            )
        })
        .when(!tiling.bottom, |parent| {
            parent.child(
                resize_handle("resize-bottom", CursorStyle::ResizeUpDown)
                    .absolute()
                    .bottom_0()
                    .left(corner)
                    .right(corner)
                    .h(edge),
            )
        })
        .when(!tiling.left, |parent| {
            parent.child(
                resize_handle("resize-left", CursorStyle::ResizeLeftRight)
                    .absolute()
                    .left_0()
                    .top(corner)
                    .bottom(corner)
                    .w(edge),
            )
        })
        .when(!tiling.right, |parent| {
            parent.child(
                resize_handle("resize-right", CursorStyle::ResizeLeftRight)
                    .absolute()
                    .right_0()
                    .top(corner)
                    .bottom(corner)
                    .w(edge),
            )
        })
        .when(!tiling.top && !tiling.left, |parent| {
            parent.child(
                resize_handle("resize-top-left", CursorStyle::ResizeUpLeftDownRight)
                    .absolute()
                    .top_0()
                    .left_0()
                    .w(corner)
                    .h(corner),
            )
        })
        .when(!tiling.top && !tiling.right, |parent| {
            parent.child(
                resize_handle("resize-top-right", CursorStyle::ResizeUpRightDownLeft)
                    .absolute()
                    .top_0()
                    .right_0()
                    .w(corner)
                    .h(corner),
            )
        })
        .when(!tiling.bottom && !tiling.left, |parent| {
            parent.child(
                resize_handle("resize-bottom-left", CursorStyle::ResizeUpRightDownLeft)
                    .absolute()
                    .bottom_0()
                    .left_0()
                    .w(corner)
                    .h(corner),
            )
        })
        .when(!tiling.bottom && !tiling.right, |parent| {
            parent.child(
                resize_handle("resize-bottom-right", CursorStyle::ResizeUpLeftDownRight)
                    .absolute()
                    .bottom_0()
                    .right_0()
                    .w(corner)
                    .h(corner),
            )
        })
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::{
        client_resize_edge, title_bar_pointer, ResizeHit, TitleBarPointer, CORNER_PX, EDGE_PX,
    };
    use gpui::ResizeEdge;

    fn hit(x: f32, y: f32, width: f32, height: f32, tile_right: bool) -> ResizeHit {
        ResizeHit {
            x,
            y,
            width,
            height,
            edge: EDGE_PX,
            corner: CORNER_PX,
            tile_top: false,
            tile_right,
            tile_bottom: false,
            tile_left: false,
        }
    }

    #[test]
    fn right_edge_grows_from_the_right_and_corners_are_diagonal() {
        let width = 800.0;
        let height = 600.0;
        assert_eq!(
            client_resize_edge(hit(width - 1.0, height / 2.0, width, height, false)),
            Some(ResizeEdge::Right)
        );
        assert_eq!(
            client_resize_edge(hit(1.0, height / 2.0, width, height, false)),
            Some(ResizeEdge::Left)
        );
        assert_eq!(
            client_resize_edge(hit(width - 1.0, height - 1.0, width, height, false)),
            Some(ResizeEdge::BottomRight)
        );
        assert_eq!(
            client_resize_edge(hit(width / 2.0, height - 1.0, width, height, false)),
            Some(ResizeEdge::Bottom)
        );
        assert_eq!(
            client_resize_edge(hit(width / 2.0, height / 2.0, width, height, false)),
            None
        );
        assert_eq!(
            client_resize_edge(hit(width - 1.0, height / 2.0, width, height, true)),
            None
        );
    }

    #[test]
    fn header_double_click_zooms_without_starting_a_move() {
        assert_eq!(title_bar_pointer(1), TitleBarPointer::ArmDrag);
        assert_eq!(title_bar_pointer(2), TitleBarPointer::Zoom);
        assert_eq!(title_bar_pointer(3), TitleBarPointer::Zoom);
    }
}
