//! Scrolling: momentum, caret follow, selection reveal.

use super::*;

impl LiteMarkApp {
    /// Feeds touchpad movement to the momentum tracker; wheel notches reset it.
    pub(super) fn note_scroll(&mut self, ev: &ScrollWheelEvent, _window: &mut Window, cx: &mut Context<Self>) {
        // macOS already sends its own inertial events after the lift, and
        // Windows only reports line steps, so this is for Linux touchpads.
        match ev.delta {
            _ if !cfg!(target_os = "linux") => {}
            ScrollDelta::Pixels(d) => {
                self.scroll_momentum
                    .on_event(std::time::Instant::now(), f32::from(d.y));
                cx.notify(); // render asks for the animation frames
            }
            ScrollDelta::Lines(_) => self.scroll_momentum.cancel(),
        }
    }

    /// Touchpad momentum: one frame of the coast after the fingers lift.
    pub(super) fn coast_scroll(&mut self, window: &mut Window) {
        if self.scroll_momentum.is_idle() {
            return;
        }
        let step = self.scroll_momentum.tick(std::time::Instant::now());
        if let Some(step) = step {
            let offset = self.editor_scroll.offset();
            let max = self.editor_scroll.max_offset().height;
            let y = (offset.y + px(step)).clamp(-max, px(0.0));
            if y == offset.y {
                self.scroll_momentum.cancel(); // hit the top or bottom
            } else {
                self.editor_scroll.set_offset(point(offset.x, y));
            }
        }
        if !self.scroll_momentum.is_idle() {
            window.request_animation_frame();
        }
    }

    /// Keeps the caret on screen after typing, Enter or caret moves. Waits
    /// for a paint of the current text before measuring. `Some(true)` means
    /// that paint has not happened yet and another frame should be asked for.
    pub(super) fn follow_caret(&mut self) -> Option<bool> {
        let tab = self.active_tab()?;
        let key = (
            tab.id.clone(),
            tab.selection_range(),
            tab.content.len(),
        );
        if self.caret_followed.as_ref() == Some(&key) {
            return None;
        }
        let cursor = tab.caret.cursor();
        let paint = self.editor_paint.borrow();
        if (paint.painted_len != key.2 || paint.painted_selection != key.1)
            && self.caret_follow_waits < 3
        {
            self.caret_follow_waits += 1;
            return Some(true); // geometry is from an older frame; look again
        }
        self.caret_follow_waits = 0;
        let row = paint
            .rows
            .iter()
            .rev()
            .find(|row| row.byte_start <= cursor)
            .or_else(|| paint.rows.first());
        let view = self.editor_scroll.bounds();
        if let Some(row) = row.filter(|_| view.size.height > px(0.0)) {
            // Minimal scroll, keeping one line of context past the edge.
            let margin = row.line_height;
            let (top, bottom) = (view.top() + margin, view.bottom() - margin);
            let delta = if row.origin.y + row.line_height > bottom {
                Some(row.origin.y + row.line_height - bottom)
            } else if row.origin.y < top {
                Some(row.origin.y - top)
            } else {
                None
            };
            if let Some(delta) = delta {
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

    /// Scrolls the editor so the row holding the selection start is visible.
    /// Uses the geometry from the last paint, which is still valid after a
    /// find step because the content did not change.
    pub(super) fn scroll_selection_into_view(&mut self) {
        let Some(start) = self.active_tab().map(|tab| tab.selection_range().start) else {
            return;
        };
        let paint = self.editor_paint.borrow();
        let Some(row) = paint
            .rows
            .iter()
            .rev()
            .find(|row| row.byte_start <= start)
            .or_else(|| paint.rows.first())
        else {
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
