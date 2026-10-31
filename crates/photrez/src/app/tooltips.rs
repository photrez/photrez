//! Hover tooltips: hover enters publish their control's id; leaving clears it,
//! so only one tooltip is ever visible and it disappears with the pointer.

use gpui_kit::Context;

use super::App;

impl App {
    pub(crate) fn set_tooltip(&mut self, id: Option<String>, cx: &mut Context<Self>) {
        let changed = self.state.read(cx).tooltip != id;
        if changed {
            self.state.update(cx, |state, _| state.tooltip = id);
            cx.notify();
        }
    }
}
