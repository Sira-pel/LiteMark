use crate::theme::{sz, Theme};
use gpui::prelude::*;
use gpui::*;

pub struct EditorProps {
    pub zoom_level: usize,
    pub show_zoom_hud: bool,
    /// Unified interactive editor surface (handles both WYSIWYG and Raw modes).
    pub editor_element: AnyElement,
}

pub fn render_editor(props: EditorProps, theme: &Theme) -> impl IntoElement {
    div()
        .relative()
        .flex_1()
        .flex()
        .size_full()
        .overflow_hidden()
        .bg(theme.bg_editor)
        .text_color(theme.text_primary)
        // Zoom HUD floating indicator
        .when(props.show_zoom_hud, |s| {
            s.child(
                div()
                    .absolute()
                    .bottom(px(16.0))
                    .right(px(24.0))
                    .px(px(10.0))
                    .py(px(4.0))
                    .rounded(sz::R_MD)
                    .bg(theme.menu_bg)
                    .border_1()
                    .border_color(theme.border_surface)
                    .shadow_lg()
                    .text_size(px(11.0))
                    .font_family(Theme::FONT_MONO)
                    .font_weight(FontWeight::BOLD)
                    .text_color(theme.text_primary)
                    .child(format!("Zoom {}%", props.zoom_level)),
            )
        })
        .child(
            div()
                .id("editor-scroll")
                .w_full()
                .min_w_0()
                .flex_1()
                .size_full()
                .overflow_x_hidden()
                .overflow_y_scroll()
                .child(div().w_full().min_w_0().child(props.editor_element)),
        )
}
