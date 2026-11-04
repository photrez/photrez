use super::*;

#[derive(Clone, Copy, Debug)]
pub(crate) struct NavigatorLayout {
    pub scale: f32,
    pub offset_x: f32,
    pub offset_y: f32,
    pub width: f32,
    pub height: f32,
}

/// The Navigator's 208x88 thumbnail box, centred in
/// that box regardless of the document aspect.
pub(crate) fn navigator_layout(doc_w: f32, doc_h: f32) -> Option<NavigatorLayout> {
    if !doc_w.is_finite() || !doc_h.is_finite() || doc_w <= 0. || doc_h <= 0. {
        return None;
    }
    const BOX_W: f32 = 208.;
    const BOX_H: f32 = 88.;
    let scale = (BOX_W / doc_w).min(BOX_H / doc_h);
    let width = doc_w * scale;
    let height = doc_h * scale;
    Some(NavigatorLayout {
        scale,
        offset_x: (BOX_W - width) / 2.,
        offset_y: (BOX_H - height) / 2.,
        width,
        height,
    })
}

/// The Navigator pane: the composited page scaled into its 208x88 box plus the
/// accent viewport frame. `None` for `canvas_image` draws just the panel.
pub(crate) fn navigator_element(
    canvas_image: Option<&CanvasImage>,
    camera: Option<Camera>,
    main_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    navigator_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
) -> impl IntoElement {
    let images: Vec<(TileRect, Arc<RenderImage>)> = canvas_image
        .map(|canvas| {
            canvas
                .tiles
                .iter()
                .map(|tile| (tile.rect, Arc::clone(&tile.image)))
                .collect()
        })
        .unwrap_or_default();
    let doc_size = canvas_image.map(|canvas| (canvas.width as f32, canvas.height as f32));
    canvas(
        move |bounds, _window, _cx| {
            navigator_bounds.set(Some(bounds));
            bounds
        },
        move |bounds, _paint_bounds, window, _cx| {
            let Some((doc_w, doc_h)) = doc_size else {
                return;
            };
            let Some(layout) = navigator_layout(doc_w, doc_h) else {
                return;
            };
            let origin_x = f32::from(bounds.origin.x) + NAV_PAD;
            let origin_y = f32::from(bounds.origin.y) + NAV_PAD;
            let thumb_x = origin_x + layout.offset_x;
            let thumb_y = origin_y + layout.offset_y;
            for (rect, image) in &images {
                let Some((left, top, tile_w, tile_h)) = tile_destination(
                    thumb_x,
                    thumb_y,
                    layout.width,
                    layout.height,
                    doc_w,
                    doc_h,
                    *rect,
                    1.,
                ) else {
                    continue;
                };
                let tile_bounds = Bounds {
                    origin: point(px(left), px(top)),
                    size: size(px(tile_w), px(tile_h)),
                };
                let _ = window.paint_image(
                    bounds,
                    tile_bounds,
                    Corners::default(),
                    Arc::clone(image),
                    0,
                    false,
                );
            }
            let Some(main) = main_bounds.get() else {
                return;
            };
            let viewport_w: f32 = main.size.width.into();
            let viewport_h: f32 = main.size.height.into();
            let Some(camera) = camera.or_else(|| Camera::fit(viewport_w, viewport_h, doc_w, doc_h))
            else {
                return;
            };
            let doc_left = (-camera.x / camera.zoom).max(0.);
            let doc_top = (-camera.y / camera.zoom).max(0.);
            let doc_right = ((viewport_w - camera.x) / camera.zoom).min(doc_w);
            let doc_bottom = ((viewport_h - camera.y) / camera.zoom).min(doc_h);
            if doc_right <= doc_left || doc_bottom <= doc_top {
                return;
            }
            let frame = Bounds {
                origin: point(
                    px(thumb_x + doc_left * layout.scale),
                    px(thumb_y + doc_top * layout.scale),
                ),
                size: size(
                    px((doc_right - doc_left) * layout.scale),
                    px((doc_bottom - doc_top) * layout.scale),
                ),
            };
            window.paint_quad(fill(frame, hsla(0., 0., 1., 0.08)));
            window.paint_quad(outline(frame, color::ACCENT, BorderStyle::Solid));
        },
    )
    .w_full()
    .h(px(NAV_H - NAV_HEADER_H - 1.0))
    .bg(color::CANVAS)
}

/// 16px padding around the Navigator canvas.
pub(crate) const NAV_PAD: f32 = 16.;
const NAV_H: f32 = 185.;
const NAV_HEADER_H: f32 = 38.;

/// Transient canvas chrome painted over the page: the committed/live pixel
/// selection and the crop frame. Coordinates are document space; the paint
/// pass maps them through the same page placement as the tiles.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct CanvasOverlay {
    /// The committed or live pixel selection, painted as
    /// double-stroke marching ants (the ellipse shape draws a dashed ellipse).
    pub selection: Option<SelectionOverlay>,
    /// The rotate-cursor sprite following the pointer during selection
    /// rotation.
    pub rotate_cursor: Option<RotateCursorOverlay>,
    /// Crop frame `(x, y, width, height)`.
    pub crop: Option<(f64, f64, f64, f64)>,
    /// Move-tool transform frame: four visual corners and eight handle points
    /// in document space.
    pub transform: Option<TransformFrame>,
    /// Mode B (interactive handles) when true; Mode A (corner brackets) when
    /// false. Only consulted while `transform` is present.
    pub transform_controls: bool,
    /// Smart-guide lines: `(vertical, document coordinate)`.
    pub guides: Vec<(bool, f64)>,
    /// Move-tool layer marquee `(x, y, width, height)` in document space.
    pub move_marquee: Option<(f64, f64, f64, f64)>,
    /// Move-tool hover outline `(x, y, width, height)` for a non-active layer.
    pub hover_highlight: Option<(f64, f64, f64, f64)>,
    /// Brush/eraser ring painted at the pointer (screen-visible cursor).
    pub brush_cursor: Option<BrushCursor>,
    /// Live gradient drag line while the Gradient tool is drawing.
    pub gradient_line: Option<GradientLine>,
}

/// The gradient line and endpoint handles shown while a
/// gradient drag is in progress.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct GradientLine {
    pub start: [f64; 2],
    pub end: [f64; 2],
    pub radial: bool,
}

/// The brush cursor ring: it marks the 20% alpha contour of the
/// active brush profile and follows the active layer's scale and rotation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BrushCursor {
    /// Ring centre in document space.
    pub center: [f64; 2],
    /// Ring radii in document units after the layer transform.
    pub radii: [f64; 2],
    /// Layer rotation in radians.
    pub rotation: f64,
    /// The eraser paints no centre crosshair.
    pub crosshair: bool,
}

/// One pixel selection as the display paints it, in document space. `angle`
/// rotates the rectangle about its centre (core `SelectionState.angle`);
/// `edit_mode` paints the transform chrome: the eight resize
/// handles, the rotation connector and the rotate pin.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SelectionOverlay {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub ellipse: bool,
    pub angle: f64,
    pub edit_mode: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TransformFrame {
    pub corners: [[f64; 2]; 4],
    pub handles: [[f64; 2]; 8],
}

/// The rotate-cursor sprite painted at the pointer: the
/// 32-unit white/black arrow glyph rotated by `angle` degrees. The
/// system cursor stays visible beside it until the owner rules on cursor
/// hiding (gpui exposes no hide API), so this is a disclosed difference.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RotateCursorOverlay {
    /// Window-space pointer position.
    pub position: [f32; 2],
    /// Sprite angle in degrees (pointer angle about the rotation centre
    /// plus the source's 45-degree base tilt).
    pub angle: f32,
}
