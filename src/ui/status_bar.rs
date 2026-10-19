use gpui::prelude::*;
use gpui::*;
use crate::model::ViewMode;
use crate::theme::Theme;
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
        .h(px(28.0))
        .px(px(8.0))
        .bg(theme.bg_editor)
        .border_t_1()
        .border_color(theme.border_subtle)
        .text_color(theme.text_muted)
        .text_size(px(11.0))
        .font_family(Theme::FONT_MONO)
        .flex_shrink_0()
        // Left document metrics
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(format!("Ln {}, Col {}", props.line, props.col))
                .child("·")
                .child(format!("{}w", props.words))
                .child("·")
                .child(format!("{} lines", props.lines))
                .child("·")
                .child(format!("{} chars", props.chars))
                .child("·")
                .child(format!("~{}m read", props.reading_time_min))
        )
        // Right controls
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                // Zoom controls
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
                            let z_out = on_zoom_out.clone();
                            div()
                                .id("sb-zoom-out")
                                .p(px(2.0))
                                .rounded(px(2.0))
                                .cursor_pointer()
                                .hover(|h| h.text_color(theme.text_primary))
                                .on_click(move |_, _, cx| z_out(cx))
                                .child(render_icon("zoom-out", 12.0, theme.text_muted))
                        })
                        .child({
                            let z_reset = on_reset_zoom.clone();
                            div()
                                .id("sb-zoom-reset")
                                .px(px(4.0))
                                .cursor_pointer()
                                .font_weight(if props.zoom_level != 100 { FontWeight::BOLD } else { FontWeight::NORMAL })
                                .text_color(if props.zoom_level != 100 { theme.text_primary } else { theme.text_muted })
                                .hover(|h| h.text_color(theme.text_primary))
                                .on_click(move |_, _, cx| z_reset(cx))
                                .child(format!("{}%", props.zoom_level))
                        })
                        .child({
                            let z_in = on_zoom_in.clone();
                            div()
                                .id("sb-zoom-in")
                                .p(px(2.0))
                                .rounded(px(2.0))
                                .cursor_pointer()
                                .hover(|h| h.text_color(theme.text_primary))
                                .on_click(move |_, _, cx| z_in(cx))
                                .child(render_icon("zoom-in", 12.0, theme.text_muted))
                        })
                )
                .child("·")
                .child("UTF-8")
                .child("·")
                .child("LF")
                .child("·")
                // WYSIWYG / Raw toggle button
                .child({
                    let t_mode = on_toggle_mode.clone();
                    let is_wysiwyg = props.view_mode == ViewMode::Wysiwyg;
                    let bg = if is_wysiwyg {
                        if theme.mode.is_dark() { rgb(0xf5f5f5) } else { rgb(0x171717) }
                    } else {
                        theme.bg_hover
                    };
                    let text_color = if is_wysiwyg {
                        if theme.mode.is_dark() { rgb(0x0a0a0a) } else { rgb(0xffffff) }
                    } else {
                        theme.text_primary
                    };

                    div()
                        .id("sb-toggle-view-mode")
                        .px(px(8.0))
                        .py(px(2.0))
                        .rounded(px(4.0))
                        .bg(bg)
                        .text_color(text_color)
                        .font_weight(if is_wysiwyg { FontWeight::BOLD } else { FontWeight::MEDIUM })
                        .cursor_pointer()
                        .on_click(move |_, _, cx| t_mode(cx))
                        .child(props.view_mode.label())
                })
        )
}
