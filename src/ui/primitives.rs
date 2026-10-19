use gpui::prelude::*;
use gpui::*;
use crate::theme::Theme;

pub fn icon_path(name: &str) -> SharedString {
    format!("icons/{}.svg", name).into()
}

pub fn render_icon(name: &str, size_px: f32, color: Rgba) -> impl IntoElement {
    svg()
        .path(icon_path(name))
        .size(px(size_px))
        .text_color(color)
}

pub fn icon_button<F>(
    id: impl Into<ElementId>,
    icon_name: &'static str,
    _tooltip: &'static str,
    is_active: bool,
    theme: &Theme,
    on_click: F,
) -> impl IntoElement
where
    F: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
{
    let base_color = if is_active {
        if theme.mode.is_dark() {
            rgb(0x0a0a0a)
        } else {
            rgb(0xffffff)
        }
    } else {
        theme.text_primary
    };

    let base_bg = if is_active {
        if theme.mode.is_dark() {
            rgb(0xf5f5f5)
        } else {
            rgb(0x171717)
        }
    } else {
        rgba(0x00000000)
    };

    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .w(px(28.0))
        .h(px(28.0))
        .rounded(px(4.0))
        .cursor_pointer()
        .bg(base_bg)
        .when(!is_active, |s| {
            s.hover(|h| h.bg(theme.bg_hover))
                .active(|a| a.bg(theme.bg_active))
        })
        .on_click(on_click)
        .child(render_icon(icon_name, 16.0, base_color))
}

pub fn divider(theme: &Theme) -> impl IntoElement {
    div()
        .w(px(1.0))
        .h(px(20.0))
        .bg(theme.border_subtle)
        .mx(px(4.0))
        .flex_shrink_0()
}

pub fn kbd_badge(label: &'static str, theme: &Theme) -> impl IntoElement {
    div()
        .px(px(6.0))
        .py(px(2.0))
        .rounded(px(4.0))
        .bg(if theme.mode.is_dark() { rgb(0x2b2b2b) } else { rgb(0xffffff) })
        .border_1()
        .border_color(if theme.mode.is_dark() { rgb(0x444444) } else { rgb(0xd4d4d4) })
        .text_color(if theme.mode.is_dark() { rgb(0xdfdbd2) } else { rgb(0x171717) })
        .font_family(Theme::FONT_MONO)
        .text_size(px(11.0))
        .font_weight(FontWeight::SEMIBOLD)
        .child(label)
}
