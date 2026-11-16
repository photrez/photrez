//! The Layers rows' context menu: open/close plus the target bookkeeping the
//! menu needs. The items themselves are rendered by the view side from live
//! state at render time, so a menu open across a selection change still reads
//! the document as it is.

use gpui_kit::Context;

use super::App;

impl App {
    /// Anchor the menu to the window position and the clicked layer's core
    /// id. A click on an unselected row selects it first; a click on a row
    /// that is already part of a multi selection keeps the selection.
    pub(crate) fn open_layer_context_menu(
        &mut self,
        core_id: &str,
        at: (f32, f32),
        cx: &mut Context<Self>,
    ) {
        self.state.update(cx, |state, _| {
            let row_id = state
                .layers
                .iter()
                .find(|row| row.core_id.as_deref() == Some(core_id))
                .map(|row| row.id);
            let Some(row_id) = row_id else {
                return;
            };
            if !state.selected_layers.contains(&row_id) {
                state.selected_layers = vec![row_id];
                state.selected_layer = row_id;
            }
            state.layer_context_menu = Some((at.0, at.1, core_id.to_string()));
        });
        cx.notify();
    }

    pub(crate) fn close_layer_context_menu(&mut self, cx: &mut Context<Self>) {
        self.state
            .update(cx, |state, _| state.layer_context_menu = None);
        cx.notify();
    }
}
