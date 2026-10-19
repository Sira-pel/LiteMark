use gpui::prelude::*;
use gpui::*;
use crate::markdown::{parse_markdown, Block, Span};
use crate::model::ViewMode;
use crate::theme::{sz, Theme};

#[allow(dead_code)]
pub struct EditorProps<FContentChange>
where
    FContentChange: Fn(String, &mut App) + 'static + Clone,
{
    pub content: String,
    pub view_mode: ViewMode,
    pub zoom_level: usize,
    pub base_font_size: f32,
    pub show_line_numbers: bool,
    pub word_wrap: bool,
    pub show_zoom_hud: bool,
    pub on_content_change: FContentChange,
}

pub fn render_editor<FContentChange>(
    props: EditorProps<FContentChange>,
    theme: &Theme,
) -> impl IntoElement
where
    FContentChange: Fn(String, &mut App) + 'static + Clone,
{
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
                    .child(format!("Zoom {}%", props.zoom_level))
            )
        })
        .child(if is_wysiwyg {
            render_wysiwyg_view(&props.content, base_size, zoom_factor, theme)
        } else {
            render_raw_view(&props.content, base_size, props.show_line_numbers, props.word_wrap, theme)
        })
}

fn render_spans(spans: &[Span], _text_size_px: f32, theme: &Theme, block_ix: usize) -> AnyElement {
    let mut full_text = String::new();
    let mut highlights = Vec::new();
    let mut links = Vec::new();
    let mut current_byte = 0;

    for span in spans {
        let span_len = span.text.len();
        if span_len == 0 {
            continue;
        }
        let range = current_byte..(current_byte + span_len);
        current_byte += span_len;
        full_text.push_str(&span.text);

        if let Some(ref url) = span.link {
            links.push((range.clone(), url.clone()));
        }

        let mut hs = HighlightStyle::default();
        let mut has_hs = false;

        if span.bold {
            hs.font_weight = Some(FontWeight::MEDIUM);
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

    let styled = StyledText::new(full_text).with_highlights(highlights);

    if !links.is_empty() {
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
    }
}

fn render_wysiwyg_view(content: &str, base_size: f32, zoom_factor: f32, theme: &Theme) -> AnyElement {
    let blocks = parse_markdown(content);
    let measure = sz::MEASURE_FALLBACK * zoom_factor;

    div()
        .id("wysiwyg-editor-scroll")
        .flex_1()
        .overflow_y_scroll()
        .pt(px(48.0))
        .pb(px(64.0))
        .px(px(48.0))
        .font_family(Theme::FONT_PROSE)
        .line_height(relative(1.65))
        .child(
            div()
                .w_full()
                .max_w(measure)
                .mx_auto()
                .flex()
                .flex_col()
                .children(blocks.into_iter().enumerate().map(|(ix, block)| match block {
                    Block::Heading { level, spans } => {
                        let (h_size, h_top, h_bot) = match level {
                            1 => (34.0 * zoom_factor, px(24.0), px(24.0)),
                            2 => (23.0 * zoom_factor, px(40.0), px(16.0)),
                            _ => (19.0 * zoom_factor, px(24.0), px(12.0)),
                        };
                        let mut heading_el = div()
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
                    Block::Paragraph { spans } => div()
                        .my(px(12.0))
                        .text_size(px(base_size))
                        .line_height(relative(1.65))
                        .text_color(theme.text_primary)
                        .child(render_spans(&spans, base_size, theme, ix)),
                    Block::Blockquote { spans } => div()
                        .pl(px(14.0))
                        .my(px(12.0))
                        .border_l_3()
                        .border_color(theme.border_surface)
                        .italic()
                        .child(render_spans(&spans, base_size, theme, ix)),
                    Block::CodeBlock { language: _, code } => div()
                        .my(px(16.0))
                        .p(px(14.0))
                        .rounded(sz::R_MD)
                        .bg(theme.tile_hover)
                        .border_1()
                        .border_color(theme.border_subtle)
                        .font_family(Theme::FONT_MONO)
                        .text_size(px(base_size * 0.9))
                        .text_color(theme.code_text)
                        .child(code),
                    Block::ListItem { is_task, checked, ordered, index, spans } => div()
                        .flex()
                        .items_start()
                        .my(px(4.0))
                        .line_height(relative(1.65))
                        .child(
                            div()
                                .w(px(24.0))
                                .h(px(base_size * 1.65))
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
                                        .border_color(if checked { theme.text_hi } else { theme.border_primary })
                                        .bg(if checked { theme.text_hi } else { rgba(0x00000000) })
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
                                })
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_size(px(base_size))
                                .text_color(theme.text_primary)
                                .child(render_spans(&spans, base_size, theme, ix))
                        ),
                    Block::Table { headers, rows } => div()
                        .my(px(16.0))
                        .child(
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
                                                }))
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
                                        }))
                                )
                        ),
                    Block::HorizontalRule => div()
                        .h(px(1.0))
                        .my(px(24.0))
                        .bg(theme.border_primary),
                    Block::EmptyLine => div().h(px(base_size * 0.8)),
                }))
        )
        .into_any_element()
}

fn render_raw_view(content: &str, base_size: f32, show_line_numbers: bool, word_wrap: bool, theme: &Theme) -> AnyElement {
    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len().max(1);

    div()
        .id("raw-editor-scroll")
        .flex_1()
        .flex()
        .overflow_y_scroll()
        .when(!word_wrap, |s| s.overflow_x_scroll())
        .font_family(Theme::FONT_MONO)
        .text_size(px(base_size))
        .line_height(relative(1.65))
        .when(show_line_numbers, |s| {
            s.child(
                div()
                    .w(px(48.0))
                    .py(px(16.0))
                    .pr(px(12.0))
                    .text_align(TextAlign::Right)
                    .bg(theme.bg_sidebar)
                    .border_r_1()
                    .border_color(theme.border_subtle)
                    .text_color(theme.text_faint)
                    .children((1..=total_lines).map(|n| div().child(format!("{}", n))))
            )
        })
        .child(
            div()
                .flex_1()
                .p(px(24.0))
                .text_color(theme.text_primary)
                .when(!word_wrap, |s| s.whitespace_nowrap())
                .child(content.to_string())
        )
        .into_any_element()
}
