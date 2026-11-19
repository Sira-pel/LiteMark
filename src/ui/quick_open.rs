//! The Ctrl+P pop-up: a search field with a Notes | Contents toggle, the
//! matching notes or lines below it, and a footer of key hints.

use super::line_field::LineField;
use super::primitives::{kbd_badge, render_icon};
use crate::model::quick_open::{ContentHit, NoteHit, QuickMode};
use crate::theme::Theme;
use gpui::prelude::*;
use gpui::*;
use std::ops::Range;
use std::sync::Arc;

pub type PickFn = Arc<dyn Fn(usize, &mut App)>;
pub type ModeFn = Arc<dyn Fn(QuickMode, &mut App)>;

pub struct QuickOpenProps<'a> {
    pub mode: QuickMode,
    pub field: Entity<LineField>,
    pub notes: &'a [NoteHit],
    pub contents: &'a [ContentHit],
    pub query_empty: bool,
    pub searching: bool,
    pub selected: usize,
    pub scroll: ScrollHandle,
    pub list_max_h: Pixels,
    pub on_close: Arc<dyn Fn(&mut App)>,
    pub on_pick: PickFn,
    pub on_mode: ModeFn,
}

/// `text` with the `ranges` drawn bold and bright.
fn highlighted(text: &str, ranges: &[Range<usize>], base: Rgba, bright: Rgba) -> impl IntoElement {
    let style = HighlightStyle {
        color: Some(bright.into()),
        font_weight: Some(FontWeight::BOLD),
        ..Default::default()
    };
    let runs = ranges.iter().map(|range| (range.clone(), style));
    div()
        .min_w_0()
        .truncate()
        .text_color(base)
        .child(StyledText::new(text.to_string()).with_highlights(runs))
}

fn mode_button(
    id: &'static str,
    label: &'static str,
    mode: QuickMode,
    active: bool,
    on_mode: ModeFn,
    theme: &Theme,
) -> impl IntoElement {
    div()
        .id(id)
        .px(px(10.0))
        .py(px(3.0))
        .rounded(px(5.0))
        .cursor_pointer()
        .text_size(px(12.0))
        .when(active, |s| {
            s.bg(theme.row_active)
                .text_color(theme.text_strong)
                .font_weight(FontWeight::SEMIBOLD)
        })
        .when(!active, |s| {
            s.text_color(theme.text_muted)
                .hover(|h| h.text_color(theme.text_primary))
        })
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(move |_, _, cx| on_mode(mode, cx))
        .child(label)
}

fn hint(key: &str, what: &str, theme: &Theme) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap(px(5.0))
        .child(kbd_badge(key, theme))
        .child(
            div()
                .text_size(px(11.0))
                .text_color(theme.text_faint)
                .child(what.to_string()),
        )
}

fn message(text: &str, theme: &Theme) -> impl IntoElement {
    div()
        .px(px(16.0))
        .py(px(18.0))
        .text_size(px(13.0))
        .text_color(theme.text_faint)
        .child(text.to_string())
}

fn row_shell(ix: usize, selected: bool, on_pick: PickFn, theme: &Theme) -> Stateful<Div> {
    div()
        .id(("quick-open-row", ix))
        .px(px(14.0))
        .py(px(6.0))
        .mx(px(6.0))
        .rounded(px(6.0))
        .cursor_pointer()
        .when(selected, |s| s.bg(theme.row_active))
        .when(!selected, |s| s.hover(|h| h.bg(theme.row_hover)))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(move |_, _, cx| on_pick(ix, cx))
}

fn note_row(
    ix: usize,
    hit: &NoteHit,
    selected: bool,
    on_pick: PickFn,
    theme: &Theme,
) -> impl IntoElement {
    let base = if selected {
        theme.text_strong
    } else {
        theme.text_primary
    };
    row_shell(ix, selected, on_pick, theme)
        .flex()
        .items_center()
        .gap(px(12.0))
        .text_size(px(13.0))
        .child(render_icon("file-text", 14.0, theme.text_muted))
        .child(div().flex_1().min_w_0().child(highlighted(
            &hit.name,
            &hit.name_hits,
            base,
            theme.text_strong,
        )))
        .child(
            div()
                .max_w(px(220.0))
                .min_w_0()
                .text_size(px(12.0))
                .child(highlighted(
                    &hit.folder,
                    &hit.folder_hits,
                    theme.text_faint,
                    theme.text_primary,
                )),
        )
}

fn content_row(
    ix: usize,
    hit: &ContentHit,
    selected: bool,
    on_pick: PickFn,
    theme: &Theme,
) -> impl IntoElement {
    let base = if selected {
        theme.text_strong
    } else {
        theme.text_primary
    };
    row_shell(ix, selected, on_pick, theme)
        .flex()
        .flex_col()
        .gap(px(2.0))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(10.0))
                .text_size(px(13.0))
                .child(render_icon("file-text", 14.0, theme.text_muted))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_color(base)
                        .font_weight(FontWeight::MEDIUM)
                        .child(hit.name.clone()),
                )
                .child(
                    div()
                        .max_w(px(220.0))
                        .min_w_0()
                        .truncate()
                        .text_size(px(12.0))
                        .text_color(theme.text_faint)
                        .child(hit.folder.clone()),
                ),
        )
        .child(
            div()
                .flex()
                .items_baseline()
                .gap(px(10.0))
                .pl(px(24.0))
                .text_size(px(12.0))
                .child(
                    div()
                        .w(px(32.0))
                        .flex_shrink_0()
                        .text_color(theme.text_faint)
                        .font_family(Theme::FONT_MONO)
                        .child(hit.line.to_string()),
                )
                .child(div().flex_1().min_w_0().child(highlighted(
                    &hit.snippet,
                    &hit.snippet_hits,
                    theme.text_muted,
                    theme.text_strong,
                ))),
        )
}

pub fn render_quick_open(props: QuickOpenProps<'_>, theme: &Theme) -> impl IntoElement {
    let QuickOpenProps {
        mode,
        field,
        notes,
        contents,
        query_empty,
        searching,
        selected,
        scroll,
        list_max_h,
        on_close,
        on_pick,
        on_mode,
    } = props;

    let rows: Vec<AnyElement> = match mode {
        QuickMode::Notes => notes
            .iter()
            .enumerate()
            .map(|(ix, hit)| {
                note_row(ix, hit, ix == selected, on_pick.clone(), theme).into_any_element()
            })
            .collect(),
        QuickMode::Contents => contents
            .iter()
            .enumerate()
            .map(|(ix, hit)| {
                content_row(ix, hit, ix == selected, on_pick.clone(), theme).into_any_element()
            })
            .collect(),
    };
    let empty_text = match (mode, query_empty) {
        (_, _) if !rows.is_empty() => None,
        (QuickMode::Notes, true) => Some("No recent notes yet. Type to find a note by name."),
        (QuickMode::Notes, false) => Some("No notes match."),
        (QuickMode::Contents, true) => Some("Type to search inside your notes."),
        (QuickMode::Contents, false) if searching => Some("Searching…"),
        (QuickMode::Contents, false) => Some("No matches."),
    };
    let heading = (mode == QuickMode::Notes && query_empty && !rows.is_empty()).then_some("Recent");

    let backdrop_close = on_close.clone();
    div()
        .id("quick-open-backdrop")
        .absolute()
        .inset_0()
        .bg(rgba(0x00000099))
        .flex()
        .flex_col()
        .items_center()
        .pt(px(72.0))
        .px(px(16.0))
        .occlude()
        .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
        .on_mouse_down(MouseButton::Left, move |_, _, cx| backdrop_close(cx))
        .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
        .child(
            div()
                .id("quick-open-card")
                .key_context("QuickOpen")
                .flex()
                .flex_col()
                .w(px(640.0))
                .max_w_full()
                .rounded(px(12.0))
                .bg(theme.bg_modal)
                .border_1()
                .border_color(theme.border_surface)
                .shadow_2xl()
                .overflow_hidden()
                .occlude()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
                // Search field and mode toggle
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(10.0))
                        .px(px(16.0))
                        .py(px(10.0))
                        .bg(theme.bg_modal_header)
                        .border_b_1()
                        .border_color(theme.border_subtle)
                        .child(render_icon("search", 16.0, theme.text_muted))
                        .child(field)
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .flex_shrink_0()
                                .gap(px(2.0))
                                .p(px(2.0))
                                .rounded(px(7.0))
                                .bg(theme.bg_inset)
                                .border_1()
                                .border_color(theme.border_subtle)
                                .child(mode_button(
                                    "quick-open-mode-notes",
                                    "Notes",
                                    QuickMode::Notes,
                                    mode == QuickMode::Notes,
                                    on_mode.clone(),
                                    theme,
                                ))
                                .child(mode_button(
                                    "quick-open-mode-contents",
                                    "Contents",
                                    QuickMode::Contents,
                                    mode == QuickMode::Contents,
                                    on_mode.clone(),
                                    theme,
                                )),
                        ),
                )
                .when_some(heading, |s, text| {
                    s.child(
                        div()
                            .px(px(20.0))
                            .pt(px(10.0))
                            .pb(px(2.0))
                            .text_size(px(11.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.text_faint)
                            .child(text),
                    )
                })
                // Results
                .child(
                    div()
                        .id("quick-open-list")
                        .flex()
                        .flex_col()
                        .max_h(list_max_h)
                        .py(px(6.0))
                        .overflow_y_scroll()
                        .track_scroll(&scroll)
                        .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                        .children(rows)
                        .when_some(empty_text, |s, text| s.child(message(text, theme))),
                )
                // Key hints
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(16.0))
                        .px(px(16.0))
                        .py(px(8.0))
                        .bg(theme.bg_modal_header)
                        .border_t_1()
                        .border_color(theme.border_subtle)
                        .child(hint("Tab", "notes / contents", theme))
                        .child(hint("↑↓", "move", theme))
                        .child(hint("Enter", "open", theme))
                        .child(hint("Esc", "close", theme)),
                ),
        )
}
