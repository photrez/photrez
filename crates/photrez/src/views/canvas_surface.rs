//! Pointer surface for the canvas: it owns the hitbox that captures the mouse.
//!
//! A plain `div` receives `on_mouse_move`/`on_mouse_up` only while its hitbox is
//! hovered (`elements/div.rs`), so a fast stroke released outside the canvas
//! never commits. This element inserts its own hitbox, captures it on
//! mouse-down, and routes move/up through the capture. The wrapped surface
//! still owns focus, the `Canvas` key context, the cursor and the test id.
//!
//! The captured id is stored in a slot owned by the app: a repaint allocates a
//! fresh hitbox id, and the listeners registered by the new frame must keep
//! routing the capture that started on the old id.

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::{
    AnyElement, App, Bounds, DispatchPhase, Element, ElementId, FileDropEvent, GlobalElementId,
    Hitbox, HitboxBehavior, HitboxId, InspectorElementId, IntoElement, LayoutId, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, Pixels, WeakEntity, Window,
};

use crate::app::App as PhotrezApp;

pub(crate) struct CanvasPointerSurface {
    surface: AnyElement,
    app: WeakEntity<PhotrezApp>,
    bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    capture: Rc<Cell<Option<HitboxId>>>,
    doc_size: (u32, u32),
}

impl CanvasPointerSurface {
    pub(crate) fn new(
        surface: impl IntoElement,
        app: WeakEntity<PhotrezApp>,
        bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
        capture: Rc<Cell<Option<HitboxId>>>,
        doc_size: (u32, u32),
    ) -> Self {
        Self {
            surface: surface.into_any_element(),
            app,
            bounds,
            capture,
            doc_size,
        }
    }
}

impl IntoElement for CanvasPointerSurface {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for CanvasPointerSurface {
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
        (self.surface.request_layout(window, cx), ())
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
        self.surface.prepaint(window, cx);
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
        let down_bounds = Rc::clone(&self.bounds);
        let down_capture = Rc::clone(&self.capture);
        let doc_size = self.doc_size;
        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
            if phase != DispatchPhase::Bubble || !down_hitbox.contains(&event.position) {
                return;
            }
            if let Some(current) = window.captured_hitbox() {
                // A stale capture from a cancelled canvas gesture (Escape/tool
                // switch) is ours to replace; any other owner refuses the down.
                if down_capture.get() != Some(current) {
                    return;
                }
                window.release_pointer();
            }
            window.capture_pointer(down_hitbox.id);
            down_capture.set(Some(down_hitbox.id));
            if let Some(app) = down_app.upgrade() {
                app.update(cx, |app, cx| {
                    app.canvas_pointer_down(event, &down_bounds, doc_size, cx);
                });
            }
        });

        let hover_app = self.app.clone();
        let hover_bounds = Rc::clone(&self.bounds);
        let hover_hitbox = hitbox.clone();
        let hover_doc = self.doc_size;
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, _window, cx| {
            if phase != DispatchPhase::Bubble || !hover_hitbox.contains(&event.position) {
                return;
            }
            if let Some(app) = hover_app.upgrade() {
                app.update(cx, |app, cx| {
                    app.canvas_hover(event, &hover_bounds, hover_doc, cx);
                });
            }
        });

        let move_app = self.app.clone();
        let move_bounds = Rc::clone(&self.bounds);
        let move_capture = Rc::clone(&self.capture);
        let move_doc = self.doc_size;
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
            if phase != DispatchPhase::Bubble || !owns_capture(window, &move_capture) {
                return;
            }
            if let Some(app) = move_app.upgrade() {
                app.update(cx, |app, cx| {
                    app.canvas_pointer_move(event, &move_bounds, move_doc, cx);
                });
            }
        });

        let up_app = self.app.clone();
        let up_bounds = Rc::clone(&self.bounds);
        let up_capture = Rc::clone(&self.capture);
        let up_doc = self.doc_size;
        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
            if phase != DispatchPhase::Bubble || !owns_capture(window, &up_capture) {
                return;
            }
            up_capture.set(None);
            if let Some(app) = up_app.upgrade() {
                app.update(cx, |app, cx| {
                    app.canvas_pointer_up(event, &up_bounds, up_doc, cx);
                });
            }
        });

        let drop_app = self.app.clone();
        window.on_mouse_event(move |event: &FileDropEvent, phase, _window, cx| {
            if phase != DispatchPhase::Bubble {
                return;
            }
            let Some(app) = drop_app.upgrade() else {
                return;
            };
            match event {
                FileDropEvent::Entered { paths, .. } => app.update(cx, |app, _| {
                    app.stage_dropped_paths(paths.paths().to_vec());
                }),
                FileDropEvent::Submit { .. } => app.update(cx, |app, cx| {
                    app.submit_dropped_paths(cx);
                }),
                FileDropEvent::Exited | FileDropEvent::Ended => app.update(cx, |app, _| {
                    app.clear_dropped_paths();
                }),
                FileDropEvent::Pending { .. } => {}
            }
        });

        self.surface.paint(window, cx);
    }
}

fn owns_capture(window: &Window, capture: &Cell<Option<HitboxId>>) -> bool {
    let Some(captured) = capture.get() else {
        return false;
    };
    window.captured_hitbox() == Some(captured)
}
