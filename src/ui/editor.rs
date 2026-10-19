use crate::markdown::{parse_markdown, source_offset_for_click, Block, Span};
use crate::model::settings::ContentAlignment;
use crate::model::{text_ops::wysiwyg_list_row_height, ViewMode};
use crate::theme::{sz, Theme};

pub type WysiwygPlace = Arc<dyn for<'a, 'b> Fn(usize, &'a mut Window, &'b mut App) + 'static>;
pub type TaskToggle = Arc<dyn for<'a, 'b> Fn(std::ops::Range<usize>, &'a mut Window, &'b mut App) + 'static>;
pub type TableEdit = Arc<
    dyn for<'a, 'b> Fn(
            std::ops::Range<usize>,
            Vec<String>,
            Vec<Vec<String>>,
            &'a mut Window,
            &'b mut App,
        ) + 'static,
>;

use gpui::prelude::*;
use gpui::*;
use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

pub struct EditorProps {
    pub content: String,
    pub view_mode: ViewMode,
    pub zoom_level: usize,
    pub base_font_size: f32,
    pub content_alignment: ContentAlignment,
    pub show_zoom_hud: bool,
    /// Interactive raw editor surface.
    pub raw_editor: AnyElement,
    /// Invisible input surface for keystroke handling in WYSIWYG mode.
    pub wysiwyg_input: AnyElement,
    pub wysiwyg_bounds: Rc<RefCell<Vec<Option<Bounds<Pixels>>>>>,
    pub on_wysiwyg_place: WysiwygPlace,
    pub caret: Option<Range<usize>>,
    pub on_toggle_task: TaskToggle,
    pub on_table_edit: TableEdit,
}

pub fn render_editor(props: EditorProps, theme: &Theme) -> impl IntoElement {
    let zoom_factor = props.zoom_level as f32 / 100.0;
    let base_size = props.base_font_size * zoom_factor;
    let is_wysiwyg = props.view_mode == ViewMode::Wysiwyg;

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
        .child(if is_wysiwyg {
            div()
                .flex_1()
                .size_full()
                .overflow_hidden()
                .relative()
                .child(
                    div()
                        .absolute()
                        .size_full()
                        .child(props.wysiwyg_input),
                )
                .child(render_wysiwyg_view(
                    &props.content,
                    base_size,
                    zoom_factor,
                    theme,
                    props.wysiwyg_bounds.clone(),
                    props.on_wysiwyg_place.clone(),
                    props.content_alignment,
                    props.caret.clone(),
                    props.on_toggle_task.clone(),
                    props.on_table_edit.clone(),
                ))
                .into_any_element()
        } else {
            div()
                .id("raw-editor-scroll")
                .w_full()
                .min_w_0()
                .flex_1()
                .size_full()
                .overflow_x_hidden()
                .overflow_y_scroll()
                .child(
                    div()
                        .w_full()
                        .min_w_0()
                        .child(props.raw_editor),
                )
                .into_any_element()
        })
}

fn render_spans(
    spans: &[Span],
    text_size_px: f32,
    theme: &Theme,
    block_ix: usize,
    caret: Option<Range<usize>>,
    block_source: Range<usize>,
) -> AnyElement {
    let mut full_text = String::new();
    let mut highlights = Vec::new();
    let mut links = Vec::new();
    let mut current_byte = 0;
    let mut caret_char_pos: Option<usize> = None;

    for span in spans {
        let span_len = span.text.len();
        if span_len == 0 {
            continue;
        }
        let range = current_byte..(current_byte + span_len);

        if let Some(ref sel) = caret {
            if sel.is_empty() {
                if sel.start >= span.source.start && sel.start <= span.source.end {
                    let within = sel.start.saturating_sub(span.source.start).min(span_len);
                    let char_count =
                        full_text.chars().count() + span.text[..within].chars().count();
                    caret_char_pos = Some(char_count);
                }
            } else if sel.end > span.source.start && sel.start < span.source.end {
                let within_start = sel.start.saturating_sub(span.source.start).min(span_len);
                let within_end = sel.end.saturating_sub(span.source.start).min(span_len);
                if within_end > within_start {
                    highlights.push((
                        (current_byte + within_start)..(current_byte + within_end),
                        HighlightStyle {
                            background_color: Some(theme.selection_bg.into()),
                            ..Default::default()
                        },
                    ));
                }
            }
        }

        current_byte += span_len;
        full_text.push_str(&span.text);

        if let Some(ref url) = span.link {
            links.push((range.clone(), url.clone()));
        }

        let mut hs = HighlightStyle::default();
        let mut has_hs = false;

        if span.bold {
            hs.font_weight = Some(FontWeight::BOLD);
            has_hs = true;
        }
        if span.italic {
            hs.font_style = Some(FontStyle::Italic);
            has_hs = true;
        }
        if span.underline {
            hs.underline = Some(UnderlineStyle {
                thickness: px(1.0),
                color: Some(theme.text_primary.into()),
                wavy: false,
            });
            has_hs = true;
        }
        if span.strike {
            hs.strikethrough = Some(StrikethroughStyle {
                thickness: px(1.0),
                color: Some(theme.text_faint.into()),
            });
            has_hs = true;
        }
        if span.code {
            hs.background_color = Some(theme.tile_hover.into());
            hs.color = Some(theme.code_text.into());
            has_hs = true;
        }
        if span.link.is_some() {
            hs.color = Some(theme.text_hi.into());
            hs.underline = Some(UnderlineStyle {
                thickness: px(1.0),
                color: Some(theme.text_hi.into()),
                wavy: false,
            });
            has_hs = true;
        }

        if has_hs {
            highlights.push((range, hs));
        }
    }

    if let Some(ref sel) = caret {
        if sel.is_empty()
            && caret_char_pos.is_none()
            && sel.start >= block_source.start
            && sel.start <= block_source.end
        {
            if spans.is_empty() || sel.start <= spans[0].source.start {
                caret_char_pos = Some(0);
            } else if sel.start >= spans.last().unwrap().source.end {
                caret_char_pos = Some(full_text.chars().count());
            } else {
                let mut running_chars = 0;
                for span in spans {
                    if sel.start <= span.source.end {
                        let within = sel.start.saturating_sub(span.source.start).min(span.text.len());
                        caret_char_pos = Some(running_chars + span.text[..within].chars().count());
                        break;
                    }
                    running_chars += span.text.chars().count();
                }
                if caret_char_pos.is_none() {
                    caret_char_pos = Some(running_chars);
                }
            }
        }
    }

    let caret_byte_idx = caret_char_pos.map(|char_idx| {
        full_text
            .char_indices()
            .nth(char_idx)
            .map(|(i, _)| i)
            .unwrap_or(full_text.len())
    });

    let styled = StyledText::new(full_text).with_highlights(highlights);
    let text_layout = styled.layout().clone();

    let text_element: AnyElement = if !links.is_empty() {
        let click_ranges = links.iter().map(|(r, _)| r.clone()).collect::<Vec<_>>();
        let url_targets = links.into_iter().map(|(_, u)| u).collect::<Vec<_>>();
        InteractiveText::new(("interactive-span", block_ix), styled)
            .on_click(click_ranges, move |ix, _, _| {
                if let Some(url) = url_targets.get(ix) {
                    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
                }
            })
            .into_any_element()
    } else {
        styled.into_any_element()
    };

    let caret_h = px((text_size_px * 1.25).max(14.0));
    if let Some(byte_idx) = caret_byte_idx {
        let cursor_color = theme.text_hi;
        div()
            .relative()
            .w_full()
            .min_w_0()
            .min_h(caret_h)
            .child(text_element)
            .child(
                canvas(
                    |_bounds, _window, _cx| (),
                    move |bounds, (), window, _cx| {
                        let pos = text_layout
                            .position_for_index(byte_idx)
                            .unwrap_or(bounds.origin);
                        window.paint_quad(fill(
                            Bounds::new(pos, size(px(2.0), caret_h)),
                            cursor_color,
                        ));
                    },
                )
                .absolute()
                .size_full(),
            )
            .into_any_element()
    } else {
        div()
            .relative()
            .w_full()
            .min_w_0()
            .min_h(caret_h)
            .child(text_element)
            .into_any_element()
    }
}

fn track_block(
    element: Div,
    index: usize,
    hits: Rc<RefCell<Vec<Option<Bounds<Pixels>>>>>,
    content: String,
    font_size: f32,
    text_inset: f32,
    on_place: WysiwygPlace,
) -> Div {
    element
        .relative()
        .w_full()
        .min_w_0()
        .whitespace_normal()
        .on_mouse_down(MouseButton::Left, {
            let hits = hits.clone();
            move |event, window, cx| {
                if window.default_prevented() {
                    return;
                }
                let bounds = hits.borrow().get(index).copied().flatten();
                if let Some(bounds) = bounds {
                    let mut local_x = event.position.x - bounds.left() - px(text_inset);
                    let mut local_y = event.position.y - bounds.top();
                    if local_x < px(0.0) {
                        local_x = px(0.0);
                    }
                    if local_y < px(0.0) {
                        local_y = px(0.0);
                    }
                    let line_h = (font_size * 1.65).max(1.0);
                    let char_w = (font_size * 0.5).max(1.0);
                    let width = (f32::from(bounds.size.width) - text_inset).max(char_w);
                    let columns = (width / char_w).floor().max(1.0) as usize;
                    let line = (f32::from(local_y) / line_h).floor().max(0.0) as usize;
                    let column = (f32::from(local_x) / char_w).floor().max(0.0) as usize;
                    let rendered = line.saturating_mul(columns).saturating_add(column);
                    if let Some(offset) = source_offset_for_click(&content, index, rendered) {
                        on_place(offset, window, cx);
                    }
                }
                cx.stop_propagation();
            }
        })
        .child(
            canvas(
                {
                    let hits = hits.clone();
                    move |bounds, _, _| {
                        let mut hits = hits.borrow_mut();
                        if hits.len() <= index {
                            hits.resize(index + 1, None);
                        }
                        hits[index] = Some(bounds);
                    }
                },
                |_, (), _, _| {},
            )
            .absolute()
            .size_full(),
        )
}

#[allow(clippy::too_many_arguments)]
fn render_wysiwyg_view(
    content: &str,
    base_size: f32,
    zoom_factor: f32,
    theme: &Theme,
    hits: Rc<RefCell<Vec<Option<Bounds<Pixels>>>>>,
    on_place: WysiwygPlace,
    content_alignment: ContentAlignment,
    caret: Option<Range<usize>>,
    on_toggle_task: TaskToggle,
    on_table_edit: TableEdit,
) -> AnyElement {
    let blocks = parse_markdown(content);
    let measure = sz::MEASURE_FALLBACK * zoom_factor;
    let line_box = base_size * 1.65;
    let list_gap = (wysiwyg_list_row_height(base_size) - line_box).max(2.0);
    let is_centered = content_alignment == ContentAlignment::Centered;

    div()
        .id("wysiwyg-editor-scroll")
        .flex_1()
        .overflow_y_scroll()
        .pt(px(48.0))
        .pb(px(64.0))
        .px(px(48.0))
        .font_family(Theme::FONT_PROSE)
        .on_mouse_down(MouseButton::Left, {
            let on_place = on_place.clone();
            let len = content.len();
            move |_, window, cx| {
                if !window.default_prevented() {
                    on_place(len, window, cx);
                }
            }
        })
        .child(
            div()
                .w_full()
                .when(is_centered, |s| s.max_w(measure).mx_auto())
                .when(!is_centered, |s| s.max_w_full().ml_0())
                .flex()
                .flex_col()
                .min_w_0()
                .whitespace_normal()
                .children(blocks.into_iter().enumerate().map(|(ix, block)| {
                    let inset = match &block {
                        Block::ListItem { .. } => 24.0,
                        Block::Blockquote { .. } => 14.0,
                        _ => 0.0,
                    };
                    let (element, block_font_size) = match block {
                        Block::Heading { level, spans, source } => {
                            let (h_size, h_weight, h_top, h_bot) = match level {
                                1 => (34.0 * zoom_factor, FontWeight::EXTRA_BOLD, px(24.0), px(24.0)),
                                2 => (24.0 * zoom_factor, FontWeight::BOLD, px(40.0), px(16.0)),
                                _ => (19.0 * zoom_factor, FontWeight::BOLD, px(24.0), px(12.0)),
                            };
                            let mut heading_el = div()
                                .flex_shrink_0()
                                .w_full()
                                .min_w_0()
                                .whitespace_normal()
                                .mt(h_top)
                                .mb(h_bot)
                                .text_size(px(h_size))
                                .font_weight(h_weight)
                                .text_color(if level == 1 { theme.h1 } else { theme.h2 })
                                .child(render_spans(&spans, h_size, theme, ix, caret.clone(), source));

                            if level == 1 {
                                heading_el = heading_el
                                    .pb(px(8.0))
                                    .border_b_1()
                                    .border_color(theme.border_primary);
                            }
                            (heading_el, h_size)
                        }
                        Block::Paragraph { spans, source } => (
                            div()
                                .flex_shrink_0()
                                .w_full()
                                .min_w_0()
                                .whitespace_normal()
                                .my(px(12.0))
                                .text_size(px(base_size))
                                .line_height(px(base_size * 1.65))
                                .text_color(theme.text_primary)
                                .child(render_spans(&spans, base_size, theme, ix, caret.clone(), source)),
                            base_size,
                        ),
                        Block::Blockquote { spans, source } => (
                            div()
                                .flex_shrink_0()
                                .pl(px(14.0))
                                .my(px(12.0))
                                .border_l_3()
                                .border_color(theme.border_surface)
                                .italic()
                                .whitespace_normal()
                                .w_full()
                                .min_w_0()
                                .child(render_spans(&spans, base_size, theme, ix, caret.clone(), source)),
                            base_size,
                        ),
                        Block::CodeBlock {
                            language: _, code, ..
                        } => (
                            div()
                                .flex_shrink_0()
                                .my(px(16.0))
                                .p(px(14.0))
                                .rounded(sz::R_MD)
                                .bg(theme.tile_hover)
                                .border_1()
                                .border_color(theme.border_subtle)
                                .font_family(Theme::FONT_MONO)
                                .text_size(px(base_size * 0.9))
                                .text_color(theme.code_text)
                                .whitespace_normal()
                                .w_full()
                                .min_w_0()
                                .child(code),
                            base_size * 0.9,
                        ),
                        Block::ListItem {
                            is_task,
                            checked,
                            ordered,
                            index,
                            spans,
                            source,
                        } => {
                            let task_checkbox = if is_task {
                                let on_toggle = on_toggle_task.clone();
                                let src = source.clone();
                                div()
                                    .id(SharedString::from(format!("task-cb-{ix}")))
                                    .w(px(15.0))
                                    .h(px(15.0))
                                    .rounded(sz::R_SM)
                                    .cursor_pointer()
                                    .border_1()
                                    .border_color(if checked {
                                        theme.text_hi
                                    } else {
                                        theme.border_primary
                                    })
                                    .bg(if checked {
                                        theme.text_hi
                                    } else {
                                        rgba(0x00000000)
                                    })
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_size(px(10.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgb(0x121212))
                                    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                                        cx.stop_propagation();
                                        on_toggle(src.clone(), window, cx);
                                    })
                                    .child(if checked { "✓" } else { "" })
                                    .into_any_element()
                            } else if ordered {
                                div()
                                    .text_size(px(base_size))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(theme.text_faint)
                                    .child(format!("{}.", index))
                                    .into_any_element()
                            } else {
                                div()
                                    .text_size(px(base_size))
                                    .text_color(theme.text_faint)
                                    .child("•")
                                    .into_any_element()
                            };

                            (
                                div()
                                    .flex()
                                    .flex_shrink_0()
                                    .items_start()
                                    .w_full()
                                    .min_w_0()
                                    .mt(px(list_gap / 2.0))
                                    .mb(px(list_gap / 2.0))
                                    .child(
                                        div()
                                            .w(px(24.0))
                                            .h(px(line_box))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .flex_shrink_0()
                                            .child(task_checkbox),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .w_full()
                                            .whitespace_normal()
                                            .text_size(px(base_size))
                                            .line_height(px(line_box))
                                            .text_color(theme.text_primary)
                                            .child(render_spans(
                                                &spans,
                                                base_size,
                                                theme,
                                                ix,
                                                caret.clone(),
                                                source,
                                            )),
                                    ),
                                base_size,
                            )
                        }
                        Block::Table {
                            headers,
                            rows,
                            source,
                        } => {
                            let on_edit = on_table_edit.clone();
                            let src = source.clone();
                            let num_cols = headers.len();
                            let num_rows = rows.len();

                            let btn = |label: &'static str, id: String, t: &Theme| {
                                div()
                                    .id(SharedString::from(id))
                                    .px(px(6.0))
                                    .py(px(2.0))
                                    .rounded(sz::R_SM)
                                    .text_size(px(11.0))
                                    .cursor_pointer()
                                    .bg(t.tile_hover)
                                    .text_color(t.text_muted)
                                    .hover(|s| s.bg(t.border_subtle).text_color(t.text_hi))
                                    .child(label)
                            };

                            (
                                div()
                                    .flex_shrink_0()
                                    .w_full()
                                    .min_w_0()
                                    .my(px(16.0))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(6.0))
                                            .mb(px(6.0))
                                            .child({
                                                let on_edit = on_edit.clone();
                                                let src = src.clone();
                                                let headers = headers.clone();
                                                let rows = rows.clone();
                                                btn("+ Row", format!("tbl-add-row-{ix}"), theme)
                                                    .on_mouse_down(
                                                        MouseButton::Left,
                                                        move |_, window, cx| {
                                                            cx.stop_propagation();
                                                            let mut new_rows = rows.clone();
                                                            new_rows.push(vec![
                                                                String::new();
                                                                headers.len()
                                                            ]);
                                                            on_edit(
                                                                src.clone(),
                                                                headers.clone(),
                                                                new_rows,
                                                                window,
                                                                cx,
                                                            );
                                                        },
                                                    )
                                            })
                                            .when(num_rows > 1, |s| {
                                                let on_edit = on_edit.clone();
                                                let src = src.clone();
                                                let headers = headers.clone();
                                                let rows = rows.clone();
                                                s.child(
                                                    btn("- Row", format!("tbl-del-row-{ix}"), theme)
                                                        .on_mouse_down(
                                                            MouseButton::Left,
                                                            move |_, window, cx| {
                                                                cx.stop_propagation();
                                                                let mut new_rows = rows.clone();
                                                                new_rows.pop();
                                                                on_edit(
                                                                    src.clone(),
                                                                    headers.clone(),
                                                                    new_rows,
                                                                    window,
                                                                    cx,
                                                                );
                                                            },
                                                        ),
                                                )
                                            })
                                            .child({
                                                let on_edit = on_edit.clone();
                                                let src = src.clone();
                                                let headers = headers.clone();
                                                let rows = rows.clone();
                                                btn("+ Col", format!("tbl-add-col-{ix}"), theme)
                                                    .on_mouse_down(
                                                        MouseButton::Left,
                                                        move |_, window, cx| {
                                                            cx.stop_propagation();
                                                            let mut new_headers = headers.clone();
                                                            new_headers.push("Column".into());
                                                            let mut new_rows = rows.clone();
                                                            for r in &mut new_rows {
                                                                r.push(String::new());
                                                            }
                                                            on_edit(
                                                                src.clone(),
                                                                new_headers,
                                                                new_rows,
                                                                window,
                                                                cx,
                                                            );
                                                        },
                                                    )
                                            })
                                            .when(num_cols > 1, |s| {
                                                let on_edit = on_edit.clone();
                                                let src = src.clone();
                                                let headers = headers.clone();
                                                let rows = rows.clone();
                                                s.child(
                                                    btn("- Col", format!("tbl-del-col-{ix}"), theme)
                                                        .on_mouse_down(
                                                            MouseButton::Left,
                                                            move |_, window, cx| {
                                                                cx.stop_propagation();
                                                                let mut new_headers = headers.clone();
                                                                new_headers.pop();
                                                                let mut new_rows = rows.clone();
                                                                for r in &mut new_rows {
                                                                    r.pop();
                                                                }
                                                                on_edit(
                                                                    src.clone(),
                                                                    new_headers,
                                                                    new_rows,
                                                                    window,
                                                                    cx,
                                                                );
                                                            },
                                                        ),
                                                )
                                            }),
                                    )
                                    .child(
                                        div()
                                            .id(SharedString::from(format!("table-scroll-{ix}")))
                                            .overflow_x_scroll()
                                            .border_1()
                                            .border_color(theme.border_subtle)
                                            .rounded(sz::R_SM)
                                            .child(
                                                div()
                                                    .flex()
                                                    .flex_col()
                                                    .child(
                                                        div()
                                                            .flex()
                                                            .bg(theme.tile_hover)
                                                            .border_b_1()
                                                            .border_color(theme.border_subtle)
                                                            .children(
                                                                headers.into_iter().map(|h| {
                                                                    div()
                                                                        .px(px(12.0))
                                                                        .py(px(8.0))
                                                                        .font_weight(
                                                                            FontWeight::BOLD,
                                                                        )
                                                                        .text_size(px(base_size))
                                                                        .border_r_1()
                                                                        .border_color(
                                                                            theme.border_subtle,
                                                                        )
                                                                        .child(if h.is_empty() {
                                                                            "—".to_string()
                                                                        } else {
                                                                            h
                                                                        })
                                                                }),
                                                            ),
                                                    )
                                                    .children(rows.into_iter().map(|row| {
                                                        div()
                                                            .flex()
                                                            .border_b_1()
                                                            .border_color(theme.border_subtle)
                                                            .children(row.into_iter().map(|cell| {
                                                                div()
                                                                    .px(px(12.0))
                                                                    .py(px(6.0))
                                                                    .text_size(px(base_size))
                                                                    .border_r_1()
                                                                    .border_color(
                                                                        theme.border_subtle,
                                                                    )
                                                                    .child(if cell.is_empty() {
                                                                        " ".to_string()
                                                                    } else {
                                                                        cell
                                                                    })
                                                            }))
                                                    })),
                                            ),
                                    ),
                                base_size,
                            )
                        }
                        Block::HorizontalRule { .. } => (
                            div()
                                .flex_shrink_0()
                                .h(px(1.0))
                                .my(px(24.0))
                                .bg(theme.border_primary),
                            base_size,
                        ),
                        Block::EmptyLine { source } => {
                            let has_cursor = caret.as_ref().is_some_and(|c| {
                                c.is_empty() && c.start >= source.start && c.start <= source.end
                            });
                            (
                                div()
                                    .flex_shrink_0()
                                    .h(px(base_size * 0.9))
                                    .flex()
                                    .items_center()
                                    .when(has_cursor, |s| {
                                        s.child(
                                            div()
                                                .w(px(2.0))
                                                .h(px(base_size * 0.9))
                                                .bg(theme.text_hi),
                                        )
                                    }),
                                base_size,
                            )
                        }
                    };
                    track_block(
                        element,
                        ix,
                        hits.clone(),
                        content.to_string(),
                        block_font_size,
                        inset,
                        on_place.clone(),
                    )
                })),
        )
        .into_any_element()
}
