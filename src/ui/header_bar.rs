use gpui::prelude::*;
use gpui::*;
use crate::model::DocumentTab;
use crate::theme::Theme;
use super::primitives::{icon_button, render_icon};

pub struct HeaderBarProps<FSelectTab, FCloseTab, FNewTab, FToggleSidebar, FSave, FOpen, FExport, FToggleTheme, FToggleZen, FOpenShortcuts, FZoomIn, FZoomOut, FResetZoom, FViewPdf>
where
    FSelectTab: Fn(String, &mut App) + 'static + Clone,
    FCloseTab: Fn(String, &mut App) + 'static + Clone,
    FNewTab: Fn(&mut App) + 'static + Clone,
    FToggleSidebar: Fn(&mut App) + 'static + Clone,
    FSave: Fn(&mut App) + 'static + Clone,
    FOpen: Fn(&mut App) + 'static + Clone,
    FExport: Fn(&'static str, &mut App) + 'static + Clone,
    FToggleTheme: Fn(&mut App) + 'static + Clone,
    FToggleZen: Fn(&mut App) + 'static + Clone,
    FOpenShortcuts: Fn(&mut App) + 'static + Clone,
    FZoomIn: Fn(&mut App) + 'static + Clone,
    FZoomOut: Fn(&mut App) + 'static + Clone,
    FResetZoom: Fn(&mut App) + 'static + Clone,
    FViewPdf: Fn(&mut App) + 'static + Clone,
{
    pub tabs: Vec<DocumentTab>,
    pub active_tab_id: String,
    pub is_sidebar_open: bool,
    pub zoom_level: usize,
    pub has_pdf: bool,
    pub on_select_tab: FSelectTab,
    pub on_close_tab: FCloseTab,
    pub on_new_tab: FNewTab,
    pub on_toggle_sidebar: FToggleSidebar,
    pub on_save: FSave,
    pub on_open: FOpen,
    pub on_export: FExport,
    pub on_toggle_theme: FToggleTheme,
    pub on_toggle_zen: FToggleZen,
    pub on_open_shortcuts: FOpenShortcuts,
    pub on_zoom_in: FZoomIn,
    pub on_zoom_out: FZoomOut,
    pub on_reset_zoom: FResetZoom,
    pub on_view_pdf: FViewPdf,
}

pub fn render_header_bar<FSelectTab, FCloseTab, FNewTab, FToggleSidebar, FSave, FOpen, FExport, FToggleTheme, FToggleZen, FOpenShortcuts, FZoomIn, FZoomOut, FResetZoom, FViewPdf>(
    props: HeaderBarProps<FSelectTab, FCloseTab, FNewTab, FToggleSidebar, FSave, FOpen, FExport, FToggleTheme, FToggleZen, FOpenShortcuts, FZoomIn, FZoomOut, FResetZoom, FViewPdf>,
    theme: &Theme,
) -> impl IntoElement
where
    FSelectTab: Fn(String, &mut App) + 'static + Clone,
    FCloseTab: Fn(String, &mut App) + 'static + Clone,
    FNewTab: Fn(&mut App) + 'static + Clone,
    FToggleSidebar: Fn(&mut App) + 'static + Clone,
    FSave: Fn(&mut App) + 'static + Clone,
    FOpen: Fn(&mut App) + 'static + Clone,
    FExport: Fn(&'static str, &mut App) + 'static + Clone,
    FToggleTheme: Fn(&mut App) + 'static + Clone,
    FToggleZen: Fn(&mut App) + 'static + Clone,
    FOpenShortcuts: Fn(&mut App) + 'static + Clone,
    FZoomIn: Fn(&mut App) + 'static + Clone,
    FZoomOut: Fn(&mut App) + 'static + Clone,
    FResetZoom: Fn(&mut App) + 'static + Clone,
    FViewPdf: Fn(&mut App) + 'static + Clone,
{
    let on_select = props.on_select_tab;
    let on_close_tab = props.on_close_tab;
    let on_new_tab = props.on_new_tab;
    let on_toggle_sidebar = props.on_toggle_sidebar;
    let on_save = props.on_save;
    let on_open = props.on_open;
    let on_export = props.on_export;
    let on_toggle_theme = props.on_toggle_theme;
    let on_toggle_zen = props.on_toggle_zen;
    let on_open_shortcuts = props.on_open_shortcuts;
    let on_zoom_in = props.on_zoom_in;
    let on_zoom_out = props.on_zoom_out;
    let on_reset_zoom = props.on_reset_zoom;
    let on_view_pdf = props.on_view_pdf;
    let active_id = props.active_tab_id;
    let tab_count = props.tabs.len();

    div()
        .flex()
        .items_center()
        .justify_between()
        .h(px(40.0))
        .px(px(6.0))
        .bg(theme.bg_header)
        .border_b_1()
        .border_color(theme.border_primary)
        .flex_shrink_0()
        // Left: Sidebar Toggle
        .child(
            div()
                .flex()
                .items_center()
                .mr(px(6.0))
                .child({
                    let t_sb = on_toggle_sidebar.clone();
                    div()
                        .id("hb-toggle-sidebar")
                        .p(px(6.0))
                        .rounded(px(4.0))
                        .cursor_pointer()
                        .bg(if props.is_sidebar_open {
                            if theme.mode.is_dark() { rgba(0xffffff26) } else { rgba(0x0000001a) }
                        } else {
                            rgba(0x00000000)
                        })
                        .hover(|h| h.bg(theme.bg_hover))
                        .on_click(move |_, _, cx| t_sb(cx))
                        .child(render_icon("sidebar-toggle", 16.0, theme.text_primary))
                })
        )
        // Center: Attached Document Tabs
        .child(
            div()
                .flex()
                .items_end()
                .gap(px(4.0))
                .flex_1()
                .overflow_x_hidden()
                .h_full()
                .children(props.tabs.into_iter().map(|tab| {
                    let is_active = tab.id == active_id;
                    let tab_id = tab.id.clone();
                    let sel = on_select.clone();
                    let cls = on_close_tab.clone();
                    let tid_for_close = tab.id.clone();

                    let height = if is_active { px(36.0) } else { px(30.0) };
                    let bg = if is_active { theme.bg_toolbar } else { rgba(0x00000000) };
                    let text_color = if is_active {
                        if theme.mode.is_dark() { rgb(0xffffff) } else { rgb(0x0a0a0a) }
                    } else {
                        theme.text_muted
                    };

                    div()
                        .id(SharedString::from(tab_id.clone()))
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .px(px(10.0))
                        .h(height)
                        .rounded_t(px(6.0))
                        .cursor_pointer()
                        .bg(bg)
                        .border_t_2()
                        .border_color(if is_active {
                            if theme.mode.is_dark() { rgb(0xffffff) } else { rgb(0x0a0a0a) }
                        } else {
                            rgba(0x00000000)
                        })
                        .text_color(text_color)
                        .font_weight(if is_active { FontWeight::BOLD } else { FontWeight::NORMAL })
                        .text_size(px(12.0))
                        .hover(|h| {
                            if !is_active {
                                h.bg(theme.bg_hover).text_color(theme.text_primary)
                            } else {
                                h
                            }
                        })
                        .on_click(move |_, _, cx| sel(tab_id.clone(), cx))
                        .child(render_icon("file-text", 14.0, text_color))
                        .child(
                            div()
                                .max_w(px(110.0))
                                .overflow_hidden()
                                .child(tab.title)
                        )
                        .when(tab.is_modified, |s| {
                            s.child(
                                div()
                                    .w(px(6.0))
                                    .h(px(6.0))
                                    .rounded_full()
                                    .bg(if theme.mode.is_dark() { rgb(0xffffff) } else { rgb(0x0a0a0a) })
                            )
                        })
                        .when(tab_count > 1, |s| {
                            s.child(
                                div()
                                    .id(SharedString::from(format!("close-tab-{}", tid_for_close)))
                                    .p(px(2.0))
                                    .rounded(px(2.0))
                                    .cursor_pointer()
                                    .hover(|h| h.bg(theme.bg_active))
                                    .on_click(move |_, _, cx| {
                                        cls(tid_for_close.clone(), cx);
                                    })
                                    .child(render_icon("x", 12.0, theme.text_muted))
                            )
                        })
                }))
                .child({
                    let nt = on_new_tab.clone();
                    div()
                        .id("hb-new-tab")
                        .mb(px(4.0))
                        .p(px(4.0))
                        .rounded(px(4.0))
                        .cursor_pointer()
                        .hover(|h| h.bg(theme.bg_hover))
                        .on_click(move |_, _, cx| nt(cx))
                        .child(render_icon("plus", 14.0, theme.text_muted))
                })
        )
        // Right: Actions & Tools
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(4.0))
                // Quick Save
                .child({
                    let s = on_save.clone();
                    icon_button("hb-save", "save", "Save File (Ctrl+S)", false, theme, move |_, _, cx| s(cx))
                })
                // Open File
                .child({
                    let o = on_open.clone();
                    icon_button("hb-open", "folder-open", "Open File (Ctrl+O)", false, theme, move |_, _, cx| o(cx))
                })
                // Export HTML
                .child({
                    let exp = on_export.clone();
                    icon_button("hb-export", "download", "Export as HTML", false, theme, move |_, _, cx| exp("html", cx))
                })
                // Original PDF (if available)
                .when(props.has_pdf, |s| {
                    let vp = on_view_pdf.clone();
                    s.child(
                        div()
                            .id("hb-view-pdf")
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .px(px(8.0))
                            .py(px(4.0))
                            .rounded(px(4.0))
                            .border_1()
                            .border_color(theme.border_primary)
                            .bg(theme.bg_hover)
                            .text_size(px(11.0))
                            .cursor_pointer()
                            .hover(|h| h.bg(theme.bg_active))
                            .on_click(move |_, _, cx| vp(cx))
                            .child(render_icon("file-text", 12.0, theme.text_primary))
                            .child("Original PDF")
                    )
                })
                // Zoom pill
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(2.0))
                        .px(px(4.0))
                        .py(px(1.0))
                        .rounded(px(4.0))
                        .border_1()
                        .border_color(theme.border_primary)
                        .bg(theme.bg_hover)
                        .child({
                            let zo = on_zoom_out.clone();
                            div()
                                .id("hb-zoom-out")
                                .p(px(2.0))
                                .rounded(px(2.0))
                                .cursor_pointer()
                                .hover(|h| h.bg(theme.bg_active))
                                .on_click(move |_, _, cx| zo(cx))
                                .child(render_icon("zoom-out", 12.0, theme.text_muted))
                        })
                        .child({
                            let rz = on_reset_zoom.clone();
                            div()
                                .id("hb-zoom-reset")
                                .px(px(4.0))
                                .rounded(px(2.0))
                                .text_size(px(11.0))
                                .font_family(Theme::FONT_MONO)
                                .cursor_pointer()
                                .text_color(theme.text_primary)
                                .hover(|h| h.bg(theme.bg_active))
                                .on_click(move |_, _, cx| rz(cx))
                                .child(format!("{}%", props.zoom_level))
                        })
                        .child({
                            let zi = on_zoom_in.clone();
                            div()
                                .id("hb-zoom-in")
                                .p(px(2.0))
                                .rounded(px(2.0))
                                .cursor_pointer()
                                .hover(|h| h.bg(theme.bg_active))
                                .on_click(move |_, _, cx| zi(cx))
                                .child(render_icon("zoom-in", 12.0, theme.text_muted))
                        })
                )
                // Theme toggle
                .child({
                    let tt = on_toggle_theme.clone();
                    let icon = if theme.mode.is_dark() { "sun" } else { "moon" };
                    let tip = if theme.mode.is_dark() { "Switch to Yaru Light" } else { "Switch to Yaru Dark" };
                    icon_button("hb-theme", icon, tip, false, theme, move |_, _, cx| tt(cx))
                })
                // Zen mode
                .child({
                    let tz = on_toggle_zen.clone();
                    icon_button("hb-zen", "maximize-2", "Zen Mode (F11)", false, theme, move |_, _, cx| tz(cx))
                })
                // Help / Shortcuts
                .child({
                    let sc = on_open_shortcuts.clone();
                    icon_button("hb-help", "help-circle", "Keyboard Shortcuts", false, theme, move |_, _, cx| sc(cx))
                })
                // GNOME Window Controls
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .ml(px(6.0))
                        .pl(px(8.0))
                        .border_l_1()
                        .border_color(theme.border_subtle)
                        .child(
                            div()
                                .id("hb-win-min")
                                .w(px(16.0))
                                .h(px(16.0))
                                .rounded_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_size(px(10.0))
                                .text_color(theme.text_muted)
                                .hover(|h| h.bg(theme.bg_active).text_color(theme.text_primary))
                                .child("─")
                        )
                        .child(
                            div()
                                .id("hb-win-max")
                                .w(px(16.0))
                                .h(px(16.0))
                                .rounded_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_size(px(9.0))
                                .text_color(theme.text_muted)
                                .hover(|h| h.bg(theme.bg_active).text_color(theme.text_primary))
                                .child("□")
                        )
                        .child(
                            div()
                                .id("hb-win-close")
                                .w(px(16.0))
                                .h(px(16.0))
                                .rounded_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_size(px(10.0))
                                .text_color(theme.text_muted)
                                .hover(|h| h.bg(theme.bg_active).text_color(theme.text_primary))
                                .child("✕")
                        )
                )
        )
}
