//! The live loading state behind document opening.

use gpui_kit::Context;

use super::App;
use crate::state::LoadingState;

impl App {
    /// Start a loading overlay and return the generation that owns it.
    pub(crate) fn begin_loading(&mut self, cx: &mut Context<Self>) -> u64 {
        self.loading_generation = self.loading_generation.wrapping_add(1);
        self.state.update(cx, |state, _| {
            state.loading = Some(LoadingState::Indeterminate);
        });
        cx.notify();
        self.loading_generation
    }

    pub(crate) fn set_loading_progress(&mut self, fraction: f32, cx: &mut Context<Self>) {
        if !fraction.is_finite() {
            return;
        }
        let fraction = fraction.clamp(0.0, 1.0);
        self.state.update(cx, |state, _| {
            if state.loading.is_some() {
                state.loading = Some(LoadingState::Progress(fraction));
            }
        });
        cx.notify();
    }

    pub(crate) fn end_loading(&mut self, cx: &mut Context<Self>) {
        self.loading_generation = self.loading_generation.wrapping_add(1);
        self.state.update(cx, |state, _| state.loading = None);
        cx.notify();
    }
}
