//! The input actions and the helpers they call: setting a tool, cancelling a
//! gesture, toggling the selection shape, and stepping the brush.

use super::*;

impl App {
    pub(crate) fn set_canvas_tool_action(
        &mut self,
        action: &SetCanvasTool,
        cx: &mut Context<Self>,
    ) {
        self.select_canvas_tool(action.0, cx);
    }

    pub(crate) fn cancel_canvas_gesture_action(
        &mut self,
        _: &CancelCanvasGesture,
        cx: &mut Context<Self>,
    ) {
        let (
            brush_menu,
            context_menu,
            open_select,
            more_open,
            active_tool,
            stroke_popover,
            text_stroke_popover,
        ) = {
            let state = self.state.read(cx);
            (
                state.brush_menu.is_some(),
                state.context_menu.is_some(),
                state.open_select.is_some(),
                state.more_open,
                state.active_tool,
                state.stroke_popover_open,
                state.text_stroke_popover_open,
            )
        };
        if brush_menu {
            self.close_brush_menu(cx);
            return;
        }
        if context_menu {
            self.close_context_menu(cx);
            return;
        }
        if active_tool == ToolId::Text && text_stroke_popover {
            self.close_text_stroke_popover(cx);
            return;
        }
        if active_tool == ToolId::Shape && stroke_popover {
            self.close_stroke_popover(cx);
            return;
        }
        if open_select {
            self.state.update(cx, |state, _| state.open_select = None);
            cx.notify();
            return;
        }
        if more_open {
            self.state.update(cx, |state, _| state.more_open = false);
            cx.notify();
            return;
        }

        let move_idle = self.state.read(cx).active_tool == ToolId::Move
            && self.transform_drag.is_none()
            && self.group_transform_drag.is_none()
            && self.move_marquee.is_none()
            && !self.selection_edit_mode;
        if move_idle {
            let has_selection = !self.state.read(cx).selected_layers.is_empty();
            if has_selection {
                self.state.update(cx, |state, _| {
                    state.selected_layers.clear();
                    state.selected_layer = 0;
                    state.multi_bounds = None;
                });
                cx.notify();
            }
            return;
        }

        self.cancel_canvas_gesture(cx);
        self.selection_edit_mode = false;
    }

    pub(crate) fn toggle_selection_shape_action(
        &mut self,
        _: &ToggleSelectionShape,
        cx: &mut Context<Self>,
    ) {
        self.state.update(cx, |state, _| {
            state.selection_shape = match state.selection_shape {
                SelectionShape::Rect => SelectionShape::Ellipse,
                SelectionShape::Ellipse => SelectionShape::Rect,
            };
        });
        cx.notify();
    }

    pub(crate) fn decrease_brush_size_action(
        &mut self,
        _: &DecreaseBrushSize,
        cx: &mut Context<Self>,
    ) {
        self.step_brush(cx, -1.0, 0.0);
    }

    pub(crate) fn increase_brush_size_action(
        &mut self,
        _: &IncreaseBrushSize,
        cx: &mut Context<Self>,
    ) {
        self.step_brush(cx, 1.0, 0.0);
    }

    pub(crate) fn decrease_brush_hardness_action(
        &mut self,
        _: &DecreaseBrushHardness,
        cx: &mut Context<Self>,
    ) {
        self.step_brush(cx, 0.0, -1.0);
    }

    pub(crate) fn increase_brush_hardness_action(
        &mut self,
        _: &IncreaseBrushHardness,
        cx: &mut Context<Self>,
    ) {
        self.step_brush(cx, 0.0, 1.0);
    }

    /// Bracket steps from the paint-tool chain: size moves by ~10% of the
    /// current size (at least 1 px) and hardness by 0.1, only for the paint
    /// tools. The native eraser shares `brush_size`.
    pub(super) fn step_brush(
        &mut self,
        cx: &mut Context<Self>,
        size_direction: f64,
        hardness_direction: f64,
    ) {
        let tool = self.state.read(cx).active_tool;
        if !matches!(tool, ToolId::Brush | ToolId::Eraser) {
            return;
        }
        self.state.update(cx, |state, _| {
            let eraser = tool == ToolId::Eraser;
            if size_direction != 0.0 {
                let current = if eraser {
                    state.eraser_size
                } else {
                    state.brush_size
                };
                let step = (current * 0.1).round().max(1.0);
                let next = (current + size_direction * step).clamp(1.0, 2000.0);
                if eraser {
                    state.eraser_size = next;
                } else {
                    state.brush_size = next;
                }
            }
            if hardness_direction != 0.0 {
                if eraser {
                    state.eraser_hardness =
                        (state.eraser_hardness + hardness_direction * 0.1).clamp(0.0, 1.0);
                } else {
                    state.brush_hardness =
                        (state.brush_hardness + hardness_direction * 0.1).clamp(0.0, 1.0);
                }
            }
        });
        cx.notify();
    }

    pub(super) fn select_canvas_tool(&mut self, tool: ToolId, cx: &mut Context<Self>) {
        self.cancel_canvas_gesture(cx);
        self.selection_edit_mode = false;
        // Entering Crop with no pending frame opens the default frame
        // over the whole page (the crop overlay renders as soon as the tool arms).
        let seed_crop = if tool == ToolId::Crop && self.state.read(cx).crop_rect.is_none() {
            self.active_document_size().ok()
        } else {
            None
        };
        self.state.update(cx, |state, _| {
            state.active_tool = tool;
            if tool == ToolId::Selection {
                state.selection_shape = SelectionShape::Rect;
            }
            if tool != ToolId::Crop {
                state.crop_rect = None;
                state.crop_history.clear();
                state.crop_redo.clear();
            } else if let Some((doc_w, doc_h)) = seed_crop {
                state.crop_rect = Some([0.0, 0.0, doc_w, doc_h]);
                state.crop_history.clear();
                state.crop_redo.clear();
            }
            state.selection_drag = None;
        });
        cx.notify();
    }
}
