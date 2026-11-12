use super::line_field::LineField;
use super::primitives::{icon_button, render_icon, SimpleTooltip};
use crate::theme::{sz, Theme};
use gpui::prelude::*;
use gpui::*;

pub struct FindReplaceBarProps<FFindNext, FFindPrev, FReplace, FReplaceAll, FClose, FCase, FToggleReplace>
where
    FFindNext: Fn(&mut App) + 'static + Clone,
    FFindPrev: Fn(&mut App) + 'static + Clone,
    FReplace: Fn(&mut App) + 'static + Clone,
    FReplaceAll: Fn(&mut App) + 'static + Clone,
    FClose: Fn(&mut App) + 'static + Clone,
    FCase: Fn(&mut App) + 'static + Clone,
    FToggleReplace: Fn(&mut App) + 'static + Clone,
{
    pub find_field: Entity<LineField>,
    pub replace_field: Entity<LineField>,
    pub match_label: String,
    pub match_case: bool,
    pub not_found: bool,
    /// Second row with the replace field; hidden until asked for.
    pub show_replace: bool,
    pub on_find_next: FFindNext,
    pub on_find_prev: FFindPrev,
    pub on_replace: FReplace,
    pub on_replace_all: FReplaceAll,
    pub on_close: FClose,
    pub on_toggle_case: FCase,
    pub on_toggle_replace: FToggleReplace,
}

/// Compact find panel that floats at the top right of the editor. One search
/// field by default; the chevron on the left reveals the replace row.
pub fn render_find_replace_bar<
    FFindNext,
    FFindPrev,
    FReplace,
    FReplaceAll,
    FClose,
    FCase,
    FToggleReplace,
>(
    props: FindReplaceBarProps<
        FFindNext,
        FFindPrev,
        FReplace,
        FReplaceAll,
        FClose,
        FCase,
        FToggleReplace,
    >,
    theme: &Theme,
) -> impl IntoElement
where
    FFindNext: Fn(&mut App) + 'static + Clone,
    FFindPrev: Fn(&mut App) + 'static + Clone,
    FReplace: Fn(&mut App) + 'static + Clone,
    FReplaceAll: Fn(&mut App) + 'static + Clone,
    FClose: Fn(&mut App) + 'static + Clone,
    FCase: Fn(&mut App) + 'static + Clone,
    FToggleReplace: Fn(&mut App) + 'static + Clone,
{
    let on_find_next = props.on_find_next;
    let on_find_prev = props.on_find_prev;
    let on_replace = props.on_replace;
    let on_replace_all = props.on_replace_all;
    let on_close = props.on_close;
    let on_toggle_case = props.on_toggle_case;
    let on_toggle_replace = props.on_toggle_replace;
    let show_replace = props.show_replace;

    let input_box = |id: &'static str| {
        div()
            .id(id)
            .flex()
            .items_center()
            .gap(px(6.0))
            .h(px(28.0))
            .px(px(8.0))
            .flex_1()
            .min_w_0()
            .rounded(sz::R_MD)
            .bg(theme.bg_input)
            .border_1()
            .border_color(theme.border_subtle)
    };

    div()
        .id("find-replace-panel")
        .absolute()
        .top(sz::TITLE_BAR + sz::TOOLBAR + px(8.0))
        .right(px(24.0))
        .w(px(440.0))
        .flex()
        .flex_col()
        .gap(px(6.0))
        .p(px(8.0))
        .rounded(sz::R_LG)
        .bg(theme.menu_bg)
        .border_1()
        .border_color(theme.border_surface)
        .shadow_lg()
        .text_size(px(12.0))
        // Keep clicks on the panel from reaching the editor underneath.
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(4.0))
                .child({
                    let toggle = on_toggle_replace.clone();
                    icon_button(
                        "fr-toggle-replace",
                        if show_replace { "chevron-down" } else { "chevron-right" },
                        if show_replace { "Hide Replace" } else { "Show Replace (Ctrl+H)" },
                        show_replace,
                        theme,
                        move |_, _, cx| toggle(cx),
                    )
                })
                .child(
                    input_box("fr-find-box")
                        .child(render_icon("search", 14.0, theme.text_muted))
                        .child(div().flex_1().min_w_0().child(props.find_field))
                        .child({
                            let toggle = on_toggle_case.clone();
                            div()
                                .id("fr-match-case")
                                .px(px(5.0))
                                .py(px(1.0))
                                .rounded(sz::R_SM)
                                .cursor_pointer()
                                .text_size(px(11.0))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(if props.match_case {
                                    theme.text_strong
                                } else {
                                    theme.text_muted
                                })
                                .bg(if props.match_case {
                                    theme.row_active
                                } else {
                                    rgba(0x00000000)
                                })
                                .hover(|h| h.bg(theme.bg_hover))
                                .tooltip({
                                    let th = *theme;
                                    move |_, cx| {
                                        cx.new(|_| SimpleTooltip::new("Match case", th)).into()
                                    }
                                })
                                .on_click(move |_, _, cx| toggle(cx))
                                .child("Aa")
                        }),
                )
                .child(
                    div()
                        .min_w(px(52.0))
                        .flex()
                        .justify_center()
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
        .when(show_replace, |s| {
            s.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    // Line up with the find box (the toggle button is 32px wide).
                    .pl(sz::BTN + px(4.0))
                    .child(
                        input_box("fr-replace-box")
                            .child(render_icon("pencil", 14.0, theme.text_muted))
                            .child(div().flex_1().min_w_0().child(props.replace_field)),
                    )
                    .child({
                        let r = on_replace.clone();
                        div()
                            .id("fr-replace-btn")
                            .px(px(10.0))
                            .h(px(28.0))
                            .flex()
                            .items_center()
                            .rounded(sz::R_MD)
                            .bg(theme.bg_hover)
                            .text_color(theme.text_primary)
                            .cursor_pointer()
                            .hover(|h| h.bg(theme.bg_active))
                            .on_click(move |_, _, cx| r(cx))
                            .child("Replace")
                    })
                    .child({
                        let ra = on_replace_all.clone();
                        div()
                            .id("fr-replace-all-btn")
                            .px(px(10.0))
                            .h(px(28.0))
                            .flex()
                            .items_center()
                            .rounded(sz::R_MD)
                            .bg(theme.bg_hover)
                            .text_color(theme.text_primary)
                            .cursor_pointer()
                            .hover(|h| h.bg(theme.bg_active))
                            .on_click(move |_, _, cx| ra(cx))
                            .child("All")
                    }),
            )
        })
}
