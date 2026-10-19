use crate::markdown::{parse_markdown, source_offset_for_click, Block, Span};
use crate::model::{text_ops::wysiwyg_list_row_height, ViewMode};
use crate::theme::{sz, Theme};

pub type WysiwygPlace = Arc<dyn for<'a, 'b> Fn(usize, &'a mut Window, &'b mut App) + 'static>;
use gpui::prelude::*;
use gpui::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

pub struct EditorProps {
    pub content: String,
    pub view_mode: ViewMode,
    pub zoom_level: usize,
    pub base_font_size: f32,
    pub show_zoom_hud: bool,
    /// Interactive raw editor. WYSIWYG keeps the formatted preview and edits through this surface.
    pub raw_editor: AnyElement,
    pub wysiwyg_bounds: Rc<RefCell<Vec<Option<Bounds<Pixels>>>>>,
    pub on_wysiwyg_place: WysiwygPlace,
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
                .flex()
                .flex_col()
                .flex_1()
                .size_full()
                .min_h_0()
                .child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .overflow_hidden()
                        .child(render_wysiwyg_view(
                            &props.content,
                            base_size,
                            zoom_factor,
                            theme,
                            props.wysiwyg_bounds.clone(),
                            props.on_wysiwyg_place.clone(),
                        )),
                )
                .child(div().h(px(1.0)).bg(theme.border_subtle).flex_shrink_0())
                .child(
                    div()
                        .id("wysiwyg-raw-fallback")
                        .flex_1()
                        .min_h(px(160.0))
                        .flex()
                        .overflow_hidden()
                        .child(props.raw_editor),
                )
                .into_any_element()
        } else {
            div()
                .flex_1()
                .size_full()
                .flex()
                .overflow_hidden()
                .child(props.raw_editor)
                .into_any_element()
        })
}

fn render_spans(spans: &[Span], text_size_px: f32, theme: &Theme, _block_ix: usize) -> AnyElement {
    div()
        .flex()
        .flex_wrap()
        .w_full()
        .min_w_0()
        .whitespace_normal()
        .children(spans.iter().enumerate().filter_map(|(span_ix, span)| {
            if span.text.is_empty() {
                return None;
            }
            let mut piece = div()
                .id(SharedString::from(format!(
                    "wysiwyg-span-{_block_ix}-{span_ix}"
                )))
                .min_w_0()
                .max_w_full()
                .whitespace_normal()
                .when(span.bold, |element| element.font_weight(FontWeight::MEDIUM))
                .when(span.italic, |element| element.italic())
                .when(span.underline || span.link.is_some(), |element| {
                    element.underline()
                })
                .when(span.strike, |element| element.line_through().relative())
                .when(span.code, |element| {
                    element
                        .px(px(3.0))
                        .rounded(sz::R_SM)
                        .bg(theme.tile_hover)
                        .text_color(theme.code_text)
                        .font_family(Theme::FONT_MONO)
                })
                .when(span.link.is_some(), |element| {
                    element.text_color(theme.text_hi).cursor_pointer()
                })
                .child(span.text.clone());
            if span.strike {
                piece = piece.child(
                    div()
                        .absolute()
                        .top(px(text_size_px * 0.55))
                        .left_0()
                        .right_0()
                        .h(px(1.0))
                        .bg(theme.text_primary),
                );
            }
            if let Some(url) = span.link.clone() {
                piece = piece.on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    cx.stop_propagation();
                    let _ = std::process::Command::new("xdg-open").arg(&url).spawn();
                });
            }
            Some(piece)
        }))
        .into_any_element()
}

fn track_block(
    element: Div,
    index: usize,
    hits: Rc<RefCell<Vec<Option<Bounds<Pixels>>>>>,
    content: String,
    base_size: f32,
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
                    let line_h = (base_size * 1.65).max(1.0);
                    let char_w = (base_size * 0.5).max(1.0);
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

fn render_wysiwyg_view(
    content: &str,
    base_size: f32,
    zoom_factor: f32,
    theme: &Theme,
    hits: Rc<RefCell<Vec<Option<Bounds<Pixels>>>>>,
    on_place: WysiwygPlace,
) -> AnyElement {
    let blocks = parse_markdown(content);
    let measure = sz::MEASURE_FALLBACK * zoom_factor;
    let line_box = base_size * 1.65;
    let list_gap = (wysiwyg_list_row_height(base_size) - line_box).max(2.0);

    div()
        .id("wysiwyg-editor-scroll")
        .flex_1()
        .overflow_y_scroll()
        .pt(px(48.0))
        .pb(px(64.0))
        .px(px(48.0))
        .font_family(Theme::FONT_PROSE)
        .child(
            div()
                .w_full()
                .max_w(measure)
                .mx_auto()
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
                    let element = match block {
                        Block::Heading { level, spans, .. } => {
                            let (h_size, h_top, h_bot) = match level {
                                1 => (34.0 * zoom_factor, px(24.0), px(24.0)),
                                2 => (23.0 * zoom_factor, px(40.0), px(16.0)),
                                _ => (19.0 * zoom_factor, px(24.0), px(12.0)),
                            };
                            let mut heading_el = div()
                                .flex_shrink_0()
                                .w_full()
                                .min_w_0()
                                .whitespace_normal()
                                .mt(h_top)
                                .mb(h_bot)
                                .text_size(px(h_size))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(if level == 1 { theme.h1 } else { theme.h2 })
                                .child(render_spans(&spans, h_size, theme, ix));

                            if level == 1 {
                                heading_el = heading_el
                                    .pb(px(8.0))
                                    .border_b_1()
                                    .border_color(theme.border_primary);
                            }
                            heading_el
                        }
                        Block::Paragraph { spans, .. } => div()
                            .flex_shrink_0()
                            .w_full()
                            .min_w_0()
                            .whitespace_normal()
                            .my(px(12.0))
                            .text_size(px(base_size))
                            .line_height(px(base_size * 1.65))
                            .text_color(theme.text_primary)
                            .child(render_spans(&spans, base_size, theme, ix)),
                        Block::Blockquote { spans, .. } => div()
                            .flex_shrink_0()
                            .pl(px(14.0))
                            .my(px(12.0))
                            .border_l_3()
                            .border_color(theme.border_surface)
                            .italic()
                            .whitespace_normal()
                            .w_full()
                            .min_w_0()
                            .child(render_spans(&spans, base_size, theme, ix)),
                        Block::CodeBlock {
                            language: _, code, ..
                        } => div()
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
                        Block::ListItem {
                            is_task,
                            checked,
                            ordered,
                            index,
                            spans,
                            ..
                        } => div()
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
                                    .child(if is_task {
                                        div()
                                            .w(px(14.0))
                                            .h(px(14.0))
                                            .rounded(sz::R_SM)
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
                                            .text_color(rgb(0x121212))
                                            .child(if checked { "✓" } else { "" })
                                    } else if ordered {
                                        div()
                                            .text_size(px(base_size))
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(theme.text_faint)
                                            .child(format!("{}.", index))
                                    } else {
                                        div()
                                            .text_size(px(base_size))
                                            .text_color(theme.text_faint)
                                            .child("•")
                                    }),
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
                                    .child(render_spans(&spans, base_size, theme, ix)),
                            ),
                        Block::Table { headers, rows, .. } => {
                            div().flex_shrink_0().w_full().min_w_0().my(px(16.0)).child(
                                div()
                                    .id("table-scroll")
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
                                                    .children(headers.into_iter().map(|h| {
                                                        div()
                                                            .px(px(12.0))
                                                            .py(px(8.0))
                                                            .font_weight(FontWeight::MEDIUM)
                                                            .text_size(px(base_size))
                                                            .border_r_1()
                                                            .border_color(theme.border_subtle)
                                                            .child(h)
                                                    })),
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
                                                            .border_color(theme.border_subtle)
                                                            .child(cell)
                                                    }))
                                            })),
                                    ),
                            )
                        }
                        Block::HorizontalRule { .. } => div()
                            .flex_shrink_0()
                            .h(px(1.0))
                            .my(px(24.0))
                            .bg(theme.border_primary),
                        Block::EmptyLine { .. } => div().flex_shrink_0().h(px(base_size * 0.8)),
                    };
                    track_block(
                        element,
                        ix,
                        hits.clone(),
                        content.to_string(),
                        base_size,
                        inset,
                        on_place.clone(),
                    )
                })),
        )
        .into_any_element()
}
