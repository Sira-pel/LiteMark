use gpui::prelude::*;
use gpui::*;
use crate::model::DocumentTab;
use crate::theme::{sz, Theme};
use super::primitives::render_icon;

pub struct HeaderBarProps<
    FSelectTab,
    FCloseTab,
    FNewTab,
    FToggleSidebar,
    FMinimize,
    FMaximize,
    FCloseWindow,
>
where
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
        // Window drag on empty space, double-click maximize, right click window menu
        .on_mouse_down(MouseButton::Left, |e, window, cx| {
            if e.click_count == 2 {
                window.zoom_window();
            } else {
                window.start_window_move();
            }
            cx.stop_propagation();
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
                .bg(theme.border_subtle)
        )
        // Left: Sidebar Toggle + Tabs + "+"
        .child(
            div()
                .flex()
                .items_end()
                .h_full()
                .child(
                    // Sidebar Toggle: 32x28 at y 6-34, 8px gap to tabs
                    div()
                        .flex()
                        .items_center()
                        .h_full()
                        .mr(px(8.0))
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
                                .bg(if props.is_sidebar_open { theme.row_active } else { rgba(0x00000000) })
                                .hover(|h| h.bg(theme.bg_hover))
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                .on_click(move |_, _, cx| t_sb(cx))
                                .child(render_icon("sidebar-toggle", 16.0, if props.is_sidebar_open { theme.text_strong } else { theme.text_muted }))
                        })
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
                                .bg(if is_active { theme.bg_toolbar } else { rgba(0x00000000) })
                                .text_color(if is_active { theme.text_hi } else { theme.text_muted })
                                .font_weight(FontWeight::NORMAL)
                                .text_size(px(13.0))
                                .when(!is_active, |s| {
                                    s.hover(|h| h.bg(theme.bg_hover).text_color(theme.text_primary))
                                })
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                .on_click(move |_, _, cx| sel(tab_id.clone(), cx))
                                .child(render_icon("file-text", 14.0, if is_active { theme.text_hi } else { theme.text_muted }))
                                .child(
                                    div()
                                        .max_w(px(200.0))
                                        .min_w(px(60.0))
                                        .truncate()
                                        .child(tab.title)
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
                                                .id(SharedString::from(format!("close-tab-{}", tid_for_close)))
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
                                                .hover(|h| h.bg(theme.border_subtle).text_color(theme.text_strong))
                                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                                .on_click(move |_, _, cx| {
                                                    cls(tid_for_close.clone(), cx);
                                                })
                                                .child(render_icon("x", 12.0, theme.text_muted))
                                        )
                                        .when(tab.is_modified, |s| {
                                            // Modified dot when not hovered
                                            s.child(
                                                div()
                                                    .w(px(6.0))
                                                    .h(px(6.0))
                                                    .rounded_full()
                                                    .bg(theme.text_hi)
                                                    .group_hover("header-tab", |h| h.invisible())
                                            )
                                        })
                                )
                        }))
                        // "+" button: 28x28 at y 6-34, 4px after last tab
                        .child({
                            let nt = on_new_tab.clone();
                            div()
                                .flex()
                                .items_center()
                                .h_full()
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
                                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                        .on_click(move |_, _, cx| nt(cx))
                                        .child(render_icon("plus", 14.0, theme.text_muted))
                                )
                        })
                )
        )
        // Center spacer (flex_1): clicking here drags the window
        .child(
            div()
                .flex_1()
                .h_full()
        )
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
                    let icon = if props.is_maximized { "wc-restore" } else { "wc-max" };
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
                })
        )
}
