use crate::theme::{sz, Theme};
use gpui::prelude::*;
use gpui::*;

pub struct EditorProps {
    pub zoom_level: usize,
    pub show_zoom_hud: bool,
    /// Unified interactive editor surface (handles both WYSIWYG and Raw modes).
    pub editor_element: AnyElement,
    pub scroll: ScrollHandle,
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
                .track_scroll(&props.scroll)
                .w_full()
                .min_w_0()
                .flex_1()
                .size_full()
                .overflow_x_hidden()
                .overflow_y_scroll()
                .child(div().w_full().min_w_0().child(props.editor_element)),
        )
}

/// Vertical scroll change needed to bring a row at `row_top` fully into the
/// viewport. `None` when it is already visible. A row above or below the
/// viewport lands a third of the way down so context stays visible.
pub fn scroll_delta_for_row(
    row_top: Pixels,
    row_height: Pixels,
    view_top: Pixels,
    view_height: Pixels,
) -> Option<Pixels> {
    if row_top >= view_top && row_top + row_height <= view_top + view_height {
        return None;
    }
    Some(row_top - (view_top + view_height / 3.0))
}

#[cfg(test)]
mod tests {
    use super::scroll_delta_for_row;
    use gpui::px;

    #[test]
    fn visible_row_does_not_scroll() {
        assert_eq!(scroll_delta_for_row(px(100.0), px(20.0), px(0.0), px(600.0)), None);
    }

    #[test]
    fn row_below_viewport_scrolls_to_a_third_down() {
        let d = scroll_delta_for_row(px(1500.0), px(20.0), px(50.0), px(600.0)).unwrap();
        assert_eq!(d, px(1500.0 - 250.0));
    }

    #[test]
    fn row_above_viewport_scrolls_up() {
        let d = scroll_delta_for_row(px(-400.0), px(20.0), px(50.0), px(600.0)).unwrap();
        assert!(d < px(0.0));
    }

    #[test]
    fn row_clipped_at_the_bottom_edge_scrolls() {
        assert!(scroll_delta_for_row(px(590.0), px(28.0), px(0.0), px(600.0)).is_some());
    }
}
