//! Capture surface for the Navigator's zoom track. A plain `div` receives
//! `on_mouse_move` only while hovered, so a drag released outside the track
//! would be lost; this element inserts a hitbox, captures it on left-down, and
//! routes move and release through the capture, the way the option bar's
//! slider surface does. The value side lives in `App::set_zoom_from_x`, so
//! this element carries no slider machinery of its own.

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::{
    AnyElement, App, Bounds, DispatchPhase, Element, ElementId, GlobalElementId, Hitbox,
    HitboxBehavior, HitboxId, InspectorElementId, IntoElement, LayoutId, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, WeakEntity, Window,
};

pub(crate) struct ZoomSurface {
    visual: AnyElement,
    app: WeakEntity<crate::app::App>,
    bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    capture: Rc<Cell<Option<HitboxId>>>,
}

impl ZoomSurface {
    pub(crate) fn new(
        visual: impl IntoElement,
        app: WeakEntity<crate::app::App>,
        bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
        capture: Rc<Cell<Option<HitboxId>>>,
    ) -> Self {
        Self {
            visual: visual.into_any_element(),
            app,
            bounds,
            capture,
        }
    }
}

impl IntoElement for ZoomSurface {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for ZoomSurface {
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
        let down_capture = Rc::clone(&self.capture);
        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
            if phase != DispatchPhase::Bubble
                || event.button != MouseButton::Left
                || !down_hitbox.contains(&event.position)
            {
                return;
            }
            // A modal dialog owns the input: the track behind it takes no drag.
            let blocked = down_app
                .upgrade()
                .is_some_and(|app| app.update(cx, |app, cx| app.modal_input_active(cx)));
            if blocked {
                return;
            }
            window.capture_pointer(down_hitbox.id);
            down_capture.set(Some(down_hitbox.id));
            if let Some(app) = down_app.upgrade() {
                app.update(cx, |app, cx| {
                    app.set_zoom_from_x(f32::from(event.position.x), cx);
                });
            }
        });

        let move_app = self.app.clone();
        let move_capture = Rc::clone(&self.capture);
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
            if phase != DispatchPhase::Bubble || !owns_capture(window, &move_capture) {
                return;
            }
            if let Some(app) = move_app.upgrade() {
                let blocked = app.update(cx, |app, cx| app.modal_input_active(cx));
                if blocked {
                    window.release_pointer();
                    move_capture.set(None);
                    return;
                }
                app.update(cx, |app, cx| {
                    app.set_zoom_from_x(f32::from(event.position.x), cx);
                });
            }
        });

        let up_capture = Rc::clone(&self.capture);
        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, _cx| {
            if phase != DispatchPhase::Bubble || !owns_capture(window, &up_capture) {
                return;
            }
            let _ = event;
            up_capture.set(None);
        });

        self.visual.paint(window, cx);
    }
}

/// Whether this surface's capture is the one the window currently routes to,
/// the same guard the option bar's slider surface uses.
fn owns_capture(window: &Window, capture: &Cell<Option<HitboxId>>) -> bool {
    let Some(captured) = capture.get() else {
        return false;
    };
    window.captured_hitbox() == Some(captured)
}
