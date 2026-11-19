//! Keyboard navigation for the Layers panel rows: arrow keys move the row
//! focus, Enter toggles visibility and Space toggles the lock of the active
//! row's target, Ctrl+A selects every row, Home/End jump the stack.
//!
//! These live in the `LayersPanel` key context so they only fire while a
//! Layers panel element holds focus (the row body or a tab button). The
//! Layers/History tabs carry the same context — the P4 tabs stay unchanged.

use gpui_kit::{Action, Context};

use crate::state::LayerTab;

use super::App;

#[derive(Clone, PartialEq, Debug, Default, Action)]
#[action(namespace = photrez, no_json)]
pub(crate) struct LayersPanelMoveUp;

#[derive(Clone, PartialEq, Debug, Default, Action)]
#[action(namespace = photrez, no_json)]
pub(crate) struct LayersPanelMoveDown;

#[derive(Clone, PartialEq, Debug, Default, Action)]
#[action(namespace = photrez, no_json)]
pub(crate) struct LayersPanelExtendUp;

#[derive(Clone, PartialEq, Debug, Default, Action)]
#[action(namespace = photrez, no_json)]
pub(crate) struct LayersPanelExtendDown;

#[derive(Clone, PartialEq, Debug, Default, Action)]
#[action(namespace = photrez, no_json)]
pub(crate) struct LayersPanelToggleVisibility;

#[derive(Clone, PartialEq, Debug, Default, Action)]
#[action(namespace = photrez, no_json)]
pub(crate) struct LayersPanelToggleLock;

#[derive(Clone, PartialEq, Debug, Default, Action)]
#[action(namespace = photrez, no_json)]
pub(crate) struct LayersPanelSelectAll;

#[derive(Clone, PartialEq, Debug, Default, Action)]
#[action(namespace = photrez, no_json)]
pub(crate) struct LayersPanelFirst;

#[derive(Clone, PartialEq, Debug, Default, Action)]
#[action(namespace = photrez, no_json)]
pub(crate) struct LayersPanelLast;

impl App {
    pub(crate) fn layers_panel_move_up_action(
        &mut self,
        _: &LayersPanelMoveUp,
        cx: &mut Context<Self>,
    ) {
        self.move_row_focus(-1, false, cx);
    }

    pub(crate) fn layers_panel_move_down_action(
        &mut self,
        _: &LayersPanelMoveDown,
        cx: &mut Context<Self>,
    ) {
        self.move_row_focus(1, false, cx);
    }

    pub(crate) fn layers_panel_extend_up_action(
        &mut self,
        _: &LayersPanelExtendUp,
        cx: &mut Context<Self>,
    ) {
        self.move_row_focus(-1, true, cx);
    }

    pub(crate) fn layers_panel_extend_down_action(
        &mut self,
        _: &LayersPanelExtendDown,
        cx: &mut Context<Self>,
    ) {
        self.move_row_focus(1, true, cx);
    }

    pub(crate) fn layers_panel_toggle_visibility_action(
        &mut self,
        _: &LayersPanelToggleVisibility,
        cx: &mut Context<Self>,
    ) {
        self.toggle_active_row_visibility(cx);
    }

    pub(crate) fn layers_panel_toggle_lock_action(
        &mut self,
        _: &LayersPanelToggleLock,
        cx: &mut Context<Self>,
    ) {
        self.toggle_active_row_lock(cx);
    }

    pub(crate) fn layers_panel_select_all_action(
        &mut self,
        _: &LayersPanelSelectAll,
        cx: &mut Context<Self>,
    ) {
        if self.state.read(cx).layer_tab != LayerTab::Layers {
            return;
        }
        self.select_all_layers(cx);
    }

    pub(crate) fn layers_panel_first_action(
        &mut self,
        _: &LayersPanelFirst,
        cx: &mut Context<Self>,
    ) {
        self.move_row_focus(i64::MIN as isize, false, cx);
    }

    pub(crate) fn layers_panel_last_action(&mut self, _: &LayersPanelLast, cx: &mut Context<Self>) {
        self.move_row_focus(i64::MAX as isize, false, cx);
    }

    fn move_row_focus(&mut self, delta: isize, extend: bool, cx: &mut Context<Self>) {
        if self.state.read(cx).layer_tab != LayerTab::Layers {
            return;
        }
        let rows: Vec<u64> = self
            .state
            .read(cx)
            .layers
            .iter()
            .map(|row| row.id)
            .collect();
        if rows.is_empty() {
            return;
        }
        let current = {
            let state = self.state.read(cx);
            row_current(&rows, state.selected_layer, &state.selected_layers)
        };
        let abs = (current as i128 + delta as i128).clamp(0, rows.len().saturating_sub(1) as i128);
        let target = rows[abs as usize];
        self.state.update(cx, |state, _| {
            if extend {
                if !state.selected_layers.contains(&target) {
                    state.selected_layers.push(target);
                }
            } else {
                state.selected_layers = vec![target];
            }
            state.selected_layer = target;
        });
        cx.notify();
    }

    fn toggle_active_row_visibility(&mut self, cx: &mut Context<Self>) {
        if self.state.read(cx).layer_tab != LayerTab::Layers {
            return;
        }
        let Some(core) = self.active_row_core(cx) else {
            return;
        };
        self.toggle_layer_visibility(core, cx);
    }

    fn toggle_active_row_lock(&mut self, cx: &mut Context<Self>) {
        if self.state.read(cx).layer_tab != LayerTab::Layers {
            return;
        }
        let Some(core) = self.active_row_core(cx) else {
            return;
        };
        self.toggle_layer_locked(core, cx);
    }

    fn active_row_core(&self, cx: &Context<Self>) -> Option<String> {
        let state = self.state.read(cx);
        let active = state.selected_layer;
        state
            .layers
            .iter()
            .find(|row| row.id == active)
            .and_then(|row| row.core_id.clone())
    }
}

fn row_current(rows: &[u64], selected_layer: u64, selected_layers: &[u64]) -> usize {
    if let Some(index) = rows.iter().position(|row| *row == selected_layer) {
        return index;
    }
    selected_layers
        .iter()
        .find_map(|id| rows.iter().position(|row| row == id))
        .unwrap_or(0)
}
