use super::line_field::LineField;
use super::primitives::icon_button;
use crate::theme::Theme;
use gpui::prelude::*;
use gpui::*;

pub struct FindReplaceBarProps<FFindNext, FFindPrev, FReplace, FReplaceAll, FClose, FCase>
where
    FFindNext: Fn(&mut App) + 'static + Clone,
    FFindPrev: Fn(&mut App) + 'static + Clone,
    FReplace: Fn(&mut App) + 'static + Clone,
    FReplaceAll: Fn(&mut App) + 'static + Clone,
    FClose: Fn(&mut App) + 'static + Clone,
    FCase: Fn(&mut App) + 'static + Clone,
{
    pub find_field: Entity<LineField>,
    pub replace_field: Entity<LineField>,
    pub match_label: String,
    pub match_case: bool,
    pub not_found: bool,
    pub on_find_next: FFindNext,
    pub on_find_prev: FFindPrev,
    pub on_replace: FReplace,
    pub on_replace_all: FReplaceAll,
    pub on_close: FClose,
    pub on_toggle_case: FCase,
}

pub fn render_find_replace_bar<FFindNext, FFindPrev, FReplace, FReplaceAll, FClose, FCase>(
    props: FindReplaceBarProps<FFindNext, FFindPrev, FReplace, FReplaceAll, FClose, FCase>,
    theme: &Theme,
) -> impl IntoElement
where
    FFindNext: Fn(&mut App) + 'static + Clone,
    FFindPrev: Fn(&mut App) + 'static + Clone,
    FReplace: Fn(&mut App) + 'static + Clone,
    FReplaceAll: Fn(&mut App) + 'static + Clone,
    FClose: Fn(&mut App) + 'static + Clone,
    FCase: Fn(&mut App) + 'static + Clone,
{
    let on_find_next = props.on_find_next;
    let on_find_prev = props.on_find_prev;
    let on_replace = props.on_replace;
    let on_replace_all = props.on_replace_all;
    let on_close = props.on_close;
    let on_toggle_case = props.on_toggle_case;

    div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .px(px(12.0))
        .py(px(6.0))
        .bg(theme.bg_surface)
        .border_b_1()
        .border_color(theme.border_primary)
        .text_size(px(12.0))
        .shadow_md()
        .flex_shrink_0()
        // Search group
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .flex_1()
                .child(props.find_field)
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(if props.not_found {
                            theme.text_strong
                        } else {
                            theme.text_muted
                        })
                        .child(props.match_label),
                )
                .child({
                    let f = on_find_prev.clone();
                    icon_button(
                        "fr-prev",
                        "chevron-up",
                        "Previous Match (Shift+Enter)",
                        false,
                        theme,
                        move |_, _, cx| f(cx),
                    )
                })
                .child({
                    let f = on_find_next.clone();
                    icon_button(
                        "fr-next",
                        "chevron-down",
                        "Next Match (Enter)",
                        false,
                        theme,
                        move |_, _, cx| f(cx),
                    )
                })
                .child({
                    let toggle = on_toggle_case.clone();
                    let label = if props.match_case { "[x] Aa" } else { "[ ] Aa" };
                    div()
                        .id("fr-match-case")
                        .px(px(6.0))
                        .py(px(4.0))
                        .rounded(px(4.0))
                        .cursor_pointer()
                        .text_color(theme.text_muted)
                        .text_size(px(11.0))
                        .hover(|h| h.bg(theme.bg_hover))
                        .on_click(move |_, _, cx| toggle(cx))
                        .child(label)
                }),
        )
        // Replace group
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .flex_1()
                .child(props.replace_field)
                .child({
                    let r = on_replace.clone();
                    div()
                        .id("fr-replace-btn")
                        .px(px(10.0))
                        .py(px(4.0))
                        .rounded(px(4.0))
                        .bg(theme.bg_hover)
                        .text_color(theme.text_primary)
                        .cursor_pointer()
                        .font_weight(FontWeight::MEDIUM)
                        .hover(|h| h.bg(theme.bg_active))
                        .on_click(move |_, _, cx| r(cx))
                        .child("Replace")
                })
                .child({
                    let ra = on_replace_all.clone();
                    div()
                        .id("fr-replace-all-btn")
                        .px(px(10.0))
                        .py(px(4.0))
                        .rounded(px(4.0))
                        .bg(if theme.mode.is_dark() {
                            rgb(0xf5f5f5)
                        } else {
                            rgb(0x171717)
                        })
                        .text_color(if theme.mode.is_dark() {
                            rgb(0x0a0a0a)
                        } else {
                            rgb(0xffffff)
                        })
                        .cursor_pointer()
                        .font_weight(FontWeight::BOLD)
                        .hover(|h| h.bg(theme.bg_active))
                        .on_click(move |_, _, cx| ra(cx))
                        .child("All")
                })
                .child({
                    let c = on_close.clone();
                    icon_button(
                        "fr-close",
                        "x",
                        "Close (Escape)",
                        false,
                        theme,
                        move |_, _, cx| c(cx),
                    )
                }),
        )
}
