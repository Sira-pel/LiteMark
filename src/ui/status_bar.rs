use gpui::prelude::*;
use gpui::*;
use crate::model::ViewMode;
use crate::theme::{sz, Theme};
use super::primitives::render_icon;

pub struct StatusBarProps<FZoomIn, FZoomOut, FResetZoom, FToggleMode>
where
    FZoomIn: Fn(&mut App) + 'static + Clone,
    FZoomOut: Fn(&mut App) + 'static + Clone,
    FResetZoom: Fn(&mut App) + 'static + Clone,
    FToggleMode: Fn(&mut App) + 'static + Clone,
{
    pub line: usize,
    pub words: usize,
    pub chars: usize,
    /// Words in the current selection, when there is one.
    pub selected_words: Option<usize>,
    pub width: Pixels,
    pub zoom_level: usize,
    pub view_mode: ViewMode,
    pub on_zoom_in: FZoomIn,
    pub on_zoom_out: FZoomOut,
    pub on_reset_zoom: FResetZoom,
    pub on_toggle_mode: FToggleMode,
}

pub fn render_status_bar<FZoomIn, FZoomOut, FResetZoom, FToggleMode>(
    props: StatusBarProps<FZoomIn, FZoomOut, FResetZoom, FToggleMode>,
    theme: &Theme,
) -> impl IntoElement
where
    FZoomIn: Fn(&mut App) + 'static + Clone,
    FZoomOut: Fn(&mut App) + 'static + Clone,
    FResetZoom: Fn(&mut App) + 'static + Clone,
    FToggleMode: Fn(&mut App) + 'static + Clone,
{
    let on_zoom_in = props.on_zoom_in;
    let on_zoom_out = props.on_zoom_out;
    let on_reset_zoom = props.on_reset_zoom;
    let on_toggle_mode = props.on_toggle_mode;
    // Drop the least important items first as the window narrows.
    let show_encoding = props.width >= px(560.0);
    let show_chars = props.width >= px(460.0);
    let show_words = props.width >= px(340.0);

    let divider = || div().w(px(1.0)).h(px(14.0)).bg(theme.border_primary);
    let text_item = |label: &'static str| div().px(px(8.0)).child(label);
    let mode_icon = match props.view_mode {
        ViewMode::Wysiwyg => "file-text",
        ViewMode::Raw => "code",
    };

    div()
        .flex()
        .items_center()
        .justify_between()
        .h(sz::STATUS)
        .pl(px(14.0))
        .pr(px(6.0))
        .bg(theme.bg_editor)
        .border_t_1()
        .border_color(theme.border_primary)
        .text_color(theme.text_muted)
        .text_size(px(12.0))
        .font_family(Theme::FONT_PROSE)
        .flex_shrink_0()
        // Left document metrics
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(format!("Ln {}", props.line))
                .when(show_chars, |d| {
                    d.child(div().text_color(theme.text_faint).child("·"))
                        .child(format!("{} chars", props.chars))
                })
                .when(show_words, |d| {
                    d.child(div().text_color(theme.text_faint).child("·"))
                        .child(match props.selected_words {
                            Some(words) => format!("{} of {} words", words, props.words),
                            None => format!("{} words", props.words),
                        })
                })
        )
        // Right controls
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(4.0))
                .when(show_encoding, |d| {
                    d.child(text_item("UTF-8"))
                        .child(text_item("LF"))
                        .child(divider())
                })
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(2.0))
                        .child({
                            let z_out = on_zoom_out.clone();
                            div()
                                .id("sb-zoom-out")
                                .p(px(3.0))
                                .rounded(sz::R_SM)
                                .cursor_pointer()
                                .hover(|h| h.bg(theme.bg_hover).text_color(theme.text_primary))
                                .on_click(move |_, _, cx| z_out(cx))
                                .child(render_icon("zoom-out", 13.0, theme.text_muted))
                        })
                        .child({
                            let z_reset = on_reset_zoom.clone();
                            div()
                                .id("sb-zoom-reset")
                                .px(px(4.0))
                                .py(px(2.0))
                                .rounded(sz::R_SM)
                                .cursor_pointer()
                                .text_color(if props.zoom_level != 100 { theme.text_hi } else { theme.text_muted })
                                .hover(|h| h.bg(theme.bg_hover).text_color(theme.text_primary))
                                .on_click(move |_, _, cx| z_reset(cx))
                                .child(format!("{}%", props.zoom_level))
                        })
                        .child({
                            let z_in = on_zoom_in.clone();
                            div()
                                .id("sb-zoom-in")
                                .p(px(3.0))
                                .rounded(sz::R_SM)
                                .cursor_pointer()
                                .hover(|h| h.bg(theme.bg_hover).text_color(theme.text_primary))
                                .on_click(move |_, _, cx| z_in(cx))
                                .child(render_icon("zoom-in", 13.0, theme.text_muted))
                        })
                )
                .child(divider())
                // WYSIWYG / Raw mode toggle
                .child({
                    let t_mode = on_toggle_mode.clone();
                    div()
                        .id("sb-toggle-view-mode")
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .px(px(8.0))
                        .py(px(3.0))
                        .rounded(sz::R_SM)
                        .cursor_pointer()
                        .hover(|h| h.bg(theme.bg_hover).text_color(theme.text_primary))
                        .on_click(move |_, _, cx| t_mode(cx))
                        .child(render_icon(mode_icon, 13.0, theme.text_muted))
                        .child(props.view_mode.label())
                })
        )
}
