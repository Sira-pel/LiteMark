use crate::theme::{sz, Theme};
use gpui::prelude::*;
use gpui::*;
use std::time::{Duration, Instant};

/// Touchpad coasting. Wayland and X11 send a stream of pixel deltas with no
/// "fingers lifted" event, so a short silence after fast movement is taken as
/// the lift and the last speed decays smoothly instead of stopping dead.
#[derive(Default)]
pub struct ScrollMomentum {
    velocity: f32,
    last_event: Option<Instant>,
    last_tick: Option<Instant>,
    coasting: bool,
}

impl ScrollMomentum {
    /// Silence that counts as the fingers lifting.
    const LIFT_GAP: Duration = Duration::from_millis(40);
    /// Longer than this between events means the fingers rested first.
    const REST_GAP: Duration = Duration::from_millis(120);
    const MIN_START: f32 = 200.0;
    const MIN_KEEP: f32 = 25.0;
    /// Time constant of the speed decay, in seconds.
    const DECAY: f32 = 0.35;

    /// A touchpad scroll event of `dy` pixels arrived (stops any coasting).
    pub fn on_event(&mut self, now: Instant, dy: f32) {
        let instant_speed = match self.last_event {
            Some(prev) if !self.coasting => {
                let dt = now.duration_since(prev);
                if dt.is_zero() || dt > Self::REST_GAP {
                    0.0
                } else {
                    dy / dt.as_secs_f32()
                }
            }
            _ => 0.0,
        };
        self.velocity = if instant_speed == 0.0 {
            0.0
        } else {
            0.6 * instant_speed + 0.4 * self.velocity
        };
        self.coasting = false;
        self.last_event = Some(now);
        self.last_tick = None;
    }

    /// A line-based wheel or any other input: forget everything.
    pub fn cancel(&mut self) {
        *self = Self::default();
    }

    pub fn is_idle(&self) -> bool {
        self.last_event.is_none()
    }

    /// Advance to `now`. Returns the pixels to scroll this frame, if any.
    /// `at_edge` ends the coast when the scroll hit its limit.
    pub fn tick(&mut self, now: Instant) -> Option<f32> {
        let last = self.last_event?;
        if !self.coasting {
            let since = now.duration_since(last);
            if since < Self::LIFT_GAP {
                return None;
            }
            if self.velocity.abs() < Self::MIN_START || since > Self::REST_GAP * 2 {
                self.cancel();
                return None;
            }
            self.coasting = true;
            self.last_tick = Some(last + Self::LIFT_GAP);
        }
        let dt = now
            .duration_since(self.last_tick.unwrap_or(now))
            .min(Duration::from_millis(50))
            .as_secs_f32();
        self.last_tick = Some(now);
        let step = self.velocity * dt;
        self.velocity *= (-dt / Self::DECAY).exp();
        if self.velocity.abs() < Self::MIN_KEEP {
            self.cancel();
        }
        Some(step)
    }
}

/// Handler for scroll-wheel and touchpad events over the editor.
pub type ScrollHandler = Box<dyn Fn(&ScrollWheelEvent, &mut Window, &mut App)>;

pub struct EditorProps {
    pub zoom_level: usize,
    pub show_zoom_hud: bool,
    /// Unified interactive editor surface (handles both WYSIWYG and Raw modes).
    pub editor_element: AnyElement,
    pub scroll: ScrollHandle,
    /// Called for every scroll-wheel / touchpad event over the editor.
    pub on_scroll: Option<ScrollHandler>,
}

pub fn render_editor(props: EditorProps, theme: &Theme) -> impl IntoElement {
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
                    .rounded(sz::R_MD)
                    .bg(theme.menu_bg)
                    .border_1()
                    .border_color(theme.border_surface)
                    .shadow_lg()
                    .text_size(px(11.0))
                    .font_family(Theme::FONT_MONO)
                    .font_weight(FontWeight::BOLD)
                    .text_color(theme.text_primary)
                    .child(format!("Zoom {}%", props.zoom_level)),
            )
        })
        .child(
            div()
                .id("editor-scroll")
                .track_scroll(&props.scroll)
                .w_full()
                .min_w_0()
                .flex_1()
                .size_full()
                .overflow_x_hidden()
                .overflow_y_scroll()
                .when_some(props.on_scroll, |s, f| s.on_scroll_wheel(f))
                .child(div().w_full().min_w_0().child(props.editor_element)),
        )
}

/// Vertical scroll change needed to bring a row at `row_top` fully into the
/// viewport. `None` when it is already visible. A row above or below the
/// viewport lands a third of the way down so context stays visible.
pub fn scroll_delta_for_row(
    row_top: Pixels,
    row_height: Pixels,
    view_top: Pixels,
    view_height: Pixels,
) -> Option<Pixels> {
    if row_top >= view_top && row_top + row_height <= view_top + view_height {
        return None;
    }
    Some(row_top - (view_top + view_height / 3.0))
}

#[cfg(test)]
mod tests {
    use super::scroll_delta_for_row;
    use gpui::px;

    #[test]
    fn a_fast_flick_coasts_and_then_stops() {
        use super::ScrollMomentum;
        use std::time::{Duration, Instant};
        let t0 = Instant::now();
        let ms = |n| t0 + Duration::from_millis(n);
        let mut m = ScrollMomentum::default();
        for i in 0..10u64 {
            m.on_event(ms(i * 8), -12.0); // ~1500 px/s
        }
        assert_eq!(m.tick(ms(72 + 10)), None, "still within the lift gap");
        let first = m.tick(ms(72 + 50)).expect("coasting starts");
        assert!(first < 0.0, "keeps the flick direction");
        let mut total = first;
        let mut t = 72 + 50;
        let mut frames = 0;
        while let Some(step) = m.tick(ms(t + 16)) {
            assert!(step <= 0.0);
            total += step;
            t += 16;
            frames += 1;
            assert!(frames < 400, "coasting never ends");
        }
        assert!(total < -200.0, "travels a meaningful distance: {total}");
        assert!(m.is_idle());
    }

    #[test]
    fn slow_scroll_or_a_rest_before_lifting_does_not_coast() {
        use super::ScrollMomentum;
        use std::time::{Duration, Instant};
        let t0 = Instant::now();
        let mut m = ScrollMomentum::default();
        for i in 0..10u64 {
            m.on_event(t0 + Duration::from_millis(i * 16), -1.0); // ~60 px/s
        }
        assert_eq!(m.tick(t0 + Duration::from_millis(300)), None);
        assert!(m.is_idle());

        let mut m = ScrollMomentum::default();
        m.on_event(t0, -30.0);
        m.on_event(t0 + Duration::from_millis(8), -30.0);
        m.on_event(t0 + Duration::from_millis(400), -1.0); // rested, then a nudge
        assert_eq!(m.tick(t0 + Duration::from_millis(480)), None);
    }

    #[test]
    fn a_new_event_stops_the_coast() {
        use super::ScrollMomentum;
        use std::time::{Duration, Instant};
        let t0 = Instant::now();
        let mut m = ScrollMomentum::default();
        for i in 0..10u64 {
            m.on_event(t0 + Duration::from_millis(i * 8), -12.0);
        }
        assert!(m.tick(t0 + Duration::from_millis(130)).is_some());
        m.on_event(t0 + Duration::from_millis(140), 3.0);
        assert_eq!(m.tick(t0 + Duration::from_millis(150)), None);
    }

    #[test]
    fn visible_row_does_not_scroll() {
        assert_eq!(scroll_delta_for_row(px(100.0), px(20.0), px(0.0), px(600.0)), None);
    }

    #[test]
    fn row_below_viewport_scrolls_to_a_third_down() {
        let d = scroll_delta_for_row(px(1500.0), px(20.0), px(50.0), px(600.0)).unwrap();
        assert_eq!(d, px(1500.0 - 250.0));
    }

    #[test]
    fn row_above_viewport_scrolls_up() {
        let d = scroll_delta_for_row(px(-400.0), px(20.0), px(50.0), px(600.0)).unwrap();
        assert!(d < px(0.0));
    }

    #[test]
    fn row_clipped_at_the_bottom_edge_scrolls() {
        assert!(scroll_delta_for_row(px(590.0), px(28.0), px(0.0), px(600.0)).is_some());
    }
}
