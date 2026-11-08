//! Hover tooltips: hover enters publish their control's id; leaving clears it,
//! so only one tooltip is ever visible and it disappears with the pointer.
//!
//! A tooltip does not appear with the pointer: the hover's start time is
//! recorded on the executor's clock and the overlay paints the tooltip only
//! once [`TOOLTIP_DELAY`] has elapsed, the way the recorded editor's 400ms
//! hover timer worked. Because the shell repaints on demand, the elapsed
//! delay alone would not repaint an idle window, so a one-shot timer notifies
//! the app when the delay is up; the overlay still answers to the clock, so
//! an earlier repaint cannot show it too soon.

use std::time::Duration;

use gpui_kit::Context;

use super::App;

/// How long a control must stay hovered before its tooltip appears.
pub(crate) const TOOLTIP_DELAY: Duration = Duration::from_millis(400);

impl App {
    pub(crate) fn set_tooltip(&mut self, id: Option<String>, cx: &mut Context<Self>) {
        let changed = {
            let state = self.state.read(cx);
            state.tooltip != id
        };
        if !changed {
            return;
        }
        let now = cx.background_executor().now();
        self.state.update(cx, |state, _| {
            state.tooltip = id.clone();
            state.tooltip_since = id.as_ref().map(|_| now);
        });
        cx.notify();
        if id.is_some() {
            cx.spawn(async move |weak, cx| {
                cx.background_executor().timer(TOOLTIP_DELAY).await;
                let _ = weak.update(cx, |_, cx| cx.notify());
            })
            .detach();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::TOOLTIP_DELAY;

    /// RED proof: change the delay to zero and this fails at the assert, which
    /// is the instant-show behaviour the overlay no longer has.
    #[test]
    fn tooltip_delay_matches_the_recorded_hover_timer() {
        assert_eq!(TOOLTIP_DELAY.as_millis(), 400);
    }
}
