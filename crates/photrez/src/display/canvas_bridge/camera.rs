/// Total fit padding across both axes (the viewport's own constant).
const FIT_PADDING: f32 = 80.;
/// The fit zoom clamp, `[0.05, 10]`.
const FIT_ZOOM_MIN: f32 = 0.05;
const FIT_ZOOM_MAX: f32 = 10.;
/// The camera's wheel-zoom clamp: `[0.01, 100]`.
pub const CAMERA_MIN_ZOOM: f32 = 0.01;
pub const CAMERA_MAX_ZOOM: f32 = 100.;

/// The viewport's fit placement for one document in one viewport:
/// `zoom = clamp(min((w - 80)/dw, (h - 80)/dh), 0.05, 10)`, centered on both
/// axes. Any non-finite or non-positive input
/// (NaN transform, zero-size viewport, negative size) returns `None` - the
/// caller draws nothing; this never panics.
pub fn fit_placement(
    viewport_w: f32,
    viewport_h: f32,
    doc_w: f32,
    doc_h: f32,
) -> Option<(f32, f32, f32, f32)> {
    if !viewport_w.is_finite()
        || !viewport_h.is_finite()
        || !doc_w.is_finite()
        || !doc_h.is_finite()
        || viewport_w <= 0.
        || viewport_h <= 0.
        || doc_w <= 0.
        || doc_h <= 0.
    {
        return None;
    }
    let zoom = ((viewport_w - FIT_PADDING) / doc_w)
        .min((viewport_h - FIT_PADDING) / doc_h)
        .clamp(FIT_ZOOM_MIN, FIT_ZOOM_MAX);
    let w = doc_w * zoom;
    let h = doc_h * zoom;
    Some(((viewport_w - w) / 2., (viewport_h - h) / 2., w, h))
}

/// The viewport camera: the page origin and zoom the display paints with.
///
/// `x`/`y` are the page's top-left corner in element-local screen pixels,
/// exactly the camera state the document keeps. The camera stays
/// concrete once the user pans or zooms; `None` at the app layer means
/// "fit to the current element bounds", which keeps the first frame correct
/// before any layout has been observed and refits on resize until the user
/// takes over the view.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub x: f32,
    pub y: f32,
    pub zoom: f32,
}

impl Camera {
    pub fn new(x: f32, y: f32, zoom: f32) -> Option<Self> {
        if !x.is_finite() || !y.is_finite() || !zoom.is_finite() || zoom <= 0. {
            return None;
        }
        Some(Self {
            x,
            y,
            zoom: zoom.clamp(CAMERA_MIN_ZOOM, CAMERA_MAX_ZOOM),
        })
    }

    /// The fit rule: padding 80, centered, clamped to `[0.05, 10]`.
    pub fn fit(viewport_w: f32, viewport_h: f32, doc_w: f32, doc_h: f32) -> Option<Self> {
        let (x, y, page_w, _) = fit_placement(viewport_w, viewport_h, doc_w, doc_h)?;
        Self::new(x, y, page_w / doc_w)
    }

    /// A stated zoom, centered on both axes.
    ///
    /// Recorded captures carry a zoom of their own, not the live fit: hiding
    /// the dock grows the canvas and a fit recomputes
    /// the scale. This is the seam that lets a fixture be drawn at the
    /// recorded scale instead.
    pub fn centred_at(
        viewport_w: f32,
        viewport_h: f32,
        doc_w: f32,
        doc_h: f32,
        zoom: f32,
    ) -> Option<Self> {
        if !viewport_w.is_finite()
            || !viewport_h.is_finite()
            || !doc_w.is_finite()
            || !doc_h.is_finite()
            || !zoom.is_finite()
            || viewport_w <= 0.
            || viewport_h <= 0.
            || doc_w <= 0.
            || doc_h <= 0.
            || zoom <= 0.
        {
            return None;
        }
        Self::new(
            (viewport_w - doc_w * zoom) / 2.,
            (viewport_h - doc_h * zoom) / 2.,
            zoom,
        )
    }

    /// Page rectangle `(x, y, width, height)` in element-local pixels.
    pub fn placement(&self, doc_w: f32, doc_h: f32) -> Option<(f32, f32, f32, f32)> {
        if !doc_w.is_finite() || !doc_h.is_finite() || doc_w <= 0. || doc_h <= 0. {
            return None;
        }
        if !self.x.is_finite() || !self.y.is_finite() || !self.zoom.is_finite() || self.zoom <= 0. {
            return None;
        }
        Some((self.x, self.y, doc_w * self.zoom, doc_h * self.zoom))
    }

    pub fn pan(&mut self, dx: f32, dy: f32) {
        if dx.is_finite() {
            self.x += dx;
        }
        if dy.is_finite() {
            self.y += dy;
        }
    }

    /// Zoom around a screen anchor: the document point under it stays fixed.
    pub fn zoom_to_point(&mut self, factor: f32, anchor_x: f32, anchor_y: f32) {
        if !factor.is_finite() || factor <= 0. || !anchor_x.is_finite() || !anchor_y.is_finite() {
            return;
        }
        let old_zoom = self.zoom;
        let new_zoom = (old_zoom * factor).clamp(CAMERA_MIN_ZOOM, CAMERA_MAX_ZOOM);
        if new_zoom == old_zoom {
            return;
        }
        self.x = anchor_x - (anchor_x - self.x) * (new_zoom / old_zoom);
        self.y = anchor_y - (anchor_y - self.y) * (new_zoom / old_zoom);
        self.zoom = new_zoom;
    }

    /// Centre the camera on a document point at the current zoom.
    pub fn center_on(&mut self, doc_x: f32, doc_y: f32, viewport_w: f32, viewport_h: f32) {
        if !doc_x.is_finite() || !doc_y.is_finite() {
            return;
        }
        self.x = viewport_w / 2. - doc_x * self.zoom;
        self.y = viewport_h / 2. - doc_y * self.zoom;
    }
}

/// The page rectangle the display paints: the camera when one is concrete,
/// otherwise the fit placement for the element bounds.
pub fn camera_placement(
    viewport_w: f32,
    viewport_h: f32,
    doc_w: f32,
    doc_h: f32,
    camera: Option<Camera>,
) -> Option<(f32, f32, f32, f32)> {
    match camera {
        Some(camera) => camera.placement(doc_w, doc_h),
        None => fit_placement(viewport_w, viewport_h, doc_w, doc_h),
    }
}
