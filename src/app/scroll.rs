//! Scrolling: momentum, caret follow, selection reveal, scroll anchoring.

use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PendingScroll {
    /// Align the caret row near the top of the viewport (Outline jump).
    CaretToTop { tab_id: String, offset: usize },
    /// Reveal the selection start inside the viewport (Find / Quick Open jump).
    SelectionIntoView { tab_id: String, offset: usize },
}

impl LiteMarkApp {
    pub(super) fn wysiwyg_parsed(&mut self) -> Option<Arc<crate::markdown::ParsedDocument>> {
        if self.effective_view_mode() != ViewMode::Wysiwyg {
            return None;
        }
        self.active_tab_mut()
            .filter(|t| t.file_type.supports_markdown_features())
            .map(|t| t.parsed())
    }

    /// Feeds touchpad movement to the momentum tracker; wheel notches reset it.
    pub(super) fn note_scroll(
        &mut self,
        ev: &ScrollWheelEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Manual scrolling cancels any pending jump re-alignment.
        self.pending_scroll = None;
        // Scrolling brings back a pointer hidden by typing or a click.
        if self.pointer_hidden {
            self.pointer_hidden = false;
            cx.notify();
        }
        // macOS already sends its own inertial events after the lift, and
        // Windows only reports line steps, so this is for Linux touchpads.
        match ev.delta {
            _ if !cfg!(target_os = "linux") => {}
            ScrollDelta::Pixels(d) => {
                let now = self.scroll_now();
                self.scroll_momentum.on_event(now, f32::from(d.y));
                cx.notify(); // render asks for the animation frames
            }
            ScrollDelta::Lines(_) => self.scroll_momentum.cancel(),
        }
    }

    fn scroll_now(&self) -> std::time::Instant {
        self.scroll_clock.unwrap_or_else(std::time::Instant::now)
    }

    /// Touchpad momentum: one frame of the coast after the fingers lift.
    pub(super) fn coast_scroll(&mut self, window: &mut Window) {
        if self.scroll_momentum.is_idle() {
            return;
        }
        let now = self.scroll_now();
        let step = self.scroll_momentum.tick(now);
        if let Some(step) = step {
            let offset = self.editor_scroll.offset();
            let max = self.editor_scroll.max_offset().height;
            let y = (offset.y + px(step)).clamp(-max, px(0.0));
            // A zero step (the first frame of a coast) is not the edge.
            if y == offset.y && step != 0.0 {
                self.scroll_momentum.cancel(); // hit the top or bottom
            } else {
                self.editor_scroll.set_offset(point(offset.x, y));
            }
        }
        if !self.scroll_momentum.is_idle() {
            window.request_animation_frame();
        }
    }

    fn follow_caret_key(&self) -> Option<(String, ViewMode, Range<usize>, usize)> {
        let view_mode = self.effective_view_mode();
        let tab = self.active_tab()?;
        Some((
            tab.id.clone(),
            view_mode,
            tab.selection_range(),
            tab.content.len(),
        ))
    }

    pub(crate) fn needs_follow_caret_check(&self) -> bool {
        if self.pending_scroll.is_some() {
            return true;
        }
        let Some(key) = self.follow_caret_key() else {
            return false;
        };
        self.caret_followed.as_ref() != Some(&key)
    }

    /// Keeps the currently-visible lines stationary when unmeasured blocks
    /// strictly above the viewport change height as they enter `ShapingBand`.
    pub(super) fn anchor_scroll_after_paint_impl(
        &mut self,
        delta: Pixels,
        total_height: Pixels,
    ) -> bool {
        if self.pending_scroll.is_some() || delta.abs() <= px(0.5) {
            return false;
        }
        let view = self.editor_scroll.bounds();
        if view.size.height <= px(0.0) {
            return false;
        }
        let offset = self.editor_scroll.offset();
        if offset.y >= px(-0.5) {
            return false;
        }
        let max = self
            .editor_scroll
            .max_offset()
            .height
            .max((total_height - view.size.height).max(px(0.0)));
        let y = (offset.y - delta).clamp(-max, px(0.0));
        if (y - offset.y).abs() > px(0.5) {
            self.editor_scroll.set_offset(point(offset.x, y));
            true
        } else {
            false
        }
    }

    /// Keeps the caret on screen after typing, Enter, mode switches, or caret moves.
    /// Runs after `EditorElement::paint` has updated `editor_paint`, so an in-viewport
    /// keystroke resolves immediately with zero extra frames. Returns `true` only when
    /// the scroll offset actually changed and a redraw is needed.
    pub(super) fn follow_caret_after_paint_impl(&mut self, _window: &mut Window) -> bool {
        let Some(key) = self.follow_caret_key() else {
            self.pending_scroll = None;
            return false;
        };
        let Some(cursor) = self.active_tab().map(|tab| tab.caret.cursor()) else {
            self.pending_scroll = None;
            return false;
        };
        if let Some(pending) = self.pending_scroll.take() {
            let paint_matches = {
                let paint = self.editor_paint.borrow();
                paint.painted_len == key.3
                    && paint.painted_selection == key.2
                    && paint.painted_view_mode == key.1
            };
            if !paint_matches {
                self.pending_scroll = Some(pending);
                return false;
            }
            match pending {
                PendingScroll::CaretToTop { tab_id, offset }
                    if tab_id == key.0 && offset == cursor =>
                {
                    let scrolled = self.scroll_caret_to_top_now();
                    self.caret_followed = Some(key);
                    return scrolled;
                }
                PendingScroll::SelectionIntoView { tab_id, offset }
                    if tab_id == key.0 && offset == key.2.start =>
                {
                    let scrolled = self.scroll_selection_into_view_now();
                    self.caret_followed = Some(key);
                    return scrolled;
                }
                _ => {}
            }
        }
        if self.caret_followed.as_ref() == Some(&key) {
            return false;
        }
        let parsed = self.wysiwyg_parsed();
        let mut paint = self.editor_paint.borrow_mut();
        if paint.painted_len != key.3
            || paint.painted_selection != key.2
            || paint.painted_view_mode != key.1
        {
            return false;
        }
        let row_y = paint.row_y_for_offset(cursor, parsed.as_deref());
        let view = self.editor_scroll.bounds();
        let mut scrolled = false;
        if let Some((row_top, row_height)) = row_y {
            if let Some(delta) = follow_caret_delta(
                row_top,
                row_height,
                paint.line_height,
                view.top(),
                view.size.height,
            ) {
                let offset = self.editor_scroll.offset();
                let max = self.editor_scroll.max_offset().height;
                let y = (offset.y - delta).clamp(-max, px(0.0));
                if (y - offset.y).abs() > px(0.01) {
                    self.editor_scroll.set_offset(point(offset.x, y));
                    scrolled = true;
                }
            }
        }
        drop(paint);
        self.caret_followed = Some(key);
        scrolled
    }

    fn scroll_caret_to_top_now(&mut self) -> bool {
        let Some(cursor) = self.active_tab().map(|tab| tab.cursor_offset()) else {
            return false;
        };
        let parsed = self.wysiwyg_parsed();
        let mut paint = self.editor_paint.borrow_mut();
        let Some((row_top, row_height)) = paint.row_y_for_offset(cursor, parsed.as_deref()) else {
            return false;
        };
        let view = self.editor_scroll.bounds();
        if view.size.height <= px(0.0) {
            return false;
        }
        let line_h = if paint.line_height > px(0.0) {
            paint.line_height
        } else {
            row_height
        };
        let delta = row_top - (view.top() + line_h * 1.5);
        let offset = self.editor_scroll.offset();
        let max = self.editor_scroll.max_offset().height;
        let y = (offset.y - delta).clamp(-max, px(0.0));
        if (y - offset.y).abs() > px(0.5) {
            self.editor_scroll.set_offset(point(offset.x, y));
            true
        } else {
            false
        }
    }

    /// Scrolls so the row holding the caret sits near the top of the view
    /// (as far as the end of the document allows).
    pub(super) fn scroll_caret_to_top(&mut self) {
        self.scroll_caret_to_top_now();
        self.pending_scroll = self.active_tab().map(|tab| PendingScroll::CaretToTop {
            tab_id: tab.id.clone(),
            offset: tab.cursor_offset(),
        });
    }

    fn scroll_selection_into_view_now(&mut self) -> bool {
        let Some(start) = self.active_tab().map(|tab| tab.selection_range().start) else {
            return false;
        };
        let parsed = self.wysiwyg_parsed();
        let mut paint = self.editor_paint.borrow_mut();
        let Some((row_top, row_height)) = paint.row_y_for_offset(start, parsed.as_deref()) else {
            return false;
        };
        let view = self.editor_scroll.bounds();
        let Some(delta) = scroll_delta_for_row(row_top, row_height, view.top(), view.size.height)
        else {
            return false;
        };
        let offset = self.editor_scroll.offset();
        let max = self.editor_scroll.max_offset().height;
        let y = (offset.y - delta).clamp(-max, px(0.0));
        if (y - offset.y).abs() > px(0.5) {
            self.editor_scroll.set_offset(point(offset.x, y));
            true
        } else {
            false
        }
    }

    /// Scrolls the editor so the row holding the selection start is visible.
    /// Uses the geometry from the last paint, which is still valid after a
    /// find step because the content did not change.
    pub(super) fn scroll_selection_into_view(&mut self) {
        self.scroll_selection_into_view_now();
        self.pending_scroll = self
            .active_tab()
            .map(|tab| PendingScroll::SelectionIntoView {
                tab_id: tab.id.clone(),
                offset: tab.selection_range().start,
            });
    }
}

/// Minimal vertical scroll delta needed to keep a caret row in view.
/// Uses the single-line `line_height` as the edge margin rather than the row's
/// full height, so tall blocks (pictures, multi-line table rows) never invert
/// the margin band and scroll off-screen.
pub(crate) fn follow_caret_delta(
    row_top: Pixels,
    row_height: Pixels,
    line_height: Pixels,
    view_top: Pixels,
    view_height: Pixels,
) -> Option<Pixels> {
    if view_height <= px(0.0) {
        return None;
    }
    let base_h = if line_height > px(0.0) {
        line_height
    } else {
        row_height
    };
    let margin = base_h.min(view_height * 0.25);
    let view_bottom = view_top + view_height;
    let top = view_top + margin;
    let bottom = view_bottom - margin;
    let row_bottom = row_top + row_height;

    if row_height > bottom - top {
        // Tall row: doesn't fit inside the padded band.
        if row_height <= view_height {
            if row_top >= view_top && row_bottom <= view_bottom {
                None
            } else {
                let pad = ((view_height - row_height) * 0.5).min(margin);
                Some(row_top - (view_top + pad))
            }
        } else if row_top <= top && row_bottom >= bottom {
            // Already covering the viewport.
            None
        } else {
            Some(row_top - top)
        }
    } else if row_bottom > bottom {
        Some(row_bottom - bottom)
    } else if row_top < top {
        Some(row_top - top)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::follow_caret_delta;
    use gpui::px;

    #[test]
    fn normal_row_uses_single_line_margin() {
        // Inside margins: no scroll.
        assert_eq!(
            follow_caret_delta(px(100.0), px(24.0), px(24.0), px(0.0), px(600.0)),
            None
        );
        // Below bottom margin (600 - 24 = 576): scrolls just enough to clear margin.
        assert_eq!(
            follow_caret_delta(px(560.0), px(24.0), px(24.0), px(0.0), px(600.0)),
            Some(px(8.0))
        );
        // Above top margin (24): scrolls up to top margin.
        assert_eq!(
            follow_caret_delta(px(10.0), px(24.0), px(24.0), px(0.0), px(600.0)),
            Some(px(-14.0))
        );
    }

    #[test]
    fn tall_picture_in_view_does_not_scroll_off_screen() {
        // 492px tall picture in a 600px viewport: row_height > view_height / 2.
        // Previously margin=492 inverted (top, bottom) and violently scrolled off top.
        assert_eq!(
            follow_caret_delta(px(40.0), px(492.0), px(24.0), px(0.0), px(600.0)),
            None
        );
        assert_eq!(
            follow_caret_delta(px(5.0), px(492.0), px(24.0), px(0.0), px(500.0)),
            None
        );
    }

    #[test]
    fn tall_picture_partially_offscreen_scrolls_cleanly_and_is_idempotent() {
        let delta = follow_caret_delta(px(450.0), px(492.0), px(24.0), px(0.0), px(600.0))
            .expect("clipped tall picture should scroll into view");
        let new_top = px(450.0) - delta;
        assert!(new_top >= px(0.0));
        assert!(new_top + px(492.0) <= px(600.0));
        assert_eq!(
            follow_caret_delta(new_top, px(492.0), px(24.0), px(0.0), px(600.0)),
            None,
            "second pass must be stable"
        );
    }

    #[test]
    fn picture_taller_than_viewport_stays_put_when_covering_and_aligns_when_offscreen() {
        // 492px picture in a 400px viewport, already covering viewport.
        assert_eq!(
            follow_caret_delta(px(10.0), px(492.0), px(24.0), px(0.0), px(400.0)),
            None
        );
        // Same picture scrolled way below the viewport.
        let delta = follow_caret_delta(px(500.0), px(492.0), px(24.0), px(0.0), px(400.0))
            .expect("off-screen giant picture should scroll into view");
        let new_top = px(500.0) - delta;
        assert_eq!(new_top, px(24.0));
        assert_eq!(
            follow_caret_delta(new_top, px(492.0), px(24.0), px(0.0), px(400.0)),
            None,
            "aligned giant picture must not oscillate"
        );
    }
}
