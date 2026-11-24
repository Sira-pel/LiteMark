//! The note outline on the editor's right edge: a strip of short lines, one
//! per heading, that opens into a card listing them.

use crate::model::outline::{strip_sample, OutlineItem};
use crate::theme::Theme;
use gpui::prelude::*;
use gpui::*;

/// Widest the open card gets (wide editors).
pub const OUTLINE_CARD_MAX_W: f32 = 260.0;
/// Below this editor width the collapsed strip is hidden (it would sit on
/// the text); the shortcut still opens the card.
pub const OUTLINE_STRIP_MIN_EDITOR_W: f32 = 400.0;
/// Gap between the card and the editor's edge.
pub const OUTLINE_EDGE: f32 = 8.0;
/// Width of the collapsed strip.
pub const OUTLINE_STRIP_W: f32 = 40.0;
/// Narrowest editor that keeps a pinned card open. Below it the card would
/// leave the text too little room, so it shows as the strip.
pub const OUTLINE_PINNED_MIN_EDITOR_W: f32 = 640.0;
const STRIP_PITCH: f32 = 10.0;
const STRIP_MAX_LINES: usize = 40;
const OUTLINE_ROW_H: f32 = 30.0;
const OUTLINE_HEADER_H: f32 = 36.0;

type HoverFn = Box<dyn Fn(bool, &mut Window, &mut App)>;
type OpenFn = Box<dyn Fn(&mut Window, &mut App)>;
type JumpFn = std::rc::Rc<dyn Fn(usize, &mut Window, &mut App)>;

pub struct OutlineProps {
    pub items: std::sync::Arc<Vec<OutlineItem>>,
    pub current: Option<usize>,
    /// Open card (hovered or pinned) instead of the strip.
    pub expanded: bool,
    /// Height of the editor area, to keep both parts inside it.
    pub area_height: Pixels,
    /// Width of the editor area, which sets how wide the card may be.
    pub area_width: Pixels,
    /// The pointer entered or left the strip or card.
    pub on_hover: HoverFn,
    /// A click on the strip (it opens the card and keeps it open).
    pub on_open: OpenFn,
    /// A click on the card's title bar: keeps it open, or closes it if it was.
    pub on_close: OpenFn,
    /// A heading was clicked: byte offset of its line.
    pub on_jump: JumpFn,
}

/// Card width: as wide as the longest heading needs, up to a cap that
/// shrinks with the editor (260 wide, 240 medium, 70% when narrow).
pub fn card_width(area_w: f32, items: &[OutlineItem]) -> f32 {
    let cap = if area_w >= 900.0 {
        OUTLINE_CARD_MAX_W
    } else if area_w >= 500.0 {
        240.0
    } else {
        (area_w * 0.7).min(240.0)
    };
    let longest = items
        .iter()
        .map(|i| i.text.chars().count().max(8))
        .max()
        .unwrap_or(8) as f32;
    // Text at ~7.8px a character, plus badge, indent and padding.
    (longest * 7.8 + 84.0).clamp(180.0_f32.min(cap), cap)
}

fn strip_width(level: u8) -> f32 {
    match level {
        0 | 1 => 24.0,
        2 => 18.0,
        _ => 12.0,
    }
}

pub fn render_outline(props: OutlineProps, theme: &Theme) -> impl IntoElement {
    let area = f32::from(props.area_height);
    let area_w = f32::from(props.area_width);
    let on_hover = props.on_hover;
    let on_close = props.on_close;
    let root = div()
        .id("outline")
        // The editor's I-beam stops at the card.
        .occlude()
        .absolute()
        .top(px(OUTLINE_EDGE))
        .on_hover(move |hovered, window, cx| on_hover(*hovered, window, cx));
    if props.expanded {
        let card_w = card_width(area_w, &props.items);
        let max_h = (area - 2.0 * OUTLINE_EDGE).max(80.0);
        let list_h = (props.items.len() as f32 * OUTLINE_ROW_H)
            .min((max_h - OUTLINE_HEADER_H).max(OUTLINE_ROW_H));
        let items = props.items.clone();
        let current_ix = props.current;
        let on_jump = props.on_jump.clone();
        let outline_current = theme.outline_current;
        let row_hover = theme.row_hover;
        let text_faint = theme.text_faint;
        let text_hi = theme.text_hi;
        let row_text = theme.row_text;
        root.right(px(OUTLINE_EDGE)).child(
            div()
                .w(px(card_w))
                .max_h(px(max_h))
                .flex()
                .flex_col()
                .rounded(px(10.0))
                .bg(theme.menu_bg)
                .border_1()
                .border_color(theme.border_surface)
                .shadow_lg()
                .overflow_hidden()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(
                    div()
                        .id("outline-header")
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_between()
                        .cursor_pointer()
                        .on_click(move |_, window, cx| on_close(window, cx))
                        .px(px(12.0))
                        .pt(px(8.0))
                        .pb(px(4.0))
                        .child(
                            div()
                                .text_size(px(12.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(theme.text_muted)
                                .child("Outline"),
                        )
                        .child(
                            div()
                                .text_size(px(10.5))
                                .font_family(Theme::FONT_MONO)
                                .text_color(theme.text_faint)
                                .child(if cfg!(target_os = "macos") {
                                    "Cmd+Shift+O"
                                } else {
                                    "Ctrl+Shift+O"
                                }),
                        ),
                )
                .child(
                    uniform_list("outline-list", items.len(), move |range, _window, _cx| {
                        range
                            .map(|i| {
                                let item = &items[i];
                                let offset = item.offset;
                                let on_jump = on_jump.clone();
                                let level = item.level.clamp(1, 6);
                                let current = current_ix == Some(i);
                                let text = if item.text.is_empty() {
                                    "Untitled".to_string()
                                } else {
                                    item.text.clone()
                                };
                                div()
                                    .id(("outline-row", i))
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .h(px(OUTLINE_ROW_H))
                                    .flex_none()
                                    .pr(px(8.0))
                                    .pl(px(8.0 + 10.0 * (level.min(3) - 1) as f32))
                                    .rounded(px(6.0))
                                    .cursor_pointer()
                                    .when(current, |s| s.bg(outline_current))
                                    .when(!current, |s| s.hover(move |s| s.bg(row_hover)))
                                    .on_click(move |_, window, cx| on_jump(offset, window, cx))
                                    .child(
                                        div()
                                            .flex_none()
                                            .w(px(22.0))
                                            .text_size(px(10.5))
                                            .font_family(Theme::FONT_MONO)
                                            .text_color(text_faint)
                                            .child(format!("H{level}")),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .truncate()
                                            .text_size(px(14.0))
                                            .text_color(if current { text_hi } else { row_text })
                                            .child(text),
                                    )
                            })
                            .collect()
                    })
                    .h(px(list_h))
                    .px(px(4.0))
                    .pb(px(6.0)),
                ),
        )
    } else {
        // Lines that fit under the top edge, never more than the cap.
        let cap = (((area - 2.0 * OUTLINE_EDGE - 16.0) / STRIP_PITCH)
            .floor()
            .max(0.0) as usize)
            .min(STRIP_MAX_LINES);
        let lines = strip_sample(props.items.len(), cap, props.current);
        let on_open = props.on_open;
        let widths: Vec<f32> = lines
            .iter()
            .map(|(index, _)| strip_width(props.items[*index].level))
            .collect();
        if area_w > 0.0 && area_w < OUTLINE_STRIP_MIN_EDITOR_W {
            return root;
        }
        root.right(px(0.0))
            .w(px(OUTLINE_STRIP_W))
            .py(px(8.0))
            .pr(px(8.0))
            .flex()
            .flex_col()
            .items_end()
            .gap(px(STRIP_PITCH - 2.0))
            .cursor_pointer()
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                on_open(window, cx);
                cx.stop_propagation();
            })
            .children(lines.into_iter().zip(widths).map(|((_, on), width)| {
                div()
                    .flex_none()
                    .h(px(3.0))
                    .w(px(width))
                    .rounded(px(1.5))
                    .bg(if on {
                        theme.text_hi
                    } else {
                        theme.border_block
                    })
            }))
    }
}
