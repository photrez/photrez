use super::*;

#[derive(Clone, Copy, Debug)]
pub(crate) struct NavigatorLayout {
    pub scale: f32,
    pub offset_x: f32,
    pub offset_y: f32,
    pub width: f32,
    pub height: f32,
}

/// The Navigator's thumbnail box, centred in
/// that box regardless of the document aspect.
pub(crate) fn navigator_layout(doc_w: f32, doc_h: f32) -> Option<NavigatorLayout> {
    if !doc_w.is_finite() || !doc_h.is_finite() || doc_w <= 0. || doc_h <= 0. {
        return None;
    }
    let scale = (NAV_BOX_W / doc_w).min(NAV_BOX_H / doc_h);
    let width = doc_w * scale;
    let height = doc_h * scale;
    Some(NavigatorLayout {
        scale,
        offset_x: (NAV_BOX_W - width) / 2.,
        offset_y: (NAV_BOX_H - height) / 2.,
        width,
        height,
    })
}

/// A rectangle in preview-box coordinates: `(x, y, width, height)`.
type NavRect = (f32, f32, f32, f32);

/// The viewport rectangle in thumbnail pixels.
///
/// It is deliberately not clamped to the document. At a zoom that shows more
/// than the page, the frame is wider and taller than the thumbnail, and the
/// preview cuts it. Clamping to the page collapses the indicator onto the page
/// border, which reads as a frame around the artboard instead of a viewport.
pub(crate) fn navigator_viewport_frame(
    camera: Camera,
    viewport: (f32, f32),
    layout: &NavigatorLayout,
) -> Option<NavRect> {
    if !(camera.zoom.is_finite()) || camera.zoom <= 0. {
        return None;
    }
    let width = viewport.0 / camera.zoom * layout.scale;
    let height = viewport.1 / camera.zoom * layout.scale;
    if !(width.is_finite()) || !(height.is_finite()) || width <= 0. || height <= 0. {
        return None;
    }
    Some((
        layout.offset_x + -camera.x / camera.zoom * layout.scale,
        layout.offset_y + -camera.y / camera.zoom * layout.scale,
        width,
        height,
    ))
}

/// What the indicator actually paints inside a preview box: the soft fill over
/// the part that survives the cut, then each edge only where it was not cut.
///
/// The reference clips with `overflow: hidden`, so an edge that falls outside
/// the preview disappears. Redrawing it along the clip line would put two
/// horizontal rules across the thumbnail that the reference does not have.
/// Rectangles are `(x, y, width, height)`; edges come back 1 px thick.
pub(crate) fn navigator_indicator_paint(
    frame: NavRect,
    box_rect: NavRect,
) -> (Option<NavRect>, Vec<NavRect>) {
    let (fx, fy, fw, fh) = frame;
    let (bx, by, bw, bh) = box_rect;
    let left = fx.max(bx);
    let top = fy.max(by);
    let right = (fx + fw).min(bx + bw);
    let bottom = (fy + fh).min(by + bh);
    if right <= left || bottom <= top {
        return (None, Vec::new());
    }
    let fill = Some((left, top, right - left, bottom - top));
    let mut edges = Vec::new();
    if fx >= bx && fx + 1. <= right {
        edges.push((fx, top, 1., bottom - top));
    }
    let fr = fx + fw;
    if fr > left && fr <= bx + bw {
        edges.push((fr - 1., top, 1., bottom - top));
    }
    if fy >= by && fy + 1. <= bottom {
        edges.push((left, fy, right - left, 1.));
    }
    let fb = fy + fh;
    if fb > top && fb <= by + bh {
        edges.push((left, fb - 1., right - left, 1.));
    }
    (fill, edges)
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
            // The reference centres its 208px canvas with `px-4` plus a flex
            // row, and 16 + (w - 32 - 208)/2 is the same place as (w - 208)/2,
            // so the box is centred horizontally. Vertically it only pads the
            // top, so the 16px stays there.
            let origin_x =
                f32::from(bounds.origin.x) + (f32::from(bounds.size.width) - NAV_BOX_W) / 2.;
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
            let Some(frame) = navigator_viewport_frame(camera, (viewport_w, viewport_h), &layout)
            else {
                return;
            };
            let (soft, edges) = navigator_indicator_paint(frame, (0., 0., NAV_BOX_W, NAV_BOX_H));
            let place = |rect: NavRect| Bounds {
                origin: point(px(origin_x + rect.0), px(origin_y + rect.1)),
                size: size(px(rect.2), px(rect.3)),
            };
            if let Some(soft) = soft {
                window.paint_quad(fill(place(soft), color::TOOL_MARK_08));
            }
            for edge in edges {
                window.paint_quad(fill(place(edge), color::TOOL_MARK));
            }
        },
    )
    .w_full()
    .h(px(NAV_H - NAV_HEADER_H - 1.0))
    // The reference's navigator body is the panel tone (30,30,30 in
    // `shell-navigator.png`), not the canvas tone; the dock shows no canvas
    // surface here.
    .bg(color::PANEL)
}

/// 16px padding around the Navigator canvas.
pub(crate) const NAV_PAD: f32 = 16.;
/// The thumbnail box inside that padding. The reference clips its navigator
/// canvas to this rect, and the viewport indicator is cut by it rather than by
/// the document.
pub(crate) const NAV_BOX_W: f32 = 208.;
pub(crate) const NAV_BOX_H: f32 = 88.;
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
