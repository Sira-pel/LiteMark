//! Scrolling: momentum, caret follow, selection reveal.

use super::*;
use crate::ui::text_editor::row_at_or_before;

impl LiteMarkApp {
    /// Feeds touchpad movement to the momentum tracker; wheel notches reset it.
    pub(super) fn note_scroll(
        &mut self,
        ev: &ScrollWheelEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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

    /// Keeps the caret on screen after typing, Enter, mode switches, or caret moves.
    /// Waits for a paint of the current text and view mode before measuring.
    /// `Some(true)` means that paint has not happened yet and another frame should be asked for.
    pub(super) fn follow_caret(&mut self) -> Option<bool> {
        let view_mode = self.effective_view_mode();
        let tab = self.active_tab()?;
        let key = (
            tab.id.clone(),
            view_mode,
            tab.selection_range(),
            tab.content.len(),
        );
        if self.caret_followed.as_ref() == Some(&key) {
            return None;
        }
        let cursor = tab.caret.cursor();
        let paint = self.editor_paint.borrow();
        if (paint.painted_len != key.3
            || paint.painted_selection != key.2
            || paint.painted_view_mode != key.1)
            && self.caret_follow_waits < 3
        {
            self.caret_follow_waits += 1;
            return Some(true); // geometry is from an older frame; look again
        }
        self.caret_follow_waits = 0;
        let row = row_at_or_before(&paint.rows, cursor);
        let view = self.editor_scroll.bounds();
        if let Some(row) = row {
            if let Some(delta) = follow_caret_delta(
                row.origin.y,
                row.line_height,
                paint.line_height,
                view.top(),
                view.size.height,
            ) {
                let offset = self.editor_scroll.offset();
                let max = self.editor_scroll.max_offset().height;
                let y = (offset.y - delta).clamp(-max, px(0.0));
                self.editor_scroll.set_offset(point(offset.x, y));
            }
        }
        drop(paint);
        self.caret_followed = Some(key);
        Some(false)
    }

    /// Scrolls so the row holding the caret sits near the top of the view
    /// (as far as the end of the document allows).
    pub(super) fn scroll_caret_to_top(&mut self) {
        let Some(cursor) = self.active_tab().map(|tab| tab.cursor_offset()) else {
            return;
        };
        let paint = self.editor_paint.borrow();
        let Some(row) = row_at_or_before(&paint.rows, cursor) else {
            return;
        };
        let view = self.editor_scroll.bounds();
        if view.size.height <= px(0.0) {
            return;
        }
        let line_h = if paint.line_height > px(0.0) {
            paint.line_height
        } else {
            row.line_height
        };
        let delta = row.origin.y - (view.top() + line_h * 1.5);
        let offset = self.editor_scroll.offset();
        let max = self.editor_scroll.max_offset().height;
        let y = (offset.y - delta).clamp(-max, px(0.0));
        self.editor_scroll.set_offset(point(offset.x, y));
    }

    /// Scrolls the editor so the row holding the selection start is visible.
    /// Uses the geometry from the last paint, which is still valid after a
    /// find step because the content did not change.
    pub(super) fn scroll_selection_into_view(&mut self) {
        let Some(start) = self.active_tab().map(|tab| tab.selection_range().start) else {
            return;
        };
        let paint = self.editor_paint.borrow();
        let Some(row) = row_at_or_before(&paint.rows, start) else {
            return;
        };
        let view = self.editor_scroll.bounds();
        let Some(delta) =
            scroll_delta_for_row(row.origin.y, row.line_height, view.top(), view.size.height)
        else {
            return;
        };
        let offset = self.editor_scroll.offset();
        let max = self.editor_scroll.max_offset().height;
        let y = (offset.y - delta).clamp(-max, px(0.0));
        self.editor_scroll.set_offset(point(offset.x, y));
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
