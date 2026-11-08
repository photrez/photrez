//! The canvas viewport: camera actions, wheel navigation and space/middle pan.
//!
//! The camera is a derived view of the active document, never document truth:
//! the page placement it produces is the same one the display seam paints and
//! the pointer mapping reads. Camera changes persist into the session's `.ptz`
//! viewport envelope, which is a session field and does not dirty the document.

use gpui_kit::{
    Context, KeyDownEvent, KeyUpEvent, MouseDownEvent, MouseMoveEvent, Pixels, Point, ScrollDelta,
    ScrollEvent,
};

use crate::display::canvas_bridge::Camera;

use super::App;

const FIT_PADDING: f32 = 80.;
const ZOOM_STEP: f32 = 1.25;
const WHEEL_LINE_PX: f32 = 20.;

/// The Navigator zoom row's slider range. The camera itself allows a wider
/// wheel range; the slider covers the useful editing band and the +/- buttons
/// stop at the same edges.
pub(crate) const NAV_ZOOM_MIN: f32 = 0.05;
pub(crate) const NAV_ZOOM_MAX: f32 = 4.0;

#[derive(Clone, PartialEq, Debug, Default, gpui_kit::Action)]
#[action(namespace = photrez, no_json)]
pub(crate) struct ZoomInAction;

#[derive(Clone, PartialEq, Debug, Default, gpui_kit::Action)]
#[action(namespace = photrez, no_json)]
pub(crate) struct ZoomOutAction;

#[derive(Clone, PartialEq, Debug, Default, gpui_kit::Action)]
#[action(namespace = photrez, no_json)]
pub(crate) struct FitToScreenAction;

#[derive(Clone, PartialEq, Debug, Default, gpui_kit::Action)]
#[action(namespace = photrez, no_json)]
pub(crate) struct ActualSizeAction;

#[derive(Clone, PartialEq, Debug, Default, gpui_kit::Action)]
#[action(namespace = photrez, no_json)]
pub(crate) struct ZoomToSelectionAction;

#[derive(Clone, Copy, Debug)]
pub(crate) struct PanDrag {
    start: Point<Pixels>,
    camera: Camera,
}

impl App {
    pub(crate) fn active_camera(&self) -> Option<Camera> {
        if let Some(camera) = self
            .active_document
            .as_ref()
            .and_then(|document| document.camera)
        {
            return Some(camera);
        }
        self.capture_camera()
    }

    /// The camera for a launch that named its zoom, and optionally its page
    /// origin.
    ///
    /// A `--shot` launch has no document session, so the display live-fits and
    /// the page scale changes with whatever chrome is hidden: 87 % with the
    /// dock open, 98 % with it closed. `--zoom` pins the scale instead, so one
    /// shot reads the same size under either layout.
    ///
    /// `--pan` places the page's top-left in element-local pixels instead of
    /// centring it, which is the only way to ask for a page position that is
    /// not the centred default.
    pub(crate) fn capture_camera(&self) -> Option<Camera> {
        let zoom = crate::shot::zoom()?;
        let (doc_w, doc_h) = self
            .active_document
            .as_ref()
            .map(|document| document.canvas.dimensions())
            .or_else(|| self.demo_canvas.as_ref().map(|canvas| canvas.dimensions()))?;
        let (viewport_w, viewport_h) = self.viewport_size()?;
        if let Some((x, y)) = crate::shot::pan() {
            return Camera::new(x, y, zoom);
        }
        Camera::centred_at(viewport_w, viewport_h, doc_w as f32, doc_h as f32, zoom)
    }

    pub(crate) fn viewport_size(&self) -> Option<(f32, f32)> {
        let bounds = self.canvas_bounds.get()?;
        Some((bounds.size.width.into(), bounds.size.height.into()))
    }

    fn document_size(&self) -> Option<(f32, f32)> {
        let canvas = self.active_document.as_ref()?.canvas.dimensions();
        Some((canvas.0 as f32, canvas.1 as f32))
    }

    /// The concrete camera the display currently uses: the saved camera when
    /// the user has taken over the view, otherwise the live fit.
    pub(crate) fn effective_camera(&self) -> Option<Camera> {
        if let Some(camera) = self.active_camera() {
            return Some(camera);
        }
        let (viewport_w, viewport_h) = self.viewport_size()?;
        let (doc_w, doc_h) = self.document_size()?;
        Camera::fit(viewport_w, viewport_h, doc_w, doc_h)
    }

    pub(crate) fn set_camera(&mut self, camera: Camera, cx: &mut Context<Self>) {
        let Some(document) = self.active_document.as_mut() else {
            return;
        };
        document.camera = Some(camera);
        self.persist_camera(camera);
        cx.notify();
    }

    fn persist_camera(&mut self, camera: Camera) {
        let Some(doc_id) = self
            .active_document
            .as_ref()
            .map(|document| document.doc_id.clone())
        else {
            return;
        };
        if let Some(session) = self.sessions.get_mut(&doc_id) {
            session.loaded_document_mut().set_viewport(
                camera.x as f64,
                camera.y as f64,
                camera.zoom as f64,
            );
        }
    }

    /// Fire a zoom action around the viewport centre, the anchor for
    /// zoom in and out.
    fn zoom_around_centre(&mut self, factor: f32, cx: &mut Context<Self>) {
        let Some((viewport_w, viewport_h)) = self.viewport_size() else {
            return;
        };
        let Some(mut camera) = self.effective_camera() else {
            return;
        };
        camera.zoom_to_point(factor, viewport_w / 2., viewport_h / 2.);
        self.set_camera(camera, cx);
    }

    pub(crate) fn zoom_in(&mut self, cx: &mut Context<Self>) {
        self.zoom_around_centre(ZOOM_STEP, cx);
    }

    pub(crate) fn zoom_out(&mut self, cx: &mut Context<Self>) {
        self.zoom_around_centre(1. / ZOOM_STEP, cx);
    }

    pub(crate) fn fit_to_screen(&mut self, cx: &mut Context<Self>) {
        let (Some((viewport_w, viewport_h)), Some((doc_w, doc_h))) =
            (self.viewport_size(), self.document_size())
        else {
            return;
        };
        if let Some(camera) = Camera::fit(viewport_w, viewport_h, doc_w, doc_h) {
            self.set_camera(camera, cx);
        }
    }

    /// Set the zoom outright, keeping the viewport's centre point anchored.
    /// The Navigator's track and +/- buttons drive this.
    pub(crate) fn set_zoom(&mut self, zoom: f32, cx: &mut Context<Self>) {
        let Some(mut camera) = self.effective_camera() else {
            return;
        };
        let Some((viewport_w, viewport_h)) = self.viewport_size() else {
            return;
        };
        let target = zoom.clamp(NAV_ZOOM_MIN, NAV_ZOOM_MAX);
        let factor = target / camera.zoom;
        camera.zoom_to_point(factor, viewport_w / 2., viewport_h / 2.);
        self.set_camera(camera, cx);
    }

    /// Map a pointer x across the Navigator's laid-out zoom track onto the
    /// slider range and apply it. The fraction matches the shared slider
    /// visual, whose thumb centre rides `fraction * width`.
    pub(crate) fn set_zoom_from_x(&mut self, x: f32, cx: &mut Context<Self>) {
        let Some(bounds) = self.state.read(cx).nav_zoom_track_bounds.get() else {
            return;
        };
        let width = f32::from(bounds.size.width).max(1.);
        let fraction = ((x - f32::from(bounds.origin.x)) / width).clamp(0., 1.);
        let zoom = NAV_ZOOM_MIN + fraction * (NAV_ZOOM_MAX - NAV_ZOOM_MIN);
        self.set_zoom(zoom, cx);
    }

    pub(crate) fn actual_size(&mut self, cx: &mut Context<Self>) {
        let Some(mut camera) = self.effective_camera() else {
            return;
        };
        let factor = 1. / camera.zoom;
        let Some((viewport_w, viewport_h)) = self.viewport_size() else {
            return;
        };
        camera.zoom_to_point(factor, viewport_w / 2., viewport_h / 2.);
        self.set_camera(camera, cx);
    }

    pub(crate) fn zoom_to_selection(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        let (Some((viewport_w, viewport_h)), Some((doc_w, doc_h))) =
            (self.viewport_size(), self.document_size())
        else {
            return Err("the canvas has no viewport yet".to_string());
        };
        let selection = self
            .selection_bounds_for_viewport()
            .unwrap_or((0., 0., doc_w, doc_h));
        let left = selection.0.clamp(0., doc_w);
        let top = selection.1.clamp(0., doc_h);
        let right = (selection.0 + selection.2).clamp(0., doc_w);
        let bottom = (selection.1 + selection.3).clamp(0., doc_h);
        let width = (right - left).max(1.);
        let height = (bottom - top).max(1.);
        let zoom = ((viewport_w - FIT_PADDING) / width)
            .min((viewport_h - FIT_PADDING) / height)
            .clamp(0.05, 10.);
        let camera = Camera::new(
            (viewport_w - width * zoom) / 2. - left * zoom,
            (viewport_h - height * zoom) / 2. - top * zoom,
            zoom,
        )
        .ok_or_else(|| "the selection produces an invalid camera".to_string())?;
        self.set_camera(camera, cx);
        Ok(())
    }

    fn selection_bounds_for_viewport(&self) -> Option<(f32, f32, f32, f32)> {
        let doc_id = self.active_document.as_ref()?.doc_id.clone();
        let content = self
            .sessions
            .get(&doc_id)?
            .loaded_document()
            .content_snapshot()?;
        let selection = content.selection.as_ref()?;
        Some((
            selection.x as f32,
            selection.y as f32,
            selection.width as f32,
            selection.height as f32,
        ))
    }

    /// Centre the camera on the document point under a Navigator click.
    pub(crate) fn navigator_pan_to(
        &mut self,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(nav) = self.navigator_bounds.get() else {
            return false;
        };
        let (Some((doc_w, doc_h)), Some((viewport_w, viewport_h))) =
            (self.document_size(), self.viewport_size())
        else {
            return false;
        };
        let Some(layout) = crate::display::canvas_bridge::navigator_layout(doc_w, doc_h) else {
            return false;
        };
        let local_x = f32::from(position.x)
            - f32::from(nav.origin.x)
            - crate::display::canvas_bridge::NAV_PAD;
        let local_y = f32::from(position.y)
            - f32::from(nav.origin.y)
            - crate::display::canvas_bridge::NAV_PAD;
        let x = (local_x - layout.offset_x) / layout.scale;
        let y = (local_y - layout.offset_y) / layout.scale;
        if x < 0. || y < 0. || x > doc_w || y > doc_h {
            return false;
        }
        let Some(mut camera) = self.effective_camera() else {
            return false;
        };
        camera.center_on(x, y, viewport_w, viewport_h);
        self.set_camera(camera, cx);
        true
    }

    pub(crate) fn zoom_in_action(&mut self, _: &ZoomInAction, cx: &mut Context<Self>) {
        self.zoom_in(cx);
    }

    pub(crate) fn zoom_out_action(&mut self, _: &ZoomOutAction, cx: &mut Context<Self>) {
        self.zoom_out(cx);
    }

    pub(crate) fn fit_to_screen_action(&mut self, _: &FitToScreenAction, cx: &mut Context<Self>) {
        self.fit_to_screen(cx);
    }

    pub(crate) fn actual_size_action(&mut self, _: &ActualSizeAction, cx: &mut Context<Self>) {
        self.actual_size(cx);
    }

    pub(crate) fn zoom_to_selection_action(
        &mut self,
        _: &ZoomToSelectionAction,
        cx: &mut Context<Self>,
    ) {
        let result = self.zoom_to_selection(cx);
        match result {
            Ok(()) => self.set_status_error(None, cx),
            Err(error) => self.set_status_error(Some(format!("View: {error}")), cx),
        }
    }

    /// Wheel over the canvas: Ctrl/Alt zooms at the cursor, plain wheel pans
    /// and Shift pans horizontally.
    pub(crate) fn canvas_scroll(&mut self, event: &ScrollEvent, cx: &mut Context<Self>) {
        // A modal dialog owns the input: the wheel behind it neither zooms
        // nor pans.
        if self.modal_input_active(cx) {
            return;
        }
        let Some(bounds) = self.canvas_bounds.get() else {
            return;
        };
        let (delta_x, delta_y) = match event.delta {
            ScrollDelta::Pixels(delta) => (f32::from(delta.x), f32::from(delta.y)),
            ScrollDelta::Lines(delta) => (delta.x * WHEEL_LINE_PX, delta.y * WHEEL_LINE_PX),
        };
        let Some(mut camera) = self.effective_camera() else {
            return;
        };
        if event.modifiers.control || event.modifiers.alt {
            let factor = if delta_y < 0. {
                ZOOM_STEP
            } else {
                1. / ZOOM_STEP
            };
            let local_x = f32::from(event.position.x) - f32::from(bounds.origin.x);
            let local_y = f32::from(event.position.y) - f32::from(bounds.origin.y);
            camera.zoom_to_point(factor, local_x, local_y);
        } else {
            let (pan_x, pan_y) = if event.modifiers.shift {
                (-delta_y, 0.)
            } else {
                (-delta_x, -delta_y)
            };
            camera.pan(pan_x, pan_y);
        }
        self.set_camera(camera, cx);
    }

    pub(crate) fn canvas_space_down(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        if self.state.read(cx).text_edit.is_some() && self.text_edit_key(event, cx) {
            return;
        }
        if self.state.read(cx).crop_edit.is_some() && self.crop_edit_key(event, cx) {
            return;
        }
        if self.state.read(cx).open_select.is_some() {
            match event.keystroke.key.as_str() {
                "down" => {
                    self.select_arrow(1, cx);
                    return;
                }
                "up" => {
                    self.select_arrow(-1, cx);
                    return;
                }
                "enter" => {
                    self.select_confirm(cx);
                    return;
                }
                "escape" => {
                    self.close_select(cx);
                    return;
                }
                _ => {}
            }
        }
        if event.keystroke.key == "space" && !self.space_pressed {
            self.space_pressed = true;
            cx.notify();
        }
    }

    pub(crate) fn canvas_space_up(&mut self, event: &KeyUpEvent, cx: &mut Context<Self>) {
        if event.keystroke.key == "space" && self.space_pressed {
            self.space_pressed = false;
            cx.notify();
        }
    }

    /// Start a pan when Space is held or the middle button went down.
    pub(crate) fn begin_pan(&mut self, event: &MouseDownEvent, cx: &mut Context<Self>) -> bool {
        let middle = event.button == gpui_kit::MouseButton::Middle;
        if !middle && !self.space_pressed {
            return false;
        }
        let Some(camera) = self.effective_camera() else {
            return false;
        };
        self.pan = Some(PanDrag {
            start: event.position,
            camera,
        });
        cx.notify();
        true
    }

    pub(crate) fn update_pan(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) -> bool {
        let Some(drag) = self.pan else {
            return false;
        };
        let mut camera = drag.camera;
        camera.pan(
            f32::from(event.position.x) - f32::from(drag.start.x),
            f32::from(event.position.y) - f32::from(drag.start.y),
        );
        self.set_camera(camera, cx);
        true
    }

    pub(crate) fn end_pan(&mut self, cx: &mut Context<Self>) -> bool {
        if self.pan.take().is_some() {
            cx.notify();
            true
        } else {
            false
        }
    }

    /// Release latched navigation when the window loses input focus.
    pub(crate) fn release_navigation(&mut self, cx: &mut Context<Self>) {
        self.stop_edge_scroll();
        if self.pan.take().is_some() || self.space_pressed {
            self.space_pressed = false;
            cx.notify();
        }
    }

    #[cfg(test)]
    pub(crate) fn camera_for_test(&self) -> Option<Camera> {
        self.active_camera()
    }

    #[cfg(test)]
    pub(crate) fn session_viewport_for_test(&self) -> Option<(f64, f64, f64)> {
        let doc_id = self.active_document.as_ref()?.doc_id.clone();
        let viewport = &self
            .sessions
            .get(&doc_id)?
            .loaded_document()
            .document_snapshot()
            .viewport;
        Some((viewport.pan_x, viewport.pan_y, viewport.zoom))
    }
}
