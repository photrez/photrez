//! Paint-colour picker: the tool rail's swatches open it, the SV/hue surfaces
//! and the hex field edit the live paint colour, and Cancel restores the
//! colour the picker opened on.
//!
//! `fg_color` / `bg_color` feed the brush tip, shapes, text and the
//! eyedropper, so those targets need no document command. The text bar's
//! swatch is the exception: with a text layer selected the picker edits that
//! layer's colour through the same history command the bar's other controls
//! use; in draw mode it falls back to the paint foreground.

use gpui_kit::{Context, KeyDownEvent, Window};

use crate::palette::{hsv_to_rgb, parse_hex, rgb_to_hsv, to_hex};
use crate::state::{ColorPickerSession, ColorTarget, PickerArea};

use super::App;

impl App {
    pub(crate) fn release_color_picker_pointer(&mut self, window: &mut Window) {
        let Some((_, hitbox)) = self.color_picker_capture.take() else {
            return;
        };
        if window.captured_hitbox() == Some(hitbox) {
            window.release_pointer();
        }
    }

    pub(crate) fn cancel_color_picker_for_document_change(&mut self, cx: &mut Context<Self>) {
        let session = self.state.update(cx, |state, _| state.color_picker.take());
        if let Some(session) = session {
            self.apply_color(session.target, session.original, cx);
        }
    }

    pub(crate) fn current_color(&self, target: ColorTarget, cx: &Context<Self>) -> [u8; 4] {
        let state = self.state.read(cx);
        match target {
            ColorTarget::Foreground => state.fg_color,
            ColorTarget::Background => state.bg_color,
            ColorTarget::Text => state.selected_text_color.unwrap_or(state.fg_color),
            ColorTarget::TextStroke => state
                .selected_text_stroke_color
                .unwrap_or(state.text_stroke_color),
            ColorTarget::ShapeStroke => state.shape_stroke_color,
            ColorTarget::ShapeFill => state
                .selected_shape
                .as_ref()
                .and_then(|shape| parse_hex(&shape.fill.color))
                .unwrap_or(state.fg_color),
        }
    }

    pub(crate) fn open_color_picker(
        &mut self,
        target: ColorTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.release_color_picker_pointer(window);
        let original = self.current_color(target, cx);
        self.state.update(cx, |state, _| {
            state.color_picker = Some(ColorPickerSession {
                target,
                hsv: rgb_to_hsv(original),
                original,
                hex: to_hex(original),
            });
        });
        window.focus(&self.picker_focus, cx);
        cx.notify();
    }

    /// Dismisses the picker. `commit == false` puts the opening colour back:
    /// the picker applies live and reverts on cancel.
    pub(crate) fn close_color_picker(
        &mut self,
        commit: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.release_color_picker_pointer(window);
        let Some(session) = self.state.read(cx).color_picker.clone() else {
            window.focus(&self.canvas_focus_handle, cx);
            return;
        };
        self.state.update(cx, |state, _| state.color_picker = None);
        if !commit {
            self.apply_color(session.target, session.original, cx);
        }
        window.focus(&self.canvas_focus_handle, cx);
        cx.notify();
    }

    /// Pointer position in `0..=1` inside the surface it was dragged on.
    pub(crate) fn pick_color_at(
        &mut self,
        area: PickerArea,
        x: f32,
        y: f32,
        cx: &mut Context<Self>,
    ) {
        if self.state.read(cx).color_picker.is_none() {
            return;
        }
        let (x, y) = (x.clamp(0.0, 1.0), y.clamp(0.0, 1.0));
        self.state.update(cx, |state, _| {
            if let Some(session) = state.color_picker.as_mut() {
                match area {
                    PickerArea::Sv => {
                        session.hsv.s = x;
                        session.hsv.v = 1.0 - y;
                    }
                    PickerArea::Hue => session.hsv.h = y * 360.0,
                }
                session.hex = to_hex(hsv_to_rgb(session.hsv));
            }
        });
        let Some(session) = self.state.read(cx).color_picker.clone() else {
            return;
        };
        self.apply_color(session.target, hsv_to_rgb(session.hsv), cx);
    }

    pub(crate) fn color_picker_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.state.read(cx).color_picker.is_none() {
            return false;
        }
        match event.keystroke.key.as_str() {
            "escape" => {
                self.close_color_picker(false, window, cx);
                true
            }
            "enter" => {
                self.commit_color_picker(window, cx);
                true
            }
            "backspace" => {
                self.state.update(cx, |state, _| {
                    if let Some(session) = state.color_picker.as_mut() {
                        session.hex.pop();
                    }
                });
                cx.notify();
                true
            }
            _ => {
                let text = event
                    .keystroke
                    .key_char
                    .clone()
                    .unwrap_or_else(|| event.keystroke.key.clone());
                let accepted = text.chars().all(|c| c == '#' || c.is_ascii_hexdigit())
                    && text.chars().count() == 1;
                if !accepted {
                    return false;
                }
                self.state.update(cx, |state, _| {
                    if let Some(session) = state.color_picker.as_mut() {
                        // The field opens holding the full `#RRGGBB` of the
                        // current colour, so a fresh digit replaces that value
                        // instead of being dropped.
                        if session.hex.trim_start_matches('#').len() >= 6 {
                            session.hex.clear();
                        }
                        session.hex.push_str(&text);
                    }
                });
                cx.notify();
                true
            }
        }
    }

    /// Enter / OK: adopt the typed hex when it parses, otherwise stay open so
    /// the user can fix or cancel instead of silently keeping a stale colour.
    pub(crate) fn commit_color_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.state.read(cx).color_picker.clone() else {
            return;
        };
        match parse_hex(&session.hex) {
            Some(color) => {
                self.release_color_picker_pointer(window);
                self.state.update(cx, |state, _| state.color_picker = None);
                self.apply_color(session.target, color, cx);
                window.focus(&self.canvas_focus_handle, cx);
                cx.notify();
            }
            None => self.set_status_error(Some("Color: enter #RRGGBB".to_string()), cx),
        }
    }

    pub(crate) fn swap_colors(&mut self, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| {
            std::mem::swap(&mut state.fg_color, &mut state.bg_color);
        });
        cx.notify();
    }

    fn apply_color(&mut self, target: ColorTarget, color: [u8; 4], cx: &mut Context<Self>) {
        if target == ColorTarget::Text {
            // Edit mode edits the layer; draw mode is the paint foreground.
            // Repeated pointer events over one spot must not stack history
            // entries, which is the no-op guard on an unchanged colour.
            if self.state.read(cx).selected_text_color.is_some() {
                if self.state.read(cx).selected_text_color == Some(color) {
                    return;
                }
                let _ = self.edit_selected_text(cx, |data| data.color = to_hex(color));
            } else {
                self.state.update(cx, |state, _| state.fg_color = color);
            }
            cx.notify();
            return;
        }
        if target == ColorTarget::TextStroke {
            if self.state.read(cx).selected_layer_type.as_deref() == Some("text") {
                if self.state.read(cx).selected_text_stroke_color == Some(color) {
                    return;
                }
                self.set_text_stroke_color(color, cx);
            } else {
                self.state
                    .update(cx, |state, _| state.text_stroke_color = color);
                cx.notify();
            }
            return;
        }
        if target == ColorTarget::ShapeFill {
            // The shape fill always follows the paint foreground; edit mode
            // also writes the selected shape through core history, with the
            // same no-op guard as the stroke/text targets.
            let selected_fill = self
                .state
                .read(cx)
                .selected_shape
                .as_ref()
                .and_then(|shape| parse_hex(&shape.fill.color));
            self.state.update(cx, |state, _| state.fg_color = color);
            if let Some(current) = selected_fill {
                if current == color {
                    cx.notify();
                    return;
                }
                self.set_shape_fill_color(color, cx);
            }
            cx.notify();
            return;
        }
        if target == ColorTarget::ShapeStroke && self.state.read(cx).selected_shape.is_some() {
            self.set_shape_stroke_color(color, cx);
            return;
        }
        self.state.update(cx, |state, _| match target {
            ColorTarget::Foreground => state.fg_color = color,
            ColorTarget::Background => state.bg_color = color,
            // Choosing a stroke colour also enables the outline so a new
            // stroke colour is never invisible.
            ColorTarget::ShapeStroke => {
                state.shape_stroke_color = color;
                state.shape_stroke = true;
            }
            ColorTarget::Text => {}
            ColorTarget::TextStroke => {}
            ColorTarget::ShapeFill => {}
        });
        cx.notify();
    }
}
