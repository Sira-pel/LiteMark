//! The note outline: which headings it lists, and jumping to one.

use super::*;
use crate::model::outline::{
    can_skip_outline_patch, current_heading, outline_items, outline_items_and_fences_raw,
    patch_outline_items, patch_outline_items_raw, OutlineItem,
};

fn outline_max_chars(items: &[OutlineItem]) -> usize {
    items
        .iter()
        .map(|i| i.text.chars().count())
        .max()
        .unwrap_or(0)
}

impl LiteMarkApp {
    /// Headings of the active tab, kept until the text changes. Empty for
    /// tabs that are not Markdown.
    fn outline_headings(&mut self) -> Arc<Vec<OutlineItem>> {
        let view_mode = self.effective_view_mode();
        let Some(tab) = self.tabs.iter_mut().find(|t| t.id == self.active_tab_id) else {
            return Arc::default();
        };
        if !tab.file_type.supports_markdown_features() {
            return Arc::default();
        }
        if view_mode == ViewMode::Raw {
            if let Some(cache) = &self.outline_cache {
                if cache.tab_id == tab.id
                    && cache.revision == tab.revision
                    && cache.parse_gen.is_none()
                {
                    return cache.items.clone();
                }
            }
            let last_edit = tab.last_edit();
            let line_delta = tab.last_edit_line_delta();
            if let (Some(cache), Some((edit_rev, old_range, new_len))) =
                (&mut self.outline_cache, last_edit)
            {
                if cache.tab_id == tab.id && cache.parse_gen.is_none() && cache.revision == edit_rev
                {
                    let items_mut = Arc::make_mut(&mut cache.items);
                    let patched = patch_outline_items_raw(
                        items_mut,
                        &mut cache.raw_fences,
                        &tab.content,
                        old_range,
                        new_len,
                        line_delta,
                        |prefix_items, win_start| {
                            if let Some(prev) = prefix_items.last() {
                                prev.line_index
                                    + crate::model::text_ops::count_line_endings(
                                        &tab.content.as_bytes()[prev.offset..win_start],
                                    ) as u32
                            } else {
                                crate::model::text_ops::count_line_endings(
                                    &tab.content.as_bytes()[..win_start],
                                ) as u32
                            }
                        },
                    );
                    if patched {
                        cache.revision = tab.revision;
                        cache.max_chars = outline_max_chars(&cache.items);
                        return cache.items.clone();
                    }
                }
            }
            let (raw_items, raw_fences) = outline_items_and_fences_raw(&tab.content);
            let max_chars = outline_max_chars(&raw_items);
            let items = Arc::new(raw_items);
            self.outline_cache = Some(OutlineCache {
                tab_id: tab.id.clone(),
                revision: tab.revision,
                parse_gen: None,
                items: items.clone(),
                raw_fences,
                max_chars,
            });
            return items;
        }

        if let Some(cache) = &self.outline_cache {
            if cache.tab_id == tab.id && cache.revision == tab.revision && cache.parse_gen.is_some()
            {
                return cache.items.clone();
            }
        }
        let parsed = tab.parsed();
        let parse_gen = parsed.meta.generation;
        if let Some(cache) = &mut self.outline_cache {
            if cache.tab_id == tab.id {
                if let (Some(old_gen), Some((splice_gen, restart, removed, inserted))) =
                    (cache.parse_gen, parsed.meta.last_splice)
                {
                    if old_gen == splice_gen {
                        if !can_skip_outline_patch(
                            &cache.items,
                            &parsed,
                            restart,
                            removed,
                            inserted,
                            parsed.meta.last_byte_delta,
                        ) {
                            patch_outline_items(
                                Arc::make_mut(&mut cache.items),
                                &parsed,
                                &tab.content,
                                restart,
                                removed,
                                inserted,
                                parsed.meta.last_byte_delta,
                            );
                            cache.max_chars = outline_max_chars(&cache.items);
                        }
                        cache.revision = tab.revision;
                        cache.parse_gen = Some(parse_gen);
                        return cache.items.clone();
                    }
                }
            }
        }
        let raw_items = outline_items(&parsed, &tab.content);
        let max_chars = outline_max_chars(&raw_items);
        let items = Arc::new(raw_items);
        self.outline_cache = Some(OutlineCache {
            tab_id: tab.id.clone(),
            revision: tab.revision,
            parse_gen: Some(parse_gen),
            items: items.clone(),
            raw_fences: Vec::new(),
            max_chars,
        });
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
        let max_chars = self
            .outline_cache
            .as_ref()
            .map(|c| c.max_chars)
            .unwrap_or(0);
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
                items: items.clone(),
                max_chars,
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
