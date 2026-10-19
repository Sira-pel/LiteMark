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
    pub col: usize,
    pub words: usize,
    pub chars: usize,
    pub lines: usize,
    pub reading_time_min: usize,
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

    div()
        .flex()
        .items_center()
        .justify_between()
        .h(sz::STATUS)
        .px(px(10.0))
        .bg(theme.bg_editor)
        .border_t_1()
        .border_color(theme.border_primary)
        .text_color(theme.text_muted)
        .text_size(px(11.5))
        .font_family(Theme::FONT_MONO)
        .flex_shrink_0()
        // Left document metrics
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(format!("Ln {}, Col {}", props.line, props.col))
                .child(
                    div()
                        .text_color(theme.text_faint)
                        .child("·")
                )
                .child(format!("{}w", props.words))
                .child(
                    div()
                        .text_color(theme.text_faint)
                        .child("·")
                )
                .child(format!("{} lines", props.lines))
                .child(
                    div()
                        .text_color(theme.text_faint)
                        .child("·")
                )
                .child(format!("{} chars", props.chars))
                .child(
                    div()
                        .text_color(theme.text_faint)
                        .child("·")
                )
                .child(format!("~{}m read", props.reading_time_min))
        )
        // Right controls
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                // Zoom chip in menu_bg
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(2.0))
                        .px(px(6.0))
                        .py(px(2.0))
                        .rounded(sz::R_SM)
                        .border_1()
                        .border_color(theme.border_primary)
                        .bg(theme.menu_bg)
                        .child({
                            let z_out = on_zoom_out.clone();
                            div()
                                .id("sb-zoom-out")
                                .p(px(2.0))
                                .rounded(sz::R_SM)
                                .cursor_pointer()
                                .hover(|h| h.bg(theme.bg_hover).text_color(theme.text_primary))
                                .on_click(move |_, _, cx| z_out(cx))
                                .child(render_icon("zoom-out", 12.0, theme.text_muted))
                        })
                        .child({
                            let z_reset = on_reset_zoom.clone();
                            div()
                                .id("sb-zoom-reset")
                                .px(px(4.0))
                                .cursor_pointer()
                                .text_color(if props.zoom_level != 100 { theme.text_hi } else { theme.text_muted })
                                .hover(|h| h.text_color(theme.text_primary))
                                .on_click(move |_, _, cx| z_reset(cx))
                                .child(format!("{}%", props.zoom_level))
                        })
                        .child({
                            let z_in = on_zoom_in.clone();
                            div()
                                .id("sb-zoom-in")
                                .p(px(2.0))
                                .rounded(sz::R_SM)
                                .cursor_pointer()
                                .hover(|h| h.bg(theme.bg_hover).text_color(theme.text_primary))
                                .on_click(move |_, _, cx| z_in(cx))
                                .child(render_icon("zoom-in", 12.0, theme.text_muted))
                        })
                )
                .child(
                    div()
                        .text_color(theme.text_faint)
                        .child("·")
                )
                .child("UTF-8")
                .child(
                    div()
                        .text_color(theme.text_faint)
                        .child("·")
                )
                .child("LF")
                .child(
                    div()
                        .text_color(theme.text_faint)
                        .child("·")
                )
                // WYSIWYG / Raw toggle pill
                .child({
                    let t_mode = on_toggle_mode.clone();
                    div()
                        .id("sb-toggle-view-mode")
                        .px(px(8.0))
                        .py(px(2.0))
                        .rounded(sz::R_SM)
                        .border_1()
                        .border_color(theme.pill_border)
                        .bg(theme.tile_hover)
                        .text_color(theme.pill_text)
                        .cursor_pointer()
                        .hover(|h| h.bg(theme.bg_hover).text_color(theme.text_primary))
                        .on_click(move |_, _, cx| t_mode(cx))
                        .child(props.view_mode.label())
                })
        )
}
