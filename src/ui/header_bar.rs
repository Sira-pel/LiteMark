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
    /// Lives on the app so a redraw between mouse-down and mouse-move does
    /// not drop the drag. X11 only moves if the grab starts while the button
    /// is still down.
    pub drag_armed: Rc<Cell<bool>>,
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
    let drag_armed = props.drag_armed;

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
        .overflow_hidden()
        // Double-click zooms. A plain click only arms a drag. X11 has to
        // receive `_NET_WM_MOVERESIZE` while the button is still down, so the
        // first move with the left button pressed starts the move immediately.
        .on_mouse_down(MouseButton::Left, {
            let drag_armed = drag_armed.clone();
            move |event, window, cx| {
                if title_bar_pointer(event.click_count) == TitleBarPointer::Zoom {
                    drag_armed.set(false);
                    window.zoom_window();
                    window.refresh();
                } else {
                    drag_armed.set(true);
                }
                cx.stop_propagation();
            }
        })
        .on_mouse_up(MouseButton::Left, {
            let drag_armed = drag_armed.clone();
            move |_, _, _| {
                drag_armed.set(false);
            }
        })
        .on_mouse_move({
            let drag_armed = drag_armed.clone();
            move |event, window, cx| {
                let armed = drag_armed.get();
                let button_is_left = event.pressed_button == Some(MouseButton::Left);
                if title_drag_should_move(armed, button_is_left) {
                    drag_armed.set(false);
                    window.start_window_move();
                    cx.stop_propagation();
                } else if armed && !button_is_left {
                    drag_armed.set(false);
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
                .id("title-bar-left")
                .flex()
                .items_end()
                .h_full()
                .min_w(px(0.0))
                .overflow_hidden()
                .child(
                    // Sidebar Toggle: 32x28 at y 6-34, 8px gap to tabs
                    div()
                        .flex()
                        .items_center()
                        .h_full()
                        .mr(px(8.0))
                        .flex_shrink_0()
                        .child({
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
                        .min_w(px(0.0))
                        .overflow_hidden()
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
                                .px(px(10.0))
                                .h(sz::TAB_H)
                                .rounded_t(sz::R_MD)
                                .cursor_pointer()
                                .min_w(px(36.0))
                                .overflow_hidden()
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
                                .child(
                                    div()
                                        .flex_shrink_0()
                                        .child(render_icon(
                                            "file-text",
                                            14.0,
                                            if is_active {
                                                theme.text_hi
                                            } else {
                                                theme.text_muted
                                            },
                                        ))
                                )
                                .child(
                                    div()
                                        .max_w(px(200.0))
                                        .min_w(px(0.0))
                                        .truncate()
                                        .child(tab.title),
                                )
                                // Close slot (24px) with close button or modified dot
                                .child(
                                    div()
                                        .flex_shrink_0()
                                        .w(px(24.0))
                                        .h(px(24.0))
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
                                                .w(px(24.0))
                                                .h(px(24.0))
                                                .rounded(sz::R_MD)
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
                                                .child(render_icon("x", 14.0, theme.text_muted)),
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
                        // "+" button: 28x28 centered on the tabs (32px, flush to the
                        // bottom of the bar), 4px after the last tab
                        .child({
                            let nt = on_new_tab.clone();
                            div()
                                .flex()
                                .items_center()
                                .h(sz::TAB_H)
                                .flex_shrink_0()
                                .child(
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
                )
        )
        // Center spacer (flex_1): clicking here drags the window
        .child(
            div()
                .id("title-bar-spacer")
                .flex_1()
                .min_w(px(0.0))
                .h_full()
        )
        // Right: Window Controls (36x28, radius 6, 4px gaps, 8px from right edge)
        .child(
            div()
                .id("window-controls")
                .flex()
                .items_center()
                .gap(px(4.0))
                .flex_shrink_0()
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
                        .flex_shrink_0()
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
                        .flex_shrink_0()
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
                        .flex_shrink_0()
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

/// Start `_NET_WM_MOVERESIZE` on the first move while the button is still held.
/// A distance threshold lets the button go up before the grab, and X11 then
/// leaves the window where it is.
pub fn title_drag_should_move(armed: bool, button_is_left: bool) -> bool {
    armed && button_is_left
}

#[cfg(test)]
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
#[cfg(test)]
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResizeHandleBox {
    pub edge: ResizeEdge,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Logical-pixel hit boxes for the eight edges and corners.
/// `viewport_width` and `viewport_height` are logical pixels, the same space
/// as [`Window::viewport_size`]. The platform applies display scale, so these
/// boxes stay in logical pixels at 1.5x and every other scale.
pub fn resize_handle_boxes(
    viewport_width: f32,
    viewport_height: f32,
    edge: f32,
    corner: f32,
) -> [ResizeHandleBox; 8] {
    let width = viewport_width.max(0.0);
    let height = viewport_height.max(0.0);
    let limit = width.min(height);
    let edge = edge.max(0.0).min(limit);
    let corner = corner.max(edge).min(limit);
    let span_x = (width - corner * 2.0).max(0.0);
    let span_y = (height - corner * 2.0).max(0.0);
    [
        ResizeHandleBox {
            edge: ResizeEdge::Top,
            x: corner,
            y: 0.0,
            width: span_x,
            height: edge,
        },
        ResizeHandleBox {
            edge: ResizeEdge::Bottom,
            x: corner,
            y: (height - edge).max(0.0),
            width: span_x,
            height: edge,
        },
        ResizeHandleBox {
            edge: ResizeEdge::Left,
            x: 0.0,
            y: corner,
            width: edge,
            height: span_y,
        },
        ResizeHandleBox {
            edge: ResizeEdge::Right,
            x: (width - edge).max(0.0),
            y: corner,
            width: edge,
            height: span_y,
        },
        ResizeHandleBox {
            edge: ResizeEdge::TopLeft,
            x: 0.0,
            y: 0.0,
            width: corner,
            height: corner,
        },
        ResizeHandleBox {
            edge: ResizeEdge::TopRight,
            x: (width - corner).max(0.0),
            y: 0.0,
            width: corner,
            height: corner,
        },
        ResizeHandleBox {
            edge: ResizeEdge::BottomLeft,
            x: 0.0,
            y: (height - corner).max(0.0),
            width: corner,
            height: corner,
        },
        ResizeHandleBox {
            edge: ResizeEdge::BottomRight,
            x: (width - corner).max(0.0),
            y: (height - corner).max(0.0),
            width: corner,
            height: corner,
        },
    ]
}

/// A tiled side, or a corner that includes a tiled side, is not resizable.
pub fn resize_edge_is_blocked(
    edge: ResizeEdge,
    tile_top: bool,
    tile_right: bool,
    tile_bottom: bool,
    tile_left: bool,
) -> bool {
    match edge {
        ResizeEdge::Top => tile_top,
        ResizeEdge::Right => tile_right,
        ResizeEdge::Bottom => tile_bottom,
        ResizeEdge::Left => tile_left,
        ResizeEdge::TopLeft => tile_top || tile_left,
        ResizeEdge::TopRight => tile_top || tile_right,
        ResizeEdge::BottomLeft => tile_bottom || tile_left,
        ResizeEdge::BottomRight => tile_bottom || tile_right,
    }
}

fn resize_handle_id(edge: ResizeEdge) -> &'static str {
    match edge {
        ResizeEdge::Top => "resize-top",
        ResizeEdge::Bottom => "resize-bottom",
        ResizeEdge::Left => "resize-left",
        ResizeEdge::Right => "resize-right",
        ResizeEdge::TopLeft => "resize-top-left",
        ResizeEdge::TopRight => "resize-top-right",
        ResizeEdge::BottomLeft => "resize-bottom-left",
        ResizeEdge::BottomRight => "resize-bottom-right",
    }
}

fn resize_cursor(edge: ResizeEdge) -> CursorStyle {
    match edge {
        ResizeEdge::Top | ResizeEdge::Bottom => CursorStyle::ResizeUpDown,
        ResizeEdge::Left | ResizeEdge::Right => CursorStyle::ResizeLeftRight,
        ResizeEdge::TopLeft | ResizeEdge::BottomRight => CursorStyle::ResizeUpLeftDownRight,
        ResizeEdge::TopRight | ResizeEdge::BottomLeft => CursorStyle::ResizeUpRightDownLeft,
    }
}

fn resize_handle(handle: ResizeHandleBox) -> Stateful<Div> {
    let edge = handle.edge;
    div()
        .id(resize_handle_id(edge))
        .absolute()
        .left(px(handle.x))
        .top(px(handle.y))
        .w(px(handle.width))
        .h(px(handle.height))
        .cursor(resize_cursor(edge))
        .on_mouse_down(MouseButton::Left, move |_event, window, cx| {
            // The hit box owns the edge. Re-deriving it from the pointer
            // disagrees with that box under fractional display scale, so the
            // bottom never starts and the left edge jumps.
            window.start_window_resize(edge);
            cx.stop_propagation();
        })
}

/// Edge and corner hit targets for client-side window decorations.
/// Each box calls [`Window::start_window_resize`] with its own edge on
/// mouse-down. Boxes are placed in logical pixels from the viewport size.
pub fn render_client_resize_borders(window: &Window) -> AnyElement {
    let Decorations::Client { tiling } = window.window_decorations() else {
        return div().into_any_element();
    };
    let size = window.viewport_size();
    let width = f32::from(size.width);
    let height = f32::from(size.height);
    let mut layer = div()
        .id("client-resize-layer")
        .absolute()
        .top_0()
        .left_0()
        .w(size.width)
        .h(size.height);
    for handle in resize_handle_boxes(width, height, EDGE_PX, CORNER_PX) {
        if resize_edge_is_blocked(
            handle.edge,
            tiling.top,
            tiling.right,
            tiling.bottom,
            tiling.left,
        ) || handle.width <= 0.0
            || handle.height <= 0.0
        {
            continue;
        }
        layer = layer.child(resize_handle(handle));
    }
    layer.into_any_element()
}

#[cfg(test)]
mod tests {
    use super::{
        client_resize_edge, resize_edge_is_blocked, resize_handle_boxes, title_bar_pointer,
        title_drag_should_move, ResizeHit, TitleBarPointer, CORNER_PX, EDGE_PX,
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
        assert!(title_drag_should_move(true, true));
        assert!(!title_drag_should_move(true, false));
        assert!(!title_drag_should_move(false, true));
    }

    #[test]
    fn every_resize_handle_owns_its_edge() {
        let width = 800.0;
        let height = 600.0;
        let boxes = resize_handle_boxes(width, height, EDGE_PX, CORNER_PX);
        let expected = [
            ResizeEdge::Top,
            ResizeEdge::Bottom,
            ResizeEdge::Left,
            ResizeEdge::Right,
            ResizeEdge::TopLeft,
            ResizeEdge::TopRight,
            ResizeEdge::BottomLeft,
            ResizeEdge::BottomRight,
        ];
        for edge in expected {
            let handle = boxes.iter().find(|hit| hit.edge == edge).unwrap();
            let x = handle.x + handle.width / 2.0;
            let y = handle.y + handle.height / 2.0;
            assert_eq!(
                client_resize_edge(hit(x, y, width, height, false)),
                Some(edge)
            );
        }
    }

    #[test]
    fn resize_handles_stay_on_the_logical_window_at_fractional_scale() {
        let scale = 1.5_f32;
        let logical_w = 800.0;
        let logical_h = 600.0;
        let boxes = resize_handle_boxes(logical_w, logical_h, EDGE_PX, CORNER_PX);
        let right = boxes
            .iter()
            .find(|hit| hit.edge == ResizeEdge::Right)
            .unwrap();
        let bottom = boxes
            .iter()
            .find(|hit| hit.edge == ResizeEdge::Bottom)
            .unwrap();
        let left = boxes
            .iter()
            .find(|hit| hit.edge == ResizeEdge::Left)
            .unwrap();
        let corner = boxes
            .iter()
            .find(|hit| hit.edge == ResizeEdge::BottomRight)
            .unwrap();
        assert!((right.x - (logical_w - EDGE_PX)).abs() < 0.01);
        assert!((right.x + right.width - logical_w).abs() < 0.01);
        assert!((bottom.y - (logical_h - EDGE_PX)).abs() < 0.01);
        assert!((bottom.y + bottom.height - logical_h).abs() < 0.01);
        assert!(left.x.abs() < 0.01);
        assert_eq!(left.edge, ResizeEdge::Left);
        assert!((corner.x - (logical_w - CORNER_PX)).abs() < 0.01);
        assert!((corner.y - (logical_h - CORNER_PX)).abs() < 0.01);
        assert!(right.x < logical_w * scale - EDGE_PX);
        assert!(bottom.y < logical_h * scale - EDGE_PX);
        assert!(resize_edge_is_blocked(
            ResizeEdge::Right,
            false,
            true,
            false,
            false
        ));
        assert!(resize_edge_is_blocked(
            ResizeEdge::BottomRight,
            false,
            true,
            false,
            false
        ));
        assert!(!resize_edge_is_blocked(
            ResizeEdge::Bottom,
            false,
            true,
            false,
            false
        ));
        assert!(!resize_edge_is_blocked(
            ResizeEdge::Left,
            false,
            true,
            false,
            false
        ));
    }

    #[test]
    fn test_window_controls_dimensions_and_responsiveness() {
        use gpui::px;
        // 3 buttons of WC_W (36.0) with two 4.0 gaps
        let wc_width = crate::theme::sz::WC_W * 3.0 + px(4.0) * 2.0;
        assert_eq!(wc_width, px(116.0));
        // Padding on right is 8.0, total window controls span is 124.0
        let wc_span = wc_width + px(8.0);
        assert_eq!(wc_span, px(124.0));

        // Sidebar button (32.0) + mr (8.0) + padding left (8.0) = 48.0
        let left_fixed = crate::theme::sz::BTN + px(8.0) + px(8.0);
        assert_eq!(left_fixed, px(48.0));

        // At narrow viewport of 360.0, available space for tabs = 360 - 124 - 48 = 188.0
        let min_narrow_viewport = px(360.0);
        let available_for_tabs = min_narrow_viewport - wc_span - left_fixed;
        assert!(available_for_tabs > px(100.0), "Tabs strip has space to shrink while window controls remain fully intact");
    }
}
