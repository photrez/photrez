//! Paint-time OS file-drop surface for the empty workspace.
//!
//! `Window::on_mouse_event` is paint-only, and the canvas surface — which
//! normally registers the file-drop handler — is not mounted when no document
//! is open. This element wraps the empty card and registers the same handler so
//! dropped images still open as documents.

use gpui_kit::{
    AnyElement, App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId, IntoElement,
    LayoutId, Pixels, WeakEntity, Window,
};

use crate::app::App as PhotrezApp;
use crate::views::canvas_surface::register_file_drop;

pub(crate) struct FileDropSurface {
    surface: AnyElement,
    app: WeakEntity<PhotrezApp>,
}

impl FileDropSurface {
    pub(crate) fn new(surface: impl IntoElement, app: WeakEntity<PhotrezApp>) -> Self {
        Self {
            surface: surface.into_any_element(),
            app,
        }
    }
}

impl IntoElement for FileDropSurface {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for FileDropSurface {
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
        _: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.surface.prepaint(window, cx);
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        _: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.surface.paint(window, cx);
        register_file_drop(window, self.app.clone());
    }
}
