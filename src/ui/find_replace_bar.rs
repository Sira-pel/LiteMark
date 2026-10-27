use super::line_field::LineField;
use super::primitives::{icon_button, SimpleTooltip};
use crate::theme::Theme;
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
    pub on_find_next: FFindNext,
    pub on_find_prev: FFindPrev,
    pub on_replace: FReplace,
    pub on_replace_all: FReplaceAll,
    pub on_close: FClose,
    pub on_toggle_case: FCase,
    pub show_replace: bool,
    pub width: Pixels,
    pub on_toggle_replace: FToggleReplace,
}

fn ghost_btn(theme: &Theme, id: &'static str, label: &'static str, f: impl Fn(&mut App) + 'static) -> impl IntoElement {
    div()
        .id(id)
        .px(px(10.0))
        .py(px(4.0))
        .rounded(px(4.0))
        .bg(theme.bg_hover)
        .text_color(theme.text_primary)
        .cursor_pointer()
        .hover(|h| h.bg(theme.bg_active))
        .on_click(move |_, _, cx| f(cx))
        .child(label)
}


pub fn render_find_replace_bar<FFindNext, FFindPrev, FReplace, FReplaceAll, FClose, FCase, FToggleReplace>(
    props: FindReplaceBarProps<FFindNext, FFindPrev, FReplace, FReplaceAll, FClose, FCase, FToggleReplace>,
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
    // Fields shrink with the window so the arrows, toggle and close stay on screen.
    let field_w = (props.width - px(330.0)).clamp(px(110.0), px(300.0));

    let find_row = div()
        .flex()
        .items_center()
        .gap(px(6.0))
        .child(div().w(field_w).child(props.find_field))
        .child(
            div()
                .min_w(px(64.0))
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
            icon_button("fr-prev", "chevron-up", "Previous match (Shift+Enter)", false, theme, move |_, _, cx| f(cx))
        })
        .child({
            let f = on_find_next.clone();
            icon_button("fr-next", "chevron-down", "Next match (Enter)", false, theme, move |_, _, cx| f(cx))
        })
        .child({
            let toggle = on_toggle_case.clone();
            div()
                .id("fr-match-case")
                .px(px(7.0))
                .py(px(4.0))
                .rounded(px(4.0))
                .cursor_pointer()
                .text_size(px(12.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(if props.match_case { theme.text_strong } else { theme.text_muted })
                .when(props.match_case, |d| d.bg(theme.row_active))
                .hover(|h| h.bg(theme.bg_hover))
                .tooltip({
                    let th = *theme;
                    move |_, cx| {
                        cx.new(|_| SimpleTooltip::new("Match case", th)).into()
                    }
                })
                .on_click(move |_, _, cx| toggle(cx))
                .child("Aa")
        });

    let replace_row = div()
        .flex()
        .items_center()
        .gap(px(6.0))
        .child(div().w(field_w).child(props.replace_field))
        .child(ghost_btn(theme, "fr-replace-btn", "Replace", on_replace.clone()))
        .child(ghost_btn(theme, "fr-replace-all-btn", "Replace all", on_replace_all.clone()));

    div()
        .flex()
        .items_start()
        .gap(px(8.0))
        .px(px(12.0))
        .py(px(6.0))
        .bg(theme.bg_surface)
        .border_b_1()
        .border_color(theme.border_primary)
        .text_size(px(12.0))
        .shadow_md()
        .flex_shrink_0()
        .child({
            let t = on_toggle_replace.clone();
            icon_button(
                "fr-toggle-replace",
                if show_replace { "chevron-down" } else { "chevron-right" },
                "Toggle replace (Ctrl+H)",
                show_replace,
                theme,
                move |_, _, cx| t(cx),
            )
        })
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(find_row)
                .when(show_replace, |d| d.child(replace_row)),
        )
        .child(div().flex_1())
        .child({
            let c = on_close.clone();
            icon_button("fr-close", "x", "Close (Escape)", false, theme, move |_, _, cx| c(cx))
        })
}
