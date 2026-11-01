//! Option-bar dropdowns: one open at a time; choosing an option applies it and
//! closes the panel. Escape closes without changing the value.

use gpui_kit::{Context, Window};

use super::App;

impl App {
    pub(crate) fn toggle_select(
        &mut self,
        id: &'static str,
        active: usize,
        count: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let open = self.state.read(cx).open_select == Some(id);
        self.state.update(cx, |state, _| {
            if open {
                state.open_select = None;
            } else {
                state.open_select = Some(id);
                // No current value (the brush default is `usize::MAX`) opens
                // on the first row, as an unset selection does.
                state.select_cursor = if active < count { active } else { 0 };
                state.select_count = count;
                // The brush preset list lives inside the overflow panel; any
                // other bar dropdown replaces the overflow instead.
                if id != "opt-brush-preset" {
                    state.more_open = false;
                }
            }
        });
        if !open {
            // Escape is bound in the Canvas context; keep that context active
            // while the dropdown is open so it closes without changing focus
            // behaviour elsewhere.
            window.focus(&self.canvas_focus_handle, cx);
        }
        cx.notify();
    }

    /// Moves the open dropdown's highlight; Enter commits it.
    pub(crate) fn select_arrow(&mut self, delta: isize, cx: &mut Context<Self>) {
        let state = self.state.read(cx);
        let Some(_id) = state.open_select else {
            return;
        };
        let (count, cursor) = (state.select_count, state.select_cursor);
        if count == 0 {
            return;
        }
        let next = (cursor as isize + delta).rem_euclid(count as isize) as usize;
        self.state.update(cx, |state, _| state.select_cursor = next);
        cx.notify();
    }

    pub(crate) fn select_confirm(&mut self, cx: &mut Context<Self>) {
        let state = self.state.read(cx);
        let Some(id) = state.open_select else {
            return;
        };
        let cursor = state.select_cursor;
        self.choose_select(id, cursor, cx);
    }

    pub(crate) fn close_select(&mut self, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| {
            state.open_select = None;
            state.more_open = false;
        });
        cx.notify();
    }

    /// The option bar's `MoreDropdown`: independent of the select list so a
    /// preset list can open inside the panel without unmounting it.
    pub(crate) fn toggle_more_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let open = self.state.read(cx).more_open;
        self.state.update(cx, |state, _| {
            state.more_open = !open;
            if state.more_open {
                state.open_select = None;
            }
        });
        if !open {
            window.focus(&self.canvas_focus_handle, cx);
        }
        cx.notify();
    }

    pub(crate) fn choose_select(&mut self, id: &'static str, index: usize, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| {
            match id {
                "opt-gradient-type" => state.gradient_radial = index == 1,
                "opt-gradient-preset" => state.gradient_fg_transparent = index == 1,
                "opt-select-shape" => {
                    state.selection_shape = if index == 1 {
                        crate::state::SelectionShape::Ellipse
                    } else {
                        crate::state::SelectionShape::Rect
                    };
                }
                "opt-shape-kind" => {
                    state.shape_kind = match index {
                        1 => crate::state::ShapeTool::Ellipse,
                        2 => crate::state::ShapeTool::Line,
                        3 => crate::state::ShapeTool::Triangle,
                        4 => crate::state::ShapeTool::Star,
                        5 => crate::state::ShapeTool::BlockArrow,
                        6 => crate::state::ShapeTool::Heart,
                        7 => crate::state::ShapeTool::Diamond,
                        8 => crate::state::ShapeTool::SpeechBubble,
                        9 => crate::state::ShapeTool::Hexagon,
                        _ => crate::state::ShapeTool::Rect,
                    };
                    // Choosing a kind clears the arrow head for every
                    // preset except the dedicated Arrow entry.
                    state.shape_arrow_head = false;
                }
                _ => {}
            }
            state.open_select = None;
        });
        if id == "opt-weight" {
            let weight = crate::app::tool_settings::TEXT_WEIGHTS
                .get(index)
                .copied()
                .unwrap_or(400.0);
            if let Err(error) = self.set_text_font_weight(weight, cx) {
                self.set_status_error(Some(format!("Text: {error}")), cx);
            }
        }
        if id == "opt-shape-kind" && self.state.read(cx).selected_shape.is_some() {
            let kind =
                crate::app::tool_settings::shape_kind_for_tool(self.state.read(cx).shape_kind);
            if let Err(error) = self.edit_selected_shape(cx, |params| {
                params.kind = kind.clone();
                params.arrow_head = false;
            }) {
                self.set_status_error(Some(format!("Shape: {error}")), cx);
            }
        }
        if id == "resize-unit" {
            let unit = crate::units::DimensionUnit::ALL
                .get(index)
                .copied()
                .unwrap_or_default();
            self.set_resize_unit(unit, cx);
        }
        if id == "opt-brush-preset" {
            const BRUSH: [usize; 4] = [0, 1, 2, 3];
            const ERASER: [usize; 6] = [0, 1, 2, 3, 4, 5];
            let eraser = self.state.read(cx).active_tool == crate::state::ToolId::Eraser;
            let table: &[usize] = if eraser { &ERASER } else { &BRUSH };
            if let Some(global) = table.get(index) {
                self.apply_brush_preset(*global, cx);
            }
        }
        cx.notify();
    }
}
