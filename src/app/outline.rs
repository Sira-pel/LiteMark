//! The note outline: which headings it lists, and jumping to one.

use super::*;
use crate::model::outline::{current_heading, outline_items, OutlineItem};

impl LiteMarkApp {
    /// Headings of the active tab, kept until the text changes. Empty for
    /// tabs that are not Markdown.
    fn outline_headings(&mut self) -> Arc<Vec<OutlineItem>> {
        let Some(tab) = self.tabs.iter_mut().find(|t| t.id == self.active_tab_id) else {
            return Arc::default();
        };
        if !tab.file_type.supports_markdown_features() {
            return Arc::default();
        }
        let key = (tab.id.clone(), tab.revision);
        if let Some((cached, items)) = &self.outline_cache {
            if *cached == key {
                return items.clone();
            }
        }
        let items = Arc::new(outline_items(&tab.parsed()));
        self.outline_cache = Some((key, items.clone()));
        items
    }

    /// The outline is drawn: a Markdown tab with at least one heading, and
    /// not in zen mode.
    pub(super) fn outline_visible(&mut self) -> bool {
        !self.zen_mode && !self.outline_headings().is_empty()
    }

    /// The card is pinned and the editor is wide enough to keep it open
    /// beside the text. In a narrow editor a pinned card would squeeze the
    /// text, so it shows as the strip instead (and still opens on hover).
    fn outline_pinned_open(&self) -> bool {
        let width = self.editor_scroll.bounds().size.width;
        // Before the first layout the width is unknown: keep it pinned.
        self.settings.outline_pinned
            && (width <= px(0.0) || width >= px(OUTLINE_PINNED_MIN_EDITOR_W))
    }

    /// Whether the open card (not just the strip) is showing.
    pub(super) fn outline_expanded(&self) -> bool {
        self.outline_pinned_open() || self.outline_hover || self.outline_open
    }

    /// The outline element and the right padding the editor text needs for
    /// it, or `None` when there is no outline to show.
    pub(super) fn outline_element(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
        theme: &Theme,
    ) -> Option<AnyElement> {
        if !self.outline_visible() {
            return None;
        }
        let items = self.outline_headings();
        let caret = self.active_tab()?.cursor_offset();
        let measured = self.editor_scroll.bounds().size.height;
        let area_height = if measured > px(0.0) {
            measured
        } else {
            (window.viewport_size().height - px(120.0)).max(px(120.0))
        };
        let expanded = self.outline_expanded();
        let view = cx.entity().downgrade();
        let element = render_outline(
            OutlineProps {
                current: current_heading(&items, caret),
                items: items.to_vec(),
                expanded,
                area_height,
                area_width: self.editor_scroll.bounds().size.width,
                on_hover: {
                    let view = view.clone();
                    Box::new(move |hovered, _window, cx| {
                        let _ = view.update(cx, |this, cx| {
                            if this.outline_hover != hovered {
                                this.outline_hover = hovered;
                                cx.notify();
                            }
                        });
                    })
                },
                on_open: {
                    let view = view.clone();
                    Box::new(move |_window, cx| {
                        let _ = view.update(cx, |this, cx| {
                            this.outline_open = true;
                            cx.notify();
                        });
                    })
                },
                on_close: {
                    let view = view.clone();
                    Box::new(move |_window, cx| {
                        let _ = view.update(cx, |this, cx| {
                            // The card is already showing under the pointer
                            // (hover), so a click on its title pins it open,
                            // and a click on an open card closes it.
                            this.outline_open = !this.outline_open;
                            if !this.outline_open {
                                this.outline_hover = false;
                            }
                            cx.notify();
                        });
                    })
                },
                on_jump: Rc::new(move |offset, _window, cx| {
                    let _ = view.update(cx, |this, cx| {
                        this.outline_open = false;
                        this.jump_to_heading(offset);
                        cx.notify();
                    });
                }),
            },
            theme,
        )
        .into_any_element();
        // The strip and the card (pinned or hovered) float over the editor's
        // right edge; the text never moves aside for them, in either
        // alignment.
        Some(element)
    }

    /// Puts the caret at the start of the heading line at `offset`, scrolls
    /// it near the top of the view, and hands the keyboard back to the editor.
    pub fn jump_to_heading(&mut self, offset: usize) {
        let Some(tab) = self.active_tab_mut() else {
            return;
        };
        tab.place(offset, false);
        self.field_focus = TextInput::Editor;
        self.field_owns_keys = false;
        self.applied_focus = None;
        self.scroll_caret_to_top();
    }
}
