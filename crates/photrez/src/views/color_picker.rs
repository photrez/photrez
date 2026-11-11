//! Paint-colour picker, reproducing the captured dialog content.
//!
//! The layout: a 200x200 saturation/value box, an 18x200 hue
//! strip, then a 180px column holding the new/current swatch, OK/Cancel, and
//! the HSB/RGB readouts. Two deliberate differences, both recorded in the
//! ledger:
//!
//! * HSB/RGB are readouts. The captured dialog's six number inputs are a
//!   separate primitive job (`NumField` per field); the picker's own inputs
//!   are the SV/hue surfaces and the hex field.
//! * Dismissal is Cancel/Escape/outside-click. Both surfaces capture the
//!   pointer so a drag keeps tracking after it leaves the box.

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::{
    div, px, AnyElement, App, Bounds, Context, DispatchPhase, Element, ElementId, Entity,
    FocusHandle, GlobalElementId, Hitbox, HitboxBehavior, HitboxId, InspectorElementId,
    IntoElement, LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels,
    TestSupportExt, WeakEntity, Window,
};

use crate::i18n::t;
use crate::palette::{hsla, hsv_to_rgb, Hsv};
use crate::state::{AppState, ColorPickerSession, PickerArea};
use crate::tokens::color;

const PANEL_W: f32 = 470.0;
const PANEL_LEFT: f32 = 448.0;
const PANEL_TOP: f32 = 150.0;
const SV: f32 = 200.0;
const HUE_W: f32 = 18.0;
const COLUMN_W: f32 = 180.0;
const SWATCH_BOX: f32 = 48.0;
const BUTTON_W: f32 = 84.0;
const BUTTON_H: f32 = 28.0;
const FIELD_H: f32 = 28.0;

fn white() -> gpui_kit::Hsla {
    hsla([255, 255, 255, 255])
}

fn black() -> gpui_kit::Hsla {
    hsla([0, 0, 0, 255])
}

fn pure_hue(h: f32) -> gpui_kit::Hsla {
    hsla(hsv_to_rgb(Hsv { h, s: 1.0, v: 1.0 }))
}

pub(crate) fn render(
    state: &Entity<AppState>,
    focus: &FocusHandle,
    capture: Rc<Cell<Option<(PickerArea, HitboxId)>>>,
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    let Some(session) = state.read(cx).color_picker.clone() else {
        return div().into_any_element();
    };
    div()
        .id("color-picker-layer")
        .test_support()
        .absolute()
        .inset(px(0.0))
        .track_focus(focus)
        .on_key_down(
            cx.listener(|app, event: &gpui_kit::KeyDownEvent, window, cx| {
                app.color_picker_key(event, window, cx);
            }),
        )
        .child(panel(&session, capture, cx))
        .into_any_element()
}

fn panel(
    session: &ColorPickerSession,
    capture: Rc<Cell<Option<(PickerArea, HitboxId)>>>,
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    let title = match session.target {
        crate::state::ColorTarget::Foreground => t("tools.foregroundColor"),
        crate::state::ColorTarget::Background => t("tools.backgroundColor"),
        crate::state::ColorTarget::Text => t("tools.options.textColor"),
        // This dialog title is a literal, not a catalogue key.
        crate::state::ColorTarget::ShapeStroke => "Shape Stroke Color".to_string(), // i18n-exempt: literal dialog title
        crate::state::ColorTarget::ShapeFill => "Shape Fill Color".to_string(), // i18n-exempt: literal dialog title
        crate::state::ColorTarget::TextStroke => "Stroke Color".to_string(), // i18n-exempt: literal dialog title
    };
    let content = v_flex()
        .w(px(PANEL_W))
        .rounded(px(8.0))
        .border_1()
        .border_color(color::FIELD_BORDER)
        .bg(color::PANEL)
        .overflow_hidden()
        .shadow_lg()
        .child(header(title, cx))
        .child(body(session, capture, cx));

    div()
        .id("color-picker")
        .test_support()
        .absolute()
        .left(px(PANEL_LEFT))
        .top(px(PANEL_TOP))
        .child(PickerHitbox::new(
            content,
            cx.entity().downgrade(),
            None,
            Rc::new(Cell::new(None)),
        ))
}

fn header(title: String, cx: &Context<crate::app::App>) -> impl IntoElement {
    div()
        .h(px(40.0))
        .px(px(20.0))
        .flex()
        .items_center()
        .justify_between()
        .border_b_1()
        .border_color(color::DIVIDER)
        .child(
            div()
                .text_size(px(12.0))
                .text_color(color::TEXT)
                .child(title),
        )
        .child(
            div()
                .id("color-picker-close")
                .test_support()
                .size(px(20.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(4.0))
                .text_size(px(12.0))
                .text_color(color::ICON)
                .hover(|s| s.bg(color::WHITE_5).text_color(color::TEXT))
                .on_click(cx.listener(|app, _: &gpui_kit::ClickEvent, window, cx| {
                    cx.stop_propagation();
                    app.close_color_picker(false, window, cx);
                }))
                .child("\u{00d7}"), // i18n-exempt: close glyph
        )
}

fn body(
    session: &ColorPickerSession,
    capture: Rc<Cell<Option<(PickerArea, HitboxId)>>>,
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    h_flex()
        .px(px(20.0))
        .py(px(16.0))
        .gap(px(16.0))
        .items_start()
        .child(sv_box(session, Rc::clone(&capture), cx))
        .child(hue_strip(session, capture, cx))
        .child(column(session, cx))
}

fn sv_box(
    session: &ColorPickerSession,
    capture: Rc<Cell<Option<(PickerArea, HitboxId)>>>,
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    let hsv = session.hsv;
    let indicator_x = hsv.s * SV - 6.0;
    let indicator_y = (1.0 - hsv.v) * SV - 6.0;
    let visual = div()
        .size(px(SV))
        .relative()
        .rounded(px(4.0))
        .overflow_hidden()
        .bg(pure_hue(hsv.h))
        .child(
            div()
                .absolute()
                .inset(px(0.0))
                .bg(gpui_kit::linear_gradient(
                    90.0,
                    gpui_kit::linear_color_stop(white(), 0.0),
                    gpui_kit::linear_color_stop(white().opacity(0.0), 1.0),
                )),
        )
        .child(
            div()
                .absolute()
                .inset(px(0.0))
                .bg(gpui_kit::linear_gradient(
                    0.0,
                    gpui_kit::linear_color_stop(black(), 0.0),
                    gpui_kit::linear_color_stop(black().opacity(0.0), 1.0),
                )),
        )
        .child(
            div()
                .absolute()
                .left(px(indicator_x))
                .top(px(indicator_y))
                .size(px(12.0))
                .rounded_full()
                .border_1()
                .border_color(white())
                .shadow(vec![gpui_kit::BoxShadow::new(
                    px(0.0),
                    px(0.0),
                    black().opacity(0.5),
                )
                .spread_radius(px(1.0))]),
        );

    div()
        .id("color-picker-sv")
        .test_support()
        .size(px(SV))
        .flex_shrink_0()
        .cursor(gpui_kit::CursorStyle::Crosshair)
        .child(PickerHitbox::new(
            visual,
            cx.entity().downgrade(),
            Some(PickerArea::Sv),
            capture,
        ))
}

fn hue_strip(
    session: &ColorPickerSession,
    capture: Rc<Cell<Option<(PickerArea, HitboxId)>>>,
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    let hsv = session.hsv;
    let stop_hues = [0.0, 60.0, 120.0, 180.0, 240.0, 300.0, 360.0];
    let mut visual = div()
        .w(px(HUE_W))
        .h(px(SV))
        .relative()
        .rounded(px(4.0))
        .overflow_hidden()
        .flex()
        .flex_col();
    for pair in stop_hues.windows(2) {
        visual = visual.child(div().flex_1().w_full().bg(gpui_kit::linear_gradient(
            180.0,
            gpui_kit::linear_color_stop(pure_hue(pair[0]), 0.0),
            gpui_kit::linear_color_stop(pure_hue(pair[1]), 1.0),
        )));
    }
    let visual = visual.child(
        div()
            .absolute()
            .left(px(0.0))
            .right(px(0.0))
            .top(px(hsv.h / 360.0 * SV - 3.0))
            .h(px(6.0))
            .border_t_1()
            .border_b_1()
            .border_color(white())
            .bg(black().opacity(0.1)),
    );

    div()
        .id("color-picker-hue")
        .test_support()
        .w(px(HUE_W))
        .h(px(SV))
        .flex_shrink_0()
        .cursor(gpui_kit::CursorStyle::ResizeUpDown)
        .child(PickerHitbox::new(
            visual,
            cx.entity().downgrade(),
            Some(PickerArea::Hue),
            capture,
        ))
}

fn column(session: &ColorPickerSession, cx: &Context<crate::app::App>) -> impl IntoElement {
    let rgb = hsv_to_rgb(session.hsv);
    let readouts = channel_readouts(session);
    v_flex()
        .w(px(COLUMN_W))
        .flex_shrink_0()
        .gap(px(14.0))
        .child(
            h_flex()
                .w_full()
                .items_start()
                .justify_between()
                .child(
                    v_flex()
                        .w(px(86.0))
                        .items_center()
                        .child(label(t("common.newColor")))
                        .child(
                            v_flex()
                                .w(px(SWATCH_BOX))
                                .h(px(SWATCH_BOX))
                                .rounded(px(4.0))
                                .border_1()
                                .border_color(color::FIELD_BORDER)
                                .overflow_hidden()
                                .child(
                                    div()
                                        .flex_1()
                                        .w_full()
                                        .bg(hsla(rgb))
                                        .border_b_1()
                                        .border_color(color::BG),
                                )
                                .child(div().flex_1().w_full().bg(hsla(session.original))),
                        )
                        .child(label(t("common.currentColor"))),
                )
                .child(
                    v_flex()
                        .w(px(86.0))
                        .items_end()
                        .gap(px(6.0))
                        .child(action_button(
                            "color-picker-ok",
                            t("common.ok"),
                            true,
                            cx,
                            ColorAction::Commit,
                        ))
                        .child(action_button(
                            "color-picker-cancel",
                            t("common.cancel"),
                            false,
                            cx,
                            ColorAction::Cancel,
                        )),
                ),
        )
        .child(
            h_flex()
                .w_full()
                .items_start()
                .justify_between()
                .child(readout_group(&readouts[..3]))
                .child(readout_group(&readouts[3..])),
        )
        .child(
            h_flex()
                .w_full()
                .items_center()
                .gap(px(8.0))
                .child(
                    div()
                        .text_size(px(9.0))
                        .text_color(color::TEXT_DIM)
                        .child("HEX"), // i18n-exempt: native-only input caption
                )
                .child(
                    h_flex()
                        .id("color-picker-hex")
                        .test_support()
                        .flex_1()
                        .h(px(FIELD_H))
                        .px(px(8.0))
                        .items_center()
                        .rounded(px(4.0))
                        .border_1()
                        .border_color(color::FIELD_BORDER)
                        .bg(color::FIELD)
                        .text_size(px(11.0))
                        .text_color(color::TEXT)
                        .cursor(gpui_kit::CursorStyle::IBeam)
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|app, _: &MouseDownEvent, window, cx| {
                                window.focus(&app.picker_focus, cx);
                            }),
                        )
                        .child(format!("{}|", session.hex)),
                ),
        )
}

fn label(text: String) -> impl IntoElement {
    div()
        .my(px(4.0))
        .text_size(px(9.0))
        .text_color(color::TEXT_DIM)
        .child(text)
}

fn readout(label: &'static str, value: String, suffix: &'static str) -> impl IntoElement {
    h_flex()
        .w_full()
        .items_center()
        .justify_between()
        .child(
            div()
                .w(px(12.0))
                .text_size(px(11.0))
                .text_color(color::TEXT_DIM)
                .text_right()
                .child(label),
        )
        .child(
            div()
                .w(px(54.0))
                .h(px(FIELD_H))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(4.0))
                .border_1()
                .border_color(color::FIELD_BORDER)
                .bg(color::FIELD)
                .text_size(px(11.0))
                .text_color(color::TEXT)
                .child(value),
        )
        .child(
            div()
                .w(px(12.0))
                .text_size(px(11.0))
                .text_color(color::TEXT_DIM)
                .child(suffix),
        )
}

/// One channel field as the picker displays it: its caption, the value
/// derived from the live colour, and the unit printed after the value.
pub(crate) struct Readout {
    pub(crate) label: &'static str,
    pub(crate) value: String,
    pub(crate) suffix: &'static str,
}

/// The picker lists both colour models at once - hue/saturation/brightness
/// first, then red/green/blue - and every field derives from the live colour in
/// the open session, which is the only input the column can read. Deriving them
/// here, rather than in six inline expressions, keeps the captions and the
/// numbers on the same source value, so the two models cannot drift apart or
/// keep text from the previous colour.
pub(crate) fn channel_readouts(session: &ColorPickerSession) -> [Readout; 6] {
    let hsv = session.hsv;
    let rgb = hsv_to_rgb(hsv);
    [
        Readout {
            label: "H:", // i18n-exempt: colour-model abbreviation
            value: format!("{:.0}", hsv.h),
            suffix: "\u{00b0}",
        },
        Readout {
            label: "S:", // i18n-exempt: colour-model abbreviation
            value: format!("{:.0}", hsv.s * 100.0),
            suffix: "%",
        },
        Readout {
            label: "B:", // i18n-exempt: colour-model abbreviation
            value: format!("{:.0}", hsv.v * 100.0),
            suffix: "%",
        },
        Readout {
            label: "R:", // i18n-exempt: colour-model abbreviation
            value: rgb[0].to_string(),
            suffix: "",
        },
        Readout {
            label: "G:", // i18n-exempt: colour-model abbreviation
            value: rgb[1].to_string(),
            suffix: "",
        },
        Readout {
            label: "B:", // i18n-exempt: colour-model abbreviation
            value: rgb[2].to_string(),
            suffix: "",
        },
    ]
}

fn readout_group(items: &[Readout]) -> impl IntoElement {
    let mut group = v_flex().w(px(86.0)).gap(px(6.0));
    for item in items {
        group = group.child(readout(item.label, item.value.clone(), item.suffix));
    }
    group
}

enum ColorAction {
    Commit,
    Cancel,
}

fn action_button(
    id: &'static str,
    text: String,
    primary: bool,
    cx: &Context<crate::app::App>,
    action: ColorAction,
) -> impl IntoElement {
    let mut button = div()
        .id(id)
        .test_support()
        .w(px(BUTTON_W))
        .h(px(BUTTON_H))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(4.0))
        .text_size(px(11.5));
    button = if primary {
        button.bg(color::ACCENT).text_color(color::BG)
    } else {
        button
            .border_1()
            .border_color(color::BORDER)
            .text_color(color::TEXT_DIM)
            .hover(|s| s.text_color(color::TEXT))
    };
    let commit = matches!(action, ColorAction::Commit);
    button
        .on_click(
            cx.listener(move |app, _: &gpui_kit::ClickEvent, window, cx| {
                cx.stop_propagation();
                if commit {
                    app.commit_color_picker(window, cx);
                } else {
                    app.close_color_picker(false, window, cx);
                }
            }),
        )
        .child(text)
}

/// Lay-out-preserving hitbox: an SV/hue surface captures the pointer on
/// left-down and routes move/up while captured, so a drag released outside the
/// box keeps editing the colour; the panel variant only dismisses the picker
/// on a click outside its own bounds.
struct PickerHitbox {
    visual: AnyElement,
    app: WeakEntity<crate::app::App>,
    area: Option<PickerArea>,
    capture: Rc<Cell<Option<(PickerArea, HitboxId)>>>,
}

impl PickerHitbox {
    fn new(
        visual: impl IntoElement,
        app: WeakEntity<crate::app::App>,
        area: Option<PickerArea>,
        capture: Rc<Cell<Option<(PickerArea, HitboxId)>>>,
    ) -> Self {
        Self {
            visual: visual.into_any_element(),
            app,
            area,
            capture,
        }
    }
}

impl IntoElement for PickerHitbox {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for PickerHitbox {
    type RequestLayoutState = ();
    type PrepaintState = Hitbox;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        (self.visual.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.visual.prepaint(window, cx);
        window.insert_hitbox(bounds, HitboxBehavior::Normal)
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        hitbox: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(area) = self.area else {
            let panel_hitbox = hitbox.clone();
            let close_app = self.app.clone();
            window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                if phase != DispatchPhase::Bubble
                    || event.button != MouseButton::Left
                    || panel_hitbox.contains(&event.position)
                {
                    return;
                }
                if let Some(app) = close_app.upgrade() {
                    app.update(cx, |app, cx| {
                        app.close_color_picker(false, window, cx);
                    });
                }
            });
            self.visual.paint(window, cx);
            return;
        };

        let down_hitbox = hitbox.clone();
        let down_app = self.app.clone();
        let down_capture = Rc::clone(&self.capture);
        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
            if phase != DispatchPhase::Bubble
                || event.button != MouseButton::Left
                || !down_hitbox.contains(&event.position)
            {
                return;
            }
            if let Some(current) = window.captured_hitbox() {
                if down_capture.get() != Some((area, current)) {
                    return;
                }
                window.release_pointer();
            }
            window.capture_pointer(down_hitbox.id);
            down_capture.set(Some((area, down_hitbox.id)));
            let (x, y) = normalized(down_hitbox.bounds, event.position);
            if let Some(app) = down_app.upgrade() {
                app.update(cx, |app, cx| {
                    app.pick_color_at(area, x, y, cx);
                });
            }
        });

        let move_app = self.app.clone();
        let move_capture = Rc::clone(&self.capture);
        let move_hitbox = hitbox.clone();
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
            if phase != DispatchPhase::Bubble || !owns_capture(window, &move_capture, area) {
                return;
            }
            let (x, y) = normalized(move_hitbox.bounds, event.position);
            if let Some(app) = move_app.upgrade() {
                app.update(cx, |app, cx| {
                    if app.modal_input_active(cx) {
                        app.release_color_picker_pointer(window);
                        return;
                    }
                    app.pick_color_at(area, x, y, cx);
                });
            }
        });

        let up_capture = Rc::clone(&self.capture);
        window.on_mouse_event(move |_: &MouseUpEvent, phase, window, _cx| {
            if phase != DispatchPhase::Bubble || !owns_capture(window, &up_capture, area) {
                return;
            }
            up_capture.set(None);
            window.release_pointer();
        });

        self.visual.paint(window, cx);
    }
}

fn normalized(bounds: Bounds<Pixels>, position: gpui_kit::Point<Pixels>) -> (f32, f32) {
    let width = f32::from(bounds.size.width).max(1.0);
    let height = f32::from(bounds.size.height).max(1.0);
    let x = (f32::from(position.x) - f32::from(bounds.origin.x)) / width;
    let y = (f32::from(position.y) - f32::from(bounds.origin.y)) / height;
    (x, y)
}

fn owns_capture(
    window: &Window,
    capture: &Rc<Cell<Option<(PickerArea, HitboxId)>>>,
    area: PickerArea,
) -> bool {
    let Some((owner, captured)) = capture.get() else {
        return false;
    };
    owner == area && window.captured_hitbox() == Some(captured)
}

#[cfg(test)]
mod tests;
