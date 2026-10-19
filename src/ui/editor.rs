use gpui::prelude::*;
use gpui::*;
use crate::markdown::{parse_markdown, Block, Span};
use crate::model::ViewMode;
use crate::theme::Theme;

#[allow(dead_code)]
pub struct EditorProps<FContentChange>
where
    FContentChange: Fn(String, &mut App) + 'static + Clone,
{
    pub content: String,
    pub view_mode: ViewMode,
    pub zoom_level: usize,
    pub show_line_numbers: bool,
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
    let base_size = 15.0 * zoom_factor;
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
                    .rounded(px(6.0))
                    .bg(if theme.mode.is_dark() { rgba(0x2d2d2de6) } else { rgba(0xfafafae6) })
                    .border_1()
                    .border_color(theme.border_primary)
                    .shadow_lg()
                    .text_size(px(11.0))
                    .font_family(Theme::FONT_MONO)
                    .font_weight(FontWeight::BOLD)
                    .child(format!("Zoom {}%", props.zoom_level))
            )
        })
        .child(if is_wysiwyg {
            render_wysiwyg_view(&props.content, base_size, theme)
        } else {
            render_raw_view(&props.content, base_size, props.show_line_numbers, theme)
        })
}

fn render_spans(spans: &[Span], base_size: f32, theme: &Theme) -> impl IntoElement {
    div()
        .flex()
        .flex_wrap()
        .items_center()
        .children(spans.iter().map(|span| {
            let mut el = div().text_size(px(base_size));

            if span.bold {
                el = el.font_weight(FontWeight::BOLD);
            }
            if span.italic {
                el = el.italic();
            }
            if span.underline {
                el = el.underline();
            }

            if span.code {
                div()
                    .px(px(4.0))
                    .py(px(1.0))
                    .mx(px(2.0))
                    .rounded(px(4.0))
                    .bg(if theme.mode.is_dark() { rgba(0x8f8b8226) } else { rgba(0x0000000f) })
                    .text_color(theme.text_primary)
                    .font_family(Theme::FONT_MONO)
                    .text_size(px(base_size * 0.9))
                    .child(span.text.clone())
            } else if let Some(ref _url) = span.link {
                div()
                    .text_color(theme.accent_ubuntu)
                    .underline()
                    .cursor_pointer()
                    .child(span.text.clone())
            } else {
                el.child(span.text.clone())
            }
        }))
}

fn render_wysiwyg_view(content: &str, base_size: f32, theme: &Theme) -> AnyElement {
    let blocks = parse_markdown(content);

    div()
        .id("wysiwyg-editor-scroll")
        .flex_1()
        .overflow_y_scroll()
        .p(px(24.0))
        .font_family(Theme::FONT_PROSE)
        .line_height(relative(1.7))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(10.0))
                .max_w(px(880.0))
                .mx_auto()
                .children(blocks.into_iter().map(|block| match block {
                    Block::Heading { level, spans } => {
                        let scale = match level {
                            1 => 2.0,
                            2 => 1.5,
                            _ => 1.25,
                        };
                        let heading_size = base_size * scale;
                        let mut heading_el = div()
                            .font_weight(if level == 1 { FontWeight::BOLD } else { FontWeight::SEMIBOLD })
                            .mt(px(if level == 1 { 20.0 } else { 14.0 }))
                            .mb(px(if level == 1 { 10.0 } else { 6.0 }))
                            .child(render_spans(&spans, heading_size, theme));

                        if level == 1 {
                            heading_el = heading_el
                                .pb(px(8.0))
                                .border_b_1()
                                .border_color(rgba(0x8f8b8233));
                        }
                        heading_el
                    }
                    Block::Paragraph { spans } => div()
                        .my(px(4.0))
                        .child(render_spans(&spans, base_size, theme)),
                    Block::Blockquote { spans } => div()
                        .pl(px(14.0))
                        .my(px(10.0))
                        .border_l_3()
                        .border_color(rgba(0x8f8b8280))
                        .italic()
                        .child(render_spans(&spans, base_size, theme)),
                    Block::CodeBlock { language: _, code } => div()
                        .my(px(12.0))
                        .p(px(14.0))
                        .rounded(px(6.0))
                        .bg(if theme.mode.is_dark() { rgba(0x00000040) } else { rgba(0x00000008) })
                        .border_1()
                        .border_color(theme.border_subtle)
                        .font_family(Theme::FONT_MONO)
                        .text_size(px(base_size * 0.9))
                        .child(code),
                    Block::ListItem { is_task, checked, ordered, index, spans } => div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .my(px(3.0))
                        .child(if is_task {
                            div()
                                .w(px(14.0))
                                .h(px(14.0))
                                .rounded(px(3.0))
                                .border_1()
                                .border_color(if checked { theme.accent_emerald } else { theme.border_primary })
                                .bg(if checked { theme.accent_emerald } else { rgba(0x00000000) })
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_size(px(10.0))
                                .text_color(rgb(0xffffff))
                                .child(if checked { "✓" } else { "" })
                        } else if ordered {
                            div()
                                .text_size(px(base_size))
                                .font_weight(FontWeight::BOLD)
                                .text_color(theme.text_muted)
                                .child(format!("{}.", index))
                        } else {
                            div()
                                .text_size(px(base_size))
                                .text_color(theme.text_muted)
                                .child("•")
                        })
                        .child(render_spans(&spans, base_size, theme)),
                    Block::Table { headers, rows } => div()
                        .my(px(14.0))
                        .child(
                            div()
                                .id("table-scroll")
                                .overflow_x_scroll()
                                .border_1()
                                .border_color(theme.border_subtle)
                                .rounded(px(4.0))
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        // Header row
                                        .child(
                                            div()
                                                .flex()
                                                .bg(if theme.mode.is_dark() { rgba(0x8f8b821a) } else { rgba(0x00000008) })
                                                .border_b_1()
                                                .border_color(theme.border_subtle)
                                                .children(headers.into_iter().map(|h| {
                                                    div()
                                                        .px(px(12.0))
                                                        .py(px(8.0))
                                                        .font_weight(FontWeight::BOLD)
                                                        .text_size(px(base_size))
                                                        .border_r_1()
                                                        .border_color(theme.border_subtle)
                                                        .child(h)
                                                }))
                                        )
                                        // Data rows
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
                        .my(px(18.0))
                        .bg(theme.border_subtle),
                    Block::EmptyLine => div().h(px(base_size)),
                }))
        )
        .into_any_element()
}

fn render_raw_view(content: &str, base_size: f32, show_line_numbers: bool, theme: &Theme) -> AnyElement {
    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len().max(1);

    div()
        .id("raw-editor-scroll")
        .flex_1()
        .flex()
        .overflow_y_scroll()
        .font_family(Theme::FONT_MONO)
        .text_size(px(base_size))
        .line_height(relative(1.7))
        // Line numbers gutter
        .when(show_line_numbers, |s| {
            s.child(
                div()
                    .w(px(48.0))
                    .py(px(16.0))
                    .pr(px(12.0))
                    .text_align(TextAlign::Right)
                    .bg(if theme.mode.is_dark() { rgb(0x181818) } else { rgb(0xf5f5f5) })
                    .border_r_1()
                    .border_color(theme.border_subtle)
                    .text_color(theme.text_muted)
                    .children((1..=total_lines).map(|n| div().child(format!("{}", n))))
            )
        })
        // Raw content area
        .child(
            div()
                .flex_1()
                .p(px(16.0))
                .child(content.to_string())
        )
        .into_any_element()
}
