//! Capture surface for an option-bar slider. A plain `div` receives
//! `on_mouse_move`/`on_mouse_up` only while its hitbox is hovered, so a drag
//! released outside the track would be lost. This element inserts a hitbox,
//! captures it on left-down, and routes move/up through the capture.
//!
//! The file also holds `track_focus_and_keys`, because a track's pointer host and
//! its keyboard host have to agree about the same handle or one of them focuses a
//! node the other never renders.

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::{
    point, px, size, AnyElement, App, Bounds, Context, DispatchPhase, Element, ElementId,
    FocusHandle, GlobalElementId, Hitbox, HitboxBehavior, HitboxId, InspectorElementId,
    IntoElement, KeyDownEvent, LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    Pixels, WeakEntity, Window,
};

use crate::components::ui;
use gpui_kit::prelude::*;

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
        // The painted track is 4px tall, but the thumb is 12px and overhangs
        // it by 4px on each side (and half its width past the ends). A
        // native slider makes that whole visible control clickable, so the
        // hitbox grows by half the thumb on every side while the x mapping
        // keeps using the laid-out track bounds.
        let hit_bounds = Bounds::new(
            point(bounds.origin.x - px(6.0), bounds.origin.y - px(6.0)),
            size(bounds.size.width + px(12.0), bounds.size.height + px(12.0)),
        );
        window.insert_hitbox(hit_bounds, HitboxBehavior::Normal)
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
                    app.slider_pointer_down_for(
                        down_target,
                        f32::from(event.position.x),
                        window,
                        cx,
                    );
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
                    app.slider_pointer_move_for(
                        move_target,
                        f32::from(event.position.x),
                        window,
                        cx,
                    );
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
                app.update(cx, |app, cx| app.slider_pointer_up(cx));
            }
        });

        self.visual.paint(window, cx);
    }
}

/// Give a track its focus and its keys: the inset band the option-bar pills use,
/// and the arrow handler every `SliderTarget` shares. A host with no handle (an
/// engine-less `--shot` launch, where there is no App to talk to) keeps the bare
/// track, so the painted element does not change on that path.
///
/// Generic because a host's `div().id(..)` is not one named type: outside the
/// `test-support` feature it is `Stateful<Div>`, inside it is the observed wrapper
/// around that, and a track has to compile both ways.
pub(crate) fn track_focus_and_keys<E>(
    track: E,
    focus: &Option<FocusHandle>,
    target: crate::state::SliderTarget,
    cx: &Context<crate::app::App>,
) -> E
where
    E: InteractiveElement + StatefulInteractiveElement,
{
    let Some(handle) = focus.as_ref() else {
        return track;
    };
    ui::tabbable_focus(track, handle).on_key_down(cx.listener(
        move |app, event: &KeyDownEvent, _window, cx| {
            app.slider_key(target, event, cx);
        },
    ))
}

fn owns_capture(window: &Window, capture: &Cell<Option<HitboxId>>) -> bool {
    let Some(captured) = capture.get() else {
        return false;
    };
    window.captured_hitbox() == Some(captured)
}
