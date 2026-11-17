//! Tab hover-to-switch while a layer drag is in flight: hovering a document
//! tab for [`TAB_HOVER_SWITCH`] brings that document to the front, so a
//! drag can end on its canvas rather than on the tab itself. A timer arms
//! per hover, never for the active tab, and cancels on leave, on drop, or
//! on Escape.

use std::time::Duration;

use gpui_kit::Context;

use super::App;

/// The 500 ms hover that switches documents mid-drag.
pub(crate) const TAB_HOVER_SWITCH: Duration = Duration::from_millis(500);

impl App {
    /// The tab strip calls this on hover enter and leave. While a layer
    /// drag is active and the hovered tab is not the active document, the
    /// timer arms; leaving the tab, any drop or Escape cancels it.
    pub(crate) fn tab_hover_drag(&mut self, hovered: &bool, tab_id: &str, cx: &mut Context<Self>) {
        if !*hovered {
            let leaving = self.state.read(cx).layer_drag_hover_tab.as_deref() == Some(tab_id);
            if leaving {
                self.state
                    .update(cx, |state, _| state.layer_drag_hover_tab = None);
            }
            if self.tab_hover_doc.as_deref() == Some(tab_id) {
                self.cancel_tab_hover_switch(cx);
            }
            cx.notify();
            return;
        }
        let drag_active = self.state.read(cx).layer_drag_active;
        if !drag_active {
            return;
        }
        if self.state.read(cx).layer_drag_hover_tab.as_deref() != Some(tab_id) {
            self.state.update(cx, |state, _| {
                state.layer_drag_hover_tab = Some(tab_id.to_string());
            });
        }
        let active = self
            .active_document
            .as_ref()
            .is_some_and(|document| document.doc_id == tab_id);
        if active {
            self.cancel_tab_hover_switch(cx);
            return;
        }
        if self.tab_hover_doc.as_deref() == Some(tab_id) {
            // The timer is already armed for this tab; keep it running.
            return;
        }
        self.cancel_tab_hover_switch(cx);
        let tab_id_owned = tab_id.to_string();
        self.tab_hover_doc = Some(tab_id_owned.clone());
        self.tab_hover_task = Some(cx.spawn(async move |weak, cx| {
            cx.background_executor().timer(TAB_HOVER_SWITCH).await;
            let _ = weak.update(cx, |app, cx| {
                app.finish_tab_hover_switch(&tab_id_owned, cx);
            });
        }));
        cx.notify();
    }

    /// Drop the hover timer, if any. Idempotent.
    pub(crate) fn cancel_tab_hover_switch(&mut self, cx: &mut Context<Self>) {
        if self.tab_hover_task.take().is_some() || self.tab_hover_doc.take().is_some() {
            cx.notify();
        }
    }

    /// Drop the drag flag and any armed timer. Called from the paths a
    /// layer drag can end on: any cross-document drop and Escape.
    pub(crate) fn clear_layer_drag_and_hover(&mut self, cx: &mut Context<Self>) {
        self.cancel_tab_hover_switch(cx);
        self.state.update(cx, |state, _| {
            state.layer_drag_active = false;
            state.layer_drag_source = None;
            state.layer_drag_hover_tab = None;
        });
        cx.notify();
    }

    /// The 500 ms elapsed while the pointer stayed on the tab: switch to
    /// the tab's document and re-arm nothing.
    pub(crate) fn finish_tab_hover_switch(&mut self, tab_id: &str, cx: &mut Context<Self>) {
        self.tab_hover_task = None;
        self.tab_hover_doc = None;
        let _ = self.activate_document(tab_id, cx);
        cx.notify();
    }
}
