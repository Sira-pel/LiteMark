use gpui::prelude::*;
use gpui::*;
use crate::theme::{sz, Theme};

pub fn icon_path(name: &str) -> SharedString {
    format!("icons/{}.svg", name).into()
}

pub fn render_icon(name: &str, size_px: f32, color: Rgba) -> impl IntoElement {
    svg()
        .path(icon_path(name))
        .size(px(size_px))
        .text_color(color)
}

pub struct SimpleTooltip {
    text: &'static str,
    theme: Theme,
}

impl SimpleTooltip {
    pub fn new(text: &'static str, theme: Theme) -> Self {
        Self { text, theme }
    }
}

impl Render for SimpleTooltip {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px(px(8.0))
            .py(px(4.0))
            .rounded(sz::R_MD)
            .bg(self.theme.menu_bg)
            .border_1()
            .border_color(self.theme.border_surface)
            .shadow_md()
            .text_size(px(11.0))
            .text_color(self.theme.text_primary)
            .child(self.text)
    }
}

pub fn icon_button<F>(
    id: impl Into<ElementId>,
    icon_name: &'static str,
    tooltip: &'static str,
    is_active: bool,
    theme: &Theme,
    on_click: F,
) -> impl IntoElement
where
    F: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
{
    let base_color = if is_active {
        theme.text_strong
    } else {
        theme.text_muted
    };

    let base_bg = if is_active {
        theme.row_active
    } else {
        rgba(0x00000000)
    };

    let el = div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .w(sz::BTN)
        .h(sz::BTN)
        .rounded(sz::R_MD)
        .cursor_pointer()
        .bg(base_bg)
        .when(!is_active, |s| {
            s.hover(|h| h.bg(theme.bg_hover).text_color(theme.text_primary))
                .active(|a| a.bg(theme.bg_active))
        })
        .on_click(on_click)
        .child(render_icon(icon_name, 16.0, base_color));

    if !tooltip.is_empty() {
        let th = *theme;
        el.tooltip(move |_, cx| {
            cx.new(|_| SimpleTooltip { text: tooltip, theme: th }).into()
        })
    } else {
        el
    }
}

pub const DIVIDER_W: f32 = 1.0;
pub const DIVIDER_MX: f32 = 6.0;
pub const DIVIDER_TOTAL_W: f32 = DIVIDER_W + (DIVIDER_MX * 2.0);

pub fn divider(theme: &Theme) -> impl IntoElement {
    div()
        .w(px(DIVIDER_W))
        .h(px(16.0))
        .bg(theme.border_subtle)
        .mx(px(DIVIDER_MX))
        .flex_shrink_0()
}

pub fn kbd_badge(label: &'static str, theme: &Theme) -> impl IntoElement {
    div()
        .px(px(6.0))
        .py(px(2.0))
        .rounded(sz::R_SM)
        .bg(theme.tile_hover)
        .border_1()
        .border_color(theme.border_subtle)
        .text_color(theme.text_faint)
        .font_family(Theme::FONT_MONO)
        .text_size(px(11.0))
        .font_weight(FontWeight::SEMIBOLD)
        .child(label)
}
