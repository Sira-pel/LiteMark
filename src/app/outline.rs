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

    /// Whether the open card (not just the strip) is showing.
    pub(super) fn outline_expanded(&self) -> bool {
        self.settings.outline_pinned || self.outline_hover
    }

    /// The outline element and the right padding the editor text needs for
    /// it, or `None` when there is no outline to show.
    pub(super) fn outline_element(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
        theme: &Theme,
    ) -> Option<(AnyElement, Pixels)> {
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
                            this.outline_hover = true;
                            cx.notify();
                        });
                    })
                },
                on_jump: Rc::new(move |offset, _window, cx| {
                    let _ = view.update(cx, |this, cx| {
                        this.jump_to_heading(offset);
                        cx.notify();
                    });
                }),
            },
            theme,
        )
        .into_any_element();
        let inset = if self.settings.outline_pinned {
            px(OUTLINE_PINNED_INSET)
        } else {
            px(0.0)
        };
        Some((element, inset))
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
