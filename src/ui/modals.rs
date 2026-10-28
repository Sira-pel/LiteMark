use super::primitives::{kbd_badge, render_icon};
use crate::theme::Theme;
use gpui::prelude::*;
use gpui::*;

pub fn render_shortcuts_modal<FClose>(
    max_h: Pixels,
    focus_handle: FocusHandle,
    on_close: FClose,
    theme: &Theme,
) -> impl IntoElement
where
    FClose: Fn(&mut App) + 'static + Clone,
{
    let shortcuts = [
        ("Ctrl + B", "Toggle Bold"),
        ("Ctrl + I", "Toggle Italic"),
        ("Ctrl + U", "Toggle Underline"),
        ("Ctrl + Shift + X", "Toggle Strikethrough"),
        ("Ctrl + 1 / 2 / 3", "Heading 1 / 2 / 3"),
        ("Ctrl + E", "Toggle Inline Code"),
        ("Ctrl + K", "Insert Link"),
        ("Ctrl + Shift + C", "Insert Code Block"),
        ("Ctrl + S", "Save Document"),
        ("Ctrl + Shift + S", "Save Document As"),
        ("Ctrl + O", "Open File (.md, .txt, .pdf)"),
        ("Ctrl + N", "New Note"),
        ("Ctrl + Shift + N", "New Folder"),
        ("Ctrl + T", "New Tab"),
        ("Ctrl + W", "Close Active Tab"),
        ("Ctrl + F", "Find in Document"),
        ("Ctrl + H", "Find & Replace"),
        ("Ctrl + Shift + M", "Toggle WYSIWYG / Raw Mode"),
        ("Ctrl + Shift + L", "Toggle Line Numbers (Raw Mode)"),
        ("Ctrl + + / =", "Zoom In"),
        ("Ctrl + -", "Zoom Out"),
        ("Ctrl + 0", "Reset Zoom (100%)"),
        ("Ctrl + Z", "Undo"),
        ("Ctrl + Y / Ctrl + Shift + Z", "Redo"),
        ("Ctrl + A", "Select All"),
        ("Ctrl + Left / Right", "Jump Word Left / Right"),
        ("Ctrl + Shift + Left / Right", "Select Word Left / Right"),
        ("Ctrl + Backspace / Delete", "Delete Word Left / Right"),
        ("Ctrl + Shift + Backspace / Delete", "Delete to Line Start / End"),
        ("Ctrl + Up / Down", "Move Line Up / Down"),
        ("Tab / Shift + Tab", "Indent / Outdent List or Line"),
        ("Ctrl + \\", "Toggle Notes Folder Sidebar"),
        ("Up / Down / Left / Right", "Move Through the Sidebar (when focused)"),
        ("Delete", "Move Selected Note or Folder to Trash (sidebar)"),
        ("F1", "Help & Shortcuts"),
        ("Alt + F / E / V", "Open File / Edit / View Menu"),
        ("F10", "Focus Menu Bar"),
        ("F11", "Toggle Zen Mode"),
        ("Escape", "Exit Zen Mode / Close Modals & Menus"),
    ];

    let cls = on_close.clone();

    div()
        .id("shortcuts-modal-backdrop")
        .key_context("PromptModal")
        .track_focus(&focus_handle)
        .absolute()
        .inset_0()
        .bg(rgba(0x00000099))
        .flex()
        .items_center()
        .justify_center()
        .p(px(16.0))
        .occlude()
        .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
        .child(
            div()
                .id("shortcuts-modal-card")
                .flex()
                .flex_col()
                .w(px(520.0))
                .max_w_full()
                .max_h(max_h)
                .rounded(px(12.0))
                .bg(theme.bg_modal)
                .border_1()
                .border_color(theme.border_surface)
                .shadow_2xl()
                .overflow_hidden()
                .occlude()
                .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                // Header
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .px(px(20.0))
                        .py(px(14.0))
                        .bg(theme.bg_modal_header)
                        .border_b_1()
                        .border_color(theme.border_subtle)
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .child(render_icon("keyboard", 16.0, theme.text_primary))
                                .child(
                                    div()
                                        .font_weight(FontWeight::BOLD)
                                        .text_size(px(14.0))
                                        .text_color(theme.text_primary)
                                        .child("Keyboard Shortcuts"),
                                ),
                        )
                        .child({
                            let c = cls.clone();
                            div()
                                .id("shortcuts-close-x")
                                .p(px(4.0))
                                .rounded(px(4.0))
                                .cursor_pointer()
                                .hover(|h| h.bg(theme.bg_hover))
                                .on_click(move |_, _, cx| c(cx))
                                .child(render_icon("x", 16.0, theme.text_muted))
                        }),
                )
                // Grid of shortcuts
                .child(
                    div()
                        .id("shortcuts-scroll")
                        .flex_1()
                        .overflow_y_scroll()
                        .p(px(20.0))
                        .child(div().flex().flex_col().gap(px(6.0)).children(
                            shortcuts.into_iter().map(|(key, desc)| {
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .px(px(10.0))
                                    .py(px(6.0))
                                    .rounded(px(6.0))
                                    .bg(theme.bg_inset)
                                    .border_1()
                                    .border_color(theme.border_subtle)
                                    .child(
                                        div()
                                            .text_size(px(12.0))
                                            .text_color(theme.text_muted)
                                            .child(desc),
                                    )
                                    .child(kbd_badge(key, theme))
                            }),
                        )),
                )
                // Footer
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_end()
                        .px(px(20.0))
                        .py(px(12.0))
                        .bg(theme.bg_modal_header)
                        .border_t_1()
                        .border_color(theme.border_subtle)
                        .child({
                            let c = cls.clone();
                            div()
                                .id("shortcuts-got-it-btn")
                                .px(px(16.0))
                                .py(px(6.0))
                                .rounded(px(6.0))
                                .bg(theme.row_active)
                                .text_color(theme.text_strong)
                                .hover(|h| h.bg(theme.row_hover))
                                .font_weight(FontWeight::BOLD)
                                .text_size(px(12.0))
                                .cursor_pointer()
                                .on_click(move |_, _, cx| c(cx))
                                .child("Got it")
                        }),
                ),
        )
}

pub fn render_pdf_preview_modal<FClose, FOpenExternal>(
    title: String,
    page_count: Option<usize>,
    selected_ix: usize,
    focus_handle: FocusHandle,
    on_close: FClose,
    on_open_external: FOpenExternal,
    theme: &Theme,
) -> impl IntoElement
where
    FClose: Fn(&mut App) + 'static + Clone,
    FOpenExternal: Fn(&mut App) + 'static + Clone,
{
    let cls = on_close.clone();
    let ext = on_open_external.clone();

    div()
        .id("pdf-modal-backdrop")
        .key_context("PromptModal")
        .track_focus(&focus_handle)
        .absolute()
        .inset_0()
        .bg(rgba(0x00000099))
        .flex()
        .items_center()
        .justify_center()
        .p(px(16.0))
        .occlude()
        .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
        .child(
            div()
                .id("pdf-modal-card")
                .flex()
                .flex_col()
                .w(px(560.0))
                .max_w_full()
                .rounded(px(12.0))
                .bg(theme.bg_modal)
                .border_1()
                .border_color(theme.border_surface)
                .shadow_2xl()
                .overflow_hidden()
                .occlude()
                .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .px(px(20.0))
                        .py(px(14.0))
                        .bg(theme.bg_modal_header)
                        .border_b_1()
                        .border_color(theme.border_subtle)
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .min_w_0()
                                .gap(px(8.0))
                                .child(render_icon("file-text", 18.0, theme.text_muted))
                                .child(
                                    div()
                                        .min_w_0()
                                        .truncate()
                                        .font_weight(FontWeight::BOLD)
                                        .text_size(px(14.0))
                                        .text_color(theme.text_primary)
                                        .child(format!("PDF Document ({})", title))
                                )
                        )
                        .child({
                            let c = cls.clone();
                            div()
                                .id("pdf-modal-close-x")
                                .p(px(4.0))
                                .rounded(px(4.0))
                                .cursor_pointer()
                                .hover(|h| h.bg(theme.bg_hover))
                                .on_click(move |_, _, cx| c(cx))
                                .child(render_icon("x", 16.0, theme.text_muted))
                        })
                )
                .child(
                    div()
                        .p(px(20.0))
                        .flex()
                        .flex_col()
                        .gap(px(12.0))
                        .text_size(px(12.0))
                        .text_color(theme.text_primary)
                        .child(format!(
                            "This PDF document contains {} pages. The text has been extracted into your active Markdown editor tab.",
                            page_count.unwrap_or(1)
                        ))
                        .child(
                            div()
                                .flex()
                                .gap(px(10.0))
                                .child({
                                    let oe = ext.clone();
                                    let is_selected = selected_ix == 0;
                                    div()
                                        .id("pdf-open-ext-btn")
                                        .flex()
                                        .items_center()
                                        .gap(px(6.0))
                                        .px(px(14.0))
                                        .py(px(8.0))
                                        .rounded(px(6.0))
                                        .bg(theme.bg_hover)
                                        .border_1()
                                        .border_color(if is_selected {
                                            theme.text_hi
                                        } else {
                                            theme.border_primary
                                        })
                                        .cursor_pointer()
                                        .hover(|h| h.bg(theme.bg_active))
                                        .on_click(move |_, _, cx| oe(cx))
                                        .child(render_icon("external-link", 14.0, theme.text_primary))
                                        .child("Open in Ubuntu Document Viewer")
                                })
                        )
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_end()
                        .px(px(20.0))
                        .py(px(12.0))
                        .bg(theme.bg_modal_header)
                        .border_t_1()
                        .border_color(theme.border_subtle)
                        .child({
                            let c = cls.clone();
                            let is_selected = selected_ix == 1;
                            div()
                                .id("pdf-modal-close-btn")
                                .px(px(16.0))
                                .py(px(6.0))
                                .rounded(px(6.0))
                                .bg(theme.row_active)
                                .border_1()
                                .border_color(if is_selected {
                                    theme.text_hi
                                } else {
                                    rgba(0x00000000)
                                })
                                .text_color(theme.text_strong)
                                .hover(|h| h.bg(theme.row_hover))
                                .font_weight(FontWeight::BOLD)
                                .text_size(px(12.0))
                                .cursor_pointer()
                                .on_click(move |_, _, cx| c(cx))
                                .child("Close")
                        })
                )
        )
}

pub fn render_about_modal<FClose>(
    focus_handle: FocusHandle,
    on_close: FClose,
    theme: &Theme,
) -> impl IntoElement
where
    FClose: Fn(&mut App) + 'static + Clone,
{
    let cls = on_close.clone();
    let version = env!("CARGO_PKG_VERSION");

    div()
        .id("about-modal-backdrop")
        .key_context("PromptModal")
        .track_focus(&focus_handle)
        .absolute()
        .inset_0()
        .bg(rgba(0x00000099))
        .flex()
        .items_center()
        .justify_center()
        .p(px(16.0))
        .occlude()
        .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
        .child(
            div()
                .id("about-modal-card")
                .flex()
                .flex_col()
                .w(px(420.0))
                .max_w_full()
                .rounded(px(12.0))
                .bg(theme.bg_modal)
                .border_1()
                .border_color(theme.border_surface)
                .shadow_2xl()
                .overflow_hidden()
                .occlude()
                .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                // Header
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .px(px(20.0))
                        .py(px(14.0))
                        .bg(theme.bg_modal_header)
                        .border_b_1()
                        .border_color(theme.border_subtle)
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .child(render_icon("help-circle", 16.0, theme.text_primary))
                                .child(
                                    div()
                                        .font_weight(FontWeight::BOLD)
                                        .text_size(px(14.0))
                                        .text_color(theme.text_primary)
                                        .child("About LiteMark")
                                )
                        )
                        .child({
                            let c = cls.clone();
                            div()
                                .id("about-close-x")
                                .p(px(4.0))
                                .rounded(px(4.0))
                                .cursor_pointer()
                                .hover(|h| h.bg(theme.bg_hover))
                                .on_click(move |_, _, cx| c(cx))
                                .child(render_icon("x", 16.0, theme.text_muted))
                        })
                )
                // Content
                .child(
                    div()
                        .p(px(24.0))
                        .flex()
                        .flex_col()
                        .items_center()
                        .text_center()
                        .gap(px(12.0))
                        .child(
                            div()
                                .w(px(56.0))
                                .h(px(56.0))
                                .rounded(px(12.0))
                                .bg(theme.row_active)
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(render_icon("folder-tree", 28.0, theme.text_hi))
                        )
                        .child(
                            div()
                                .text_size(px(18.0))
                                .font_weight(FontWeight::BOLD)
                                .text_color(theme.text_strong)
                                .child("LiteMark")
                        )
                        .child(
                            div()
                                .text_size(px(12.0))
                                .text_color(theme.text_muted)
                                .child(format!("Version {} (beta)", version))
                        )
                        .child(
                            div()
                                .text_size(px(13.0))
                                .text_color(theme.text_primary)
                                .line_height(relative(1.5))
                                .child("A native Markdown and text editor.")
                        )
                )
                // Footer
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_end()
                        .px(px(20.0))
                        .py(px(12.0))
                        .bg(theme.bg_modal_header)
                        .border_t_1()
                        .border_color(theme.border_subtle)
                        .child({
                            let c = cls.clone();
                            div()
                                .id("about-close-btn")
                                .px(px(16.0))
                                .py(px(6.0))
                                .rounded(px(6.0))
                                .bg(theme.row_active)
                                .text_color(theme.text_strong)
                                .hover(|h| h.bg(theme.row_hover))
                                .font_weight(FontWeight::BOLD)
                                .text_size(px(12.0))
                                .cursor_pointer()
                                .on_click(move |_, _, cx| c(cx))
                                .child("Close")
                        })
                )
        )
}

pub fn render_prompt_modal<FChoose>(
    title: impl Into<SharedString>,
    message: impl Into<SharedString>,
    choices: Vec<(&'static str, bool)>,
    selected_ix: usize,
    focus_handle: FocusHandle,
    on_choose: FChoose,
    theme: &Theme,
) -> impl IntoElement
where
    FChoose: Fn(&'static str, &mut App) + 'static + Clone,
{
    let title = title.into();
    let message = message.into();
    div()
        .id("prompt-modal-backdrop")
        .key_context("PromptModal")
        .track_focus(&focus_handle)
        .absolute()
        .inset_0()
        .bg(rgba(0x00000099))
        .flex()
        .items_center()
        .justify_center()
        .p(px(16.0))
        .occlude()
        .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
        .child(
            div()
                .id("prompt-modal-card")
                .flex()
                .flex_col()
                .w(px(420.0))
                .max_w_full()
                .rounded(px(12.0))
                .bg(theme.bg_modal)
                .border_1()
                .border_color(theme.border_surface)
                .shadow_2xl()
                .overflow_hidden()
                .occlude()
                .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                .child(
                    div()
                        .px(px(20.0))
                        .py(px(14.0))
                        .bg(theme.bg_modal_header)
                        .border_b_1()
                        .border_color(theme.border_subtle)
                        .font_weight(FontWeight::BOLD)
                        .text_size(px(14.0))
                        .text_color(theme.text_primary)
                        .child(title),
                )
                .child(
                    div()
                        .px(px(20.0))
                        .py(px(16.0))
                        .text_size(px(13.0))
                        .text_color(theme.text_primary)
                        .child(message),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_end()
                        .gap(px(8.0))
                        .px(px(16.0))
                        .py(px(12.0))
                        .bg(theme.bg_modal_header)
                        .border_t_1()
                        .border_color(theme.border_subtle)
                        .children(choices.into_iter().enumerate().map(|(ix, (label, primary))| {
                            let choose = on_choose.clone();
                            let is_selected = ix == selected_ix;
                            div()
                                .id(SharedString::from(format!("prompt-{label}")))
                                .px(px(14.0))
                                .py(px(6.0))
                                .rounded(px(6.0))
                                .cursor_pointer()
                                .font_weight(FontWeight::MEDIUM)
                                .text_size(px(12.0))
                                .bg(if primary {
                                    theme.text_hi
                                } else {
                                    theme.bg_hover
                                })
                                .border_1()
                                .border_color(if is_selected {
                                    theme.text_hi
                                } else {
                                    rgba(0x00000000)
                                })
                                .text_color(if primary {
                                    theme.on_primary
                                } else if is_selected {
                                    theme.text_hi
                                } else {
                                    theme.text_primary
                                })
                                .when(!is_selected, |s| s.opacity(0.75))
                                .on_click(move |_, _, cx| choose(label, cx))
                                .child(label)
                        })),
                ),
        )
}
