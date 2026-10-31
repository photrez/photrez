//! Records the open dropdown panel's window bounds so the canvas hit-test can
//! tell an inside-panel click (row selection) from a click-away dismissal.
//! GPUI's window-level mouse listeners do not expose the occluding hitbox, so
//! the panel publishes its rect instead.

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::{
    App, Bounds, Element, ElementId, GlobalElementId, IntoElement, LayoutId, Pixels, Window,
};

pub(crate) struct SelectPanelSurface {
    visual: gpui_kit::AnyElement,
    bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
}

impl SelectPanelSurface {
    pub(crate) fn new(visual: impl IntoElement, bounds: Rc<Cell<Option<Bounds<Pixels>>>>) -> Self {
        Self {
            visual: visual.into_any_element(),
            bounds,
        }
    }
}

impl IntoElement for SelectPanelSurface {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for SelectPanelSurface {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui_kit::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        (self.visual.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui_kit::InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.visual.prepaint(window, cx);
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui_kit::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        _: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.bounds.set(Some(bounds));
        self.visual.paint(window, cx);
    }
}
