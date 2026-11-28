use super::primitives::{kbd_badge, render_icon};
use crate::model::{
    PdfExportSettings, PdfFontSize, PdfMargins, PdfOrientation, PdfPageSize, PdfTheme,
};
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
    let mac = cfg!(target_os = "macos");
    let cmd = if mac { "Cmd" } else { "Ctrl" };
    let sec = |k: &str| k.replace("Ctrl", cmd);
    let nav = |other: &str, mac_label: &str| (if mac { mac_label } else { other }).to_string();
    let mut shortcuts: Vec<(String, &str)> = vec![
        (sec("Ctrl + B"), "Toggle Bold"),
        (sec("Ctrl + I"), "Toggle Italic"),
        (sec("Ctrl + U"), "Toggle Underline"),
        (sec("Ctrl + Shift + X"), "Toggle Strikethrough"),
        (sec("Ctrl + Shift + H"), "Toggle Highlight"),
        (sec("Ctrl + 1 – 6"), "Heading 1 – 6"),
        (sec("Ctrl + E"), "Toggle Inline Code"),
        (sec("Ctrl + K"), "Insert Link"),
        (sec("Ctrl + Shift + C"), "Insert Code Block"),
        (sec("Ctrl + S"), "Save Document"),
        (sec("Ctrl + Shift + S"), "Save Document As"),
        (sec("Ctrl + O"), "Open File (.md, .txt, .pdf)"),
        (sec("Ctrl + N"), "New Note"),
        (sec("Ctrl + Shift + N"), "New Folder"),
        (sec("Ctrl + T"), "New Tab"),
        (sec("Ctrl + W"), "Close Active Tab"),
        (sec("Ctrl + F"), "Find in Document"),
        (sec("Ctrl + P"), "Quick Open: Find a Note by Name"),
        (sec("Ctrl + Shift + F"), "Search Inside All Notes"),
        (
            if mac {
                "Cmd + Alt + F".to_string()
            } else {
                "Ctrl + H".to_string()
            },
            "Find & Replace",
        ),
        (sec("Ctrl + Shift + M"), "Toggle WYSIWYG / Raw Mode"),
        (sec("Ctrl + Shift + L"), "Toggle Line Numbers (Raw Mode)"),
        (sec("Ctrl + Shift + O"), "Pin / Unpin Outline"),
        (sec("Ctrl + + / ="), "Zoom In"),
        (sec("Ctrl + -"), "Zoom Out"),
        (sec("Ctrl + 0"), "Reset Zoom (100%)"),
        (sec("Ctrl + Z"), "Undo"),
        (sec("Ctrl + Y / Ctrl + Shift + Z"), "Redo"),
        (sec("Ctrl + A"), "Select All"),
        (
            nav("Ctrl + Left / Right", "Alt + Left / Right"),
            "Jump Word Left / Right",
        ),
        (
            nav("Ctrl + Shift + Left / Right", "Alt + Shift + Left / Right"),
            "Select Word Left / Right",
        ),
        (
            "Ctrl + Shift + Up / Down".to_string(),
            "Select Line Up / Down",
        ),
        (
            nav("Ctrl + Home / End", "Cmd + Up / Down"),
            "Jump to Document Start / End",
        ),
        (
            nav("Ctrl + Backspace / Delete", "Alt + Backspace / Delete"),
            "Delete Word Left / Right",
        ),
        (
            nav(
                "Ctrl + Shift + Backspace / Delete",
                "Cmd + Backspace / Delete",
            ),
            "Delete to Line Start / End",
        ),
        ("Alt + Up / Down".to_string(), "Move Line Up / Down"),
        (
            "Ctrl + Tab / Ctrl + Shift + Tab".to_string(),
            "Next / Previous Tab",
        ),
        (
            "Tab / Shift + Tab".to_string(),
            "Indent / Outdent List or Line",
        ),
        (
            "Shift + Enter".to_string(),
            "New Line Without List or Quote Continuation",
        ),
        (sec("Ctrl + Enter"), "Add Table Row Below"),
        (sec("Ctrl + \\"), "Toggle Notes Folder Sidebar"),
        (
            "Up / Down / Left / Right".to_string(),
            "Move Through the Sidebar (when focused)",
        ),
        (
            "Delete".to_string(),
            "Move Selected Note or Folder to Trash (sidebar)",
        ),
        (
            sec("Ctrl + X / C / V"),
            "Cut / Copy / Paste Selected Notes or Folders (sidebar)",
        ),
        (
            sec("Ctrl + D"),
            "Duplicate Selected Note or Folder (sidebar)",
        ),
        ("F1".to_string(), "Help & Shortcuts"),
        (
            "Alt + F / E / V".to_string(),
            "Open File / Edit / View Menu",
        ),
        ("F10".to_string(), "Focus Menu Bar"),
        ("F11".to_string(), "Toggle Zen Mode"),
        ("Escape".to_string(), "Exit Zen Mode / Close Modals & Menus"),
    ];
    if mac {
        shortcuts.extend([
            ("Ctrl + A / E".to_string(), "Line Start / End"),
            ("Ctrl + K".to_string(), "Delete to Line End"),
            ("Cmd + ,".to_string(), "Open Settings Menu"),
            ("Cmd + M".to_string(), "Minimize Window"),
            ("Cmd + Q".to_string(), "Quit"),
        ]);
    }

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
                                    .child(kbd_badge(&key, theme))
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
                                        .child("About LiteMark"),
                                ),
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
                        }),
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
                                .child(render_icon("folder-tree", 28.0, theme.text_hi)),
                        )
                        .child(
                            div()
                                .text_size(px(18.0))
                                .font_weight(FontWeight::BOLD)
                                .text_color(theme.text_strong)
                                .child("LiteMark"),
                        )
                        .child(
                            div()
                                .text_size(px(12.0))
                                .text_color(theme.text_muted)
                                .child(format!("Version {} (beta)", version)),
                        )
                        .child(
                            div()
                                .text_size(px(13.0))
                                .text_color(theme.text_primary)
                                .line_height(relative(1.5))
                                .child("A native Markdown and text editor."),
                        ),
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
                        }),
                ),
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
                        .children(
                            choices
                                .into_iter()
                                .enumerate()
                                .map(|(ix, (label, primary))| {
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
                                }),
                        ),
                ),
        )
}

#[allow(clippy::too_many_arguments)]
pub fn render_pdf_export_modal<FUpdate, FConfirm, FClose>(
    note_title: String,
    options: PdfExportSettings,
    focus_handle: FocusHandle,
    on_update: FUpdate,
    on_confirm: FConfirm,
    on_close: FClose,
    theme: &Theme,
) -> impl IntoElement
where
    FUpdate: Fn(PdfExportSettings, &mut App) + 'static + Clone,
    FConfirm: Fn(&mut App) + 'static + Clone,
    FClose: Fn(&mut App) + 'static + Clone,
{
    let close_bg = on_close.clone();
    let close_x = on_close.clone();
    let close_btn = on_close;
    let confirm = on_confirm;

    let segmented_row = |label: &'static str, buttons: Vec<AnyElement>| {
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .child(
                div()
                    .w(px(96.0))
                    .flex_shrink_0()
                    .text_size(px(12.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text_muted)
                    .child(label),
            )
            .child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .gap(px(2.0))
                    .p(px(2.0))
                    .rounded(px(6.0))
                    // Recessed track: a step darker than the modal in both themes.
                    .bg(theme.bg_modal_header)
                    .border_1()
                    .border_color(theme.border_subtle)
                    .children(buttons),
            )
    };

    let make_seg_btn = |id: &'static str,
                        text: &'static str,
                        swatch: Option<u32>,
                        active: bool,
                        next: PdfExportSettings,
                        on_update: FUpdate,
                        theme: &Theme|
     -> AnyElement {
        // The selected option is a raised chip: a step lighter than the modal
        // (white in light mode) with a hairline edge and full-strength text,
        // sitting on a recessed track. Unselected options use muted text, so
        // the choice reads clearly without competing with the Export button.
        let dark = theme.bg_modal.r < 0.5;
        let (active_bg, active_border) = if dark {
            (rgb(0x3a3a3a), rgb(0x4a4a4a))
        } else {
            (rgb(0xffffff), theme.border_primary)
        };
        let hover_bg = theme.bg_hover;
        let text_color = if active {
            theme.text_strong
        } else {
            theme.text_muted
        };
        let swatch_border = theme.text_muted;
        div()
            .id(id)
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .gap(px(5.0))
            .py(px(5.0))
            .px(px(6.0))
            .rounded(px(4.0))
            .text_size(px(12.0))
            .font_weight(if active {
                FontWeight::SEMIBOLD
            } else {
                FontWeight::NORMAL
            })
            .text_color(text_color)
            .bg(if active { active_bg } else { rgba(0x00000000) })
            .border_1()
            .border_color(if active {
                active_border
            } else {
                rgba(0x00000000)
            })
            .cursor_pointer()
            .when(!active, |s| s.hover(move |h| h.bg(hover_bg)))
            .on_click(move |_, _, cx| on_update(next, cx))
            .when_some(swatch, |s, hex| {
                s.child(
                    div()
                        .w(px(9.0))
                        .h(px(9.0))
                        .rounded_full()
                        .bg(rgb(hex))
                        .border_1()
                        .border_color(swatch_border),
                )
            })
            .child(text)
            .into_any_element()
    };

    let page_size_row = segmented_row(
        "Page size",
        vec![
            make_seg_btn(
                "pdf-opt-a4",
                "A4",
                None,
                options.page_size == PdfPageSize::A4,
                PdfExportSettings {
                    page_size: PdfPageSize::A4,
                    ..options
                },
                on_update.clone(),
                theme,
            ),
            make_seg_btn(
                "pdf-opt-letter",
                "Letter",
                None,
                options.page_size == PdfPageSize::Letter,
                PdfExportSettings {
                    page_size: PdfPageSize::Letter,
                    ..options
                },
                on_update.clone(),
                theme,
            ),
            make_seg_btn(
                "pdf-opt-legal",
                "Legal",
                None,
                options.page_size == PdfPageSize::Legal,
                PdfExportSettings {
                    page_size: PdfPageSize::Legal,
                    ..options
                },
                on_update.clone(),
                theme,
            ),
        ],
    );

    let orientation_row = segmented_row(
        "Orientation",
        vec![
            make_seg_btn(
                "pdf-opt-portrait",
                "Portrait",
                None,
                options.orientation == PdfOrientation::Portrait,
                PdfExportSettings {
                    orientation: PdfOrientation::Portrait,
                    ..options
                },
                on_update.clone(),
                theme,
            ),
            make_seg_btn(
                "pdf-opt-landscape",
                "Landscape",
                None,
                options.orientation == PdfOrientation::Landscape,
                PdfExportSettings {
                    orientation: PdfOrientation::Landscape,
                    ..options
                },
                on_update.clone(),
                theme,
            ),
        ],
    );

    let margins_row = segmented_row(
        "Margins",
        vec![
            make_seg_btn(
                "pdf-opt-narrow",
                "Narrow",
                None,
                options.margins == PdfMargins::Narrow,
                PdfExportSettings {
                    margins: PdfMargins::Narrow,
                    ..options
                },
                on_update.clone(),
                theme,
            ),
            make_seg_btn(
                "pdf-opt-normal-m",
                "Normal",
                None,
                options.margins == PdfMargins::Normal,
                PdfExportSettings {
                    margins: PdfMargins::Normal,
                    ..options
                },
                on_update.clone(),
                theme,
            ),
            make_seg_btn(
                "pdf-opt-wide",
                "Wide",
                None,
                options.margins == PdfMargins::Wide,
                PdfExportSettings {
                    margins: PdfMargins::Wide,
                    ..options
                },
                on_update.clone(),
                theme,
            ),
        ],
    );

    let theme_row = segmented_row(
        "Theme",
        vec![
            make_seg_btn(
                "pdf-opt-light",
                "Light",
                Some(0xFFFFFF),
                options.theme == PdfTheme::Light,
                PdfExportSettings {
                    theme: PdfTheme::Light,
                    ..options
                },
                on_update.clone(),
                theme,
            ),
            make_seg_btn(
                "pdf-opt-sepia",
                "Sepia",
                Some(0xF8F1E3),
                options.theme == PdfTheme::Sepia,
                PdfExportSettings {
                    theme: PdfTheme::Sepia,
                    ..options
                },
                on_update.clone(),
                theme,
            ),
            make_seg_btn(
                "pdf-opt-dark",
                "Dark",
                Some(0x1E1E1E),
                options.theme == PdfTheme::Dark,
                PdfExportSettings {
                    theme: PdfTheme::Dark,
                    ..options
                },
                on_update.clone(),
                theme,
            ),
        ],
    );

    let font_size_row = segmented_row(
        "Font size",
        vec![
            make_seg_btn(
                "pdf-opt-small",
                "Small",
                None,
                options.font_size == PdfFontSize::Small,
                PdfExportSettings {
                    font_size: PdfFontSize::Small,
                    ..options
                },
                on_update.clone(),
                theme,
            ),
            make_seg_btn(
                "pdf-opt-normal-f",
                "Normal",
                None,
                options.font_size == PdfFontSize::Normal,
                PdfExportSettings {
                    font_size: PdfFontSize::Normal,
                    ..options
                },
                on_update.clone(),
                theme,
            ),
            make_seg_btn(
                "pdf-opt-large",
                "Large",
                None,
                options.font_size == PdfFontSize::Large,
                PdfExportSettings {
                    font_size: PdfFontSize::Large,
                    ..options
                },
                on_update.clone(),
                theme,
            ),
        ],
    );

    let make_check_row = |id: &'static str,
                          label: &'static str,
                          checked: bool,
                          next: PdfExportSettings,
                          on_update: FUpdate,
                          theme: &Theme| {
        let hover_bg = theme.bg_hover;
        div()
            .id(id)
            .flex()
            .items_center()
            .justify_between()
            .px(px(8.0))
            .py(px(6.0))
            .rounded(px(6.0))
            .cursor_pointer()
            .hover(move |h| h.bg(hover_bg))
            .on_click(move |_, _, cx| on_update(next, cx))
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(theme.text_primary)
                    .child(label),
            )
            .child(
                div()
                    .w(px(16.0))
                    .h(px(16.0))
                    .rounded(px(4.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(if checked {
                        theme.text_hi
                    } else {
                        theme.bg_input
                    })
                    .border_1()
                    .border_color(if checked {
                        theme.text_hi
                    } else {
                        theme.text_muted
                    })
                    .when(checked, |s| {
                        s.child(render_icon("check", 11.0, theme.on_primary))
                    }),
            )
    };

    let page_numbers_row = make_check_row(
        "pdf-opt-page-numbers",
        "Show page numbers",
        options.page_numbers,
        PdfExportSettings {
            page_numbers: !options.page_numbers,
            ..options
        },
        on_update.clone(),
        theme,
    );

    let open_after_row = make_check_row(
        "pdf-opt-open-after",
        "Open PDF after export",
        options.open_after_export,
        PdfExportSettings {
            open_after_export: !options.open_after_export,
            ..options
        },
        on_update,
        theme,
    );

    div()
        .id("pdf-export-modal-backdrop")
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
        .on_mouse_down(MouseButton::Left, move |_, _, cx| close_bg(cx))
        .child(
            div()
                .id("pdf-export-modal-card")
                .flex()
                .flex_col()
                .w(px(450.0))
                .max_w_full()
                .rounded(px(12.0))
                .bg(theme.bg_modal)
                .border_1()
                .border_color(theme.border_surface)
                .shadow_2xl()
                .overflow_hidden()
                .occlude()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
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
                                .child(render_icon("printer", 16.0, theme.text_muted))
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .min_w_0()
                                        .child(
                                            div()
                                                .font_weight(FontWeight::BOLD)
                                                .text_size(px(14.0))
                                                .text_color(theme.text_primary)
                                                .child("Export as PDF"),
                                        )
                                        .child(
                                            div()
                                                .min_w_0()
                                                .truncate()
                                                .text_size(px(11.0))
                                                .text_color(theme.text_muted)
                                                .child(note_title),
                                        ),
                                ),
                        )
                        .child(
                            div()
                                .id("pdf-export-modal-close-x")
                                .p(px(4.0))
                                .rounded(px(4.0))
                                .cursor_pointer()
                                .hover(|h| h.bg(theme.bg_hover))
                                .on_click(move |_, _, cx| close_x(cx))
                                .child(render_icon("x", 16.0, theme.text_muted)),
                        ),
                )
                .child(
                    div()
                        .px(px(20.0))
                        .py(px(16.0))
                        .flex()
                        .flex_col()
                        .gap(px(10.0))
                        .child(page_size_row)
                        .child(orientation_row)
                        .child(margins_row)
                        .child(theme_row)
                        .child(font_size_row)
                        .child(div().h(px(1.0)).my(px(4.0)).bg(theme.border_subtle))
                        .child(page_numbers_row)
                        .child(open_after_row),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(8.0))
                        .px(px(20.0))
                        .py(px(12.0))
                        .bg(theme.bg_modal_header)
                        .border_t_1()
                        .border_color(theme.border_subtle)
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(theme.text_muted)
                                .child("Press Enter to export"),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .child(
                                    div()
                                        .id("pdf-export-cancel-btn")
                                        .px(px(14.0))
                                        .py(px(6.0))
                                        .rounded(px(6.0))
                                        .cursor_pointer()
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_size(px(12.0))
                                        .bg(theme.bg_hover)
                                        .text_color(theme.text_primary)
                                        .hover(|h| h.bg(theme.bg_active))
                                        .on_click(move |_, _, cx| close_btn(cx))
                                        .child("Cancel"),
                                )
                                .child(
                                    div()
                                        .id("pdf-export-confirm-btn")
                                        .px(px(14.0))
                                        .py(px(6.0))
                                        .rounded(px(6.0))
                                        .cursor_pointer()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_size(px(12.0))
                                        .bg(theme.text_hi)
                                        .text_color(theme.on_primary)
                                        .hover(|h| h.opacity(0.9))
                                        .on_click(move |_, _, cx| confirm(cx))
                                        .child("Export PDF…"),
                                ),
                        ),
                ),
        )
}
