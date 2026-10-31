//! Capture surface for an option-bar slider. A plain `div` receives
//! `on_mouse_move`/`on_mouse_up` only while its hitbox is hovered, so a drag
//! released outside the track would be lost. This element inserts a hitbox,
//! captures it on left-down, and routes move/up through the capture.

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::{
    AnyElement, App, Bounds, DispatchPhase, Element, ElementId, GlobalElementId, Hitbox,
    HitboxBehavior, HitboxId, InspectorElementId, IntoElement, LayoutId, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, WeakEntity, Window,
};

pub(crate) struct BarSliderSurface {
    visual: AnyElement,
    app: WeakEntity<crate::app::App>,
    target: crate::state::SliderTarget,
    bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    capture: Rc<Cell<Option<HitboxId>>>,
    focus: Option<gpui_kit::FocusHandle>,
}

impl BarSliderSurface {
    pub(crate) fn new(
        visual: impl IntoElement,
        app: WeakEntity<crate::app::App>,
        target: crate::state::SliderTarget,
        bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
        capture: Rc<Cell<Option<HitboxId>>>,
        focus: Option<gpui_kit::FocusHandle>,
    ) -> Self {
        Self {
            visual: visual.into_any_element(),
            app,
            target,
            bounds,
            capture,
            focus,
        }
    }
}

impl IntoElement for BarSliderSurface {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for BarSliderSurface {
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
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        hitbox: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.bounds.set(Some(bounds));

        let down_hitbox = hitbox.clone();
        let down_app = self.app.clone();
        let down_target = self.target;
        let down_capture = Rc::clone(&self.capture);
        let down_focus = self.focus.clone();
        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
            if phase != DispatchPhase::Bubble
                || event.button != MouseButton::Left
                || !down_hitbox.contains(&event.position)
            {
                return;
            }
            if let Some(current) = window.captured_hitbox() {
                if down_capture.get() != Some(current) {
                    return;
                }
                window.release_pointer();
            }
            window.capture_pointer(down_hitbox.id);
            down_capture.set(Some(down_hitbox.id));
            if let Some(handle) = down_focus.as_ref() {
                window.focus(handle, cx);
            }
            if let Some(app) = down_app.upgrade() {
                app.update(cx, |app, cx| {
                    app.slider_pointer_down_for(down_target, f32::from(event.position.x), cx);
                });
            }
        });

        let move_app = self.app.clone();
        let move_target = self.target;
        let move_capture = Rc::clone(&self.capture);
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, _cx| {
            if phase != DispatchPhase::Bubble || !owns_capture(window, &move_capture) {
                return;
            }
            if let Some(app) = move_app.upgrade() {
                app.update(_cx, |app, cx| {
                    app.slider_pointer_move_for(move_target, f32::from(event.position.x), cx);
                });
            }
        });

        let up_app = self.app.clone();
        let up_capture = Rc::clone(&self.capture);
        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
            if phase != DispatchPhase::Bubble || !owns_capture(window, &up_capture) {
                return;
            }
            up_capture.set(None);
            if let Some(app) = up_app.upgrade() {
                let _ = event;
                app.update(cx, |app, _cx| app.slider_pointer_up());
            }
        });

        self.visual.paint(window, cx);
    }
}

fn owns_capture(window: &Window, capture: &Cell<Option<HitboxId>>) -> bool {
    let Some(captured) = capture.get() else {
        return false;
    };
    window.captured_hitbox() == Some(captured)
}
