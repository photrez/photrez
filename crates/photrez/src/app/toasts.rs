//! The live toast queue: a module-level queue the views read at render time.
//!
//! `show_toast` appends `{ id, message, severity }`, evicts the oldest entry
//! once the stack exceeds three, and arms a timer that dismisses that exact
//! id. The queue is projected into `AppState` so the renderer stays a pure
//! function of state; this module owns only the queue arithmetic and the
//! timers.
//!
//! Dismiss timers are fixed per level: 3500 ms for info and warn, 5000 ms
//! for error.

use std::time::Duration;

use gpui_kit::Context;

use crate::state::{Toast, ToastSeverity};

use super::App;

/// The dismissal delay for the info and warn levels.
const SHORT_DISMISS: Duration = Duration::from_millis(3_500);

/// The dismissal delay for the error level.
const ERROR_DISMISS: Duration = Duration::from_millis(5_000);

/// Three rows is the visible cap, so a fourth push drops the oldest.
const MAX_TOASTS: usize = 3;

impl App {
    /// Push a toast and arm its dismissal timer.
    pub(crate) fn show_toast(
        &mut self,
        message: impl Into<String>,
        severity: ToastSeverity,
        cx: &mut Context<Self>,
    ) {
        let id = self.state.read(cx).next_toast_id;
        let message = message.into();
        self.state.update(cx, |state, _| {
            state.next_toast_id = state.next_toast_id.saturating_add(1);
            state.toasts.push(Toast {
                id,
                message,
                severity,
            });
            let overflow = state.toasts.len().saturating_sub(MAX_TOASTS);
            state.toasts.drain(..overflow);
        });
        cx.notify();
        let duration = match severity {
            ToastSeverity::Error => ERROR_DISMISS,
            ToastSeverity::Info | ToastSeverity::Warn => SHORT_DISMISS,
        };
        cx.spawn(async move |weak, cx| {
            cx.background_executor().timer(duration).await;
            let _ = weak.update(cx, |app, cx| app.dismiss_toast(id, cx));
        })
        .detach();
    }

    /// Drop the toast with `id`; a no-op when it was already evicted.
    pub(crate) fn dismiss_toast(&mut self, id: u64, cx: &mut Context<Self>) {
        let changed = {
            let state = self.state.read(cx);
            state.toasts.iter().any(|toast| toast.id == id)
        };
        if changed {
            self.state
                .update(cx, |state, _| state.toasts.retain(|toast| toast.id != id));
            cx.notify();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ERROR_DISMISS, MAX_TOASTS, SHORT_DISMISS};

    #[test]
    fn durations_match_the_source() {
        assert_eq!(SHORT_DISMISS.as_millis(), 3_500);
        assert_eq!(ERROR_DISMISS.as_millis(), 5_000);
        assert_eq!(MAX_TOASTS, 3);
    }
}
