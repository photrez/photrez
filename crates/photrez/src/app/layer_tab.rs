//! The Layers and History tab action and the cycle handler. Both tabs live
//! in the same key context; ArrowLeft and ArrowRight flip between them.

use gpui_kit::{Action, Context};

use crate::state::LayerTab;

use super::App;

/// Switch the Layers dock to the other tab, whichever is active.
#[derive(Clone, PartialEq, Debug, Default, Action)]
#[action(namespace = photrez, no_json)]
pub(crate) struct LayerDockTabSwitch;

impl App {
    pub(crate) fn layer_dock_tab_switch_action(
        &mut self,
        _: &LayerDockTabSwitch,
        cx: &mut Context<Self>,
    ) {
        self.state.update(cx, |state, _| {
            state.layer_tab = match state.layer_tab {
                LayerTab::Layers => LayerTab::History,
                LayerTab::History => LayerTab::Layers,
            };
        });
        cx.notify();
    }
}
