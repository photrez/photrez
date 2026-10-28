//! The canvas display seam: converts `photrez-core` render/pixel output into
//! GPUI paint calls. Nothing else in this crate paints the canvas.
//!
//! Must not: run on a non-UI thread, mutate the document, or be bypassed -
//! this module is the only sanctioned path from core pixels to the screen.
//!
//! ## What this module owns
//!
//! - The RGBA8 -> GPUI conversion. GPUI's `RenderImage` stores **BGRA** bytes
//!   (both of GPUI's own producers swap before `Frame::new`:
//!   `elements/img.rs`, `svg_renderer.rs`), so the swap happens once, here.
//! - The page placement: the frozen viewport's fit formula, recovered from
//!   `apps/desktop/src/components/editor/canvas/useViewportRenderer.ts` and
//!   pinned against the frozen capture `tests/baseline/shell-canvas.png` (the
//!   800x600 page draws as a 701x525.75 box at x=40, y=71.125 inside the
//!   781x668 crop: `min((w-80)/dw, (h-80)/dh)` clamped to `[0.05, 10]`,
//!   centered).
//! - Refusing invalid input as defined behaviour, never a panic.
//!
//! ## Dirty updates
//!
//! The display is a fixed 512 px tile grid (core patches remain 256 px; the
//! display grid is a paint granularity, not a pixel-store contract). Each tile
//! owns an immutable `RenderImage`; `update_region` rebuilds only the tiles
//! intersecting a dirty rectangle, and unchanged tiles keep their atlas
//! entries. Every tile is painted through the same `paint_image` seam; GPUI
//! batches polychrome sprites per atlas texture.
//!
//! A live edit uses a small patch overlay instead of rebuilding a whole tile:
//! `push_patch` adds an immutable image for the exact dirty rectangle, painted
//! after the base tiles, and `commit_patches` folds the overlay into the base
//! tiles in one frame. `update_region` is the base-tile path and refuses to run
//! while patches are pending. The caller must evict every retired texture (see
//! `update_region`, `commit_patches` and `into_textures`); the app's edit path
//! still lands with its own sub-plan, so the bench is the current dirty-path
//! caller.
//!
//! ## What deliberately is not here yet
//!
//! - Pan, zoom and the animated fit: `ImageViewer` is absent from the gpui-kit
//!   build, so the viewport is ours; this landing implements only the default
//!   fit placement.
//! - The artboard chrome beyond the page (the frozen overlay's 1px border,
//!   1px outer ring and 32px soft shadow): it rides the visual close-out.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::prelude::*;
use gpui_kit::{
    canvas, fill, hsla, outline, point, px, size, BorderStyle, Bounds, Corners, PathBuilder,
    Pixels, Point, RenderImage,
};

use crate::tokens::color;

/// Product bound for one document edge - the mirror of `photrez-core`'s
/// private `MAX_CANVAS_DIM` (`crates/core/src/document.rs`), which the
/// document dialog's validation contract also enforces.
pub const MAX_CANVAS_DIM: u32 = 16384;

/// Total fit padding across both axes (the frozen viewport's own constant).
const FIT_PADDING: f32 = 80.;
/// The frozen viewport's fit zoom clamp, `[0.05, 10]`.
const FIT_ZOOM_MIN: f32 = 0.05;
const FIT_ZOOM_MAX: f32 = 10.;
/// The camera's wheel-zoom clamp (`ViewportCamera.ts`: `MIN_ZOOM`/`MAX_ZOOM`).
pub const CAMERA_MIN_ZOOM: f32 = 0.01;
pub const CAMERA_MAX_ZOOM: f32 = 100.;
/// One checkerboard cell, in screen pixels (`webgl2.ts` `u_checkSize = 8`).
pub const CHECKER_CELL: f32 = 8.;
/// Checker cells sampled from `renderer/checkerboard.ts` (0.78 and 0.55 grey).
pub const CHECKER_LIGHT: [u8; 4] = [199, 199, 199, 255];
pub const CHECKER_DARK: [u8; 4] = [140, 140, 140, 255];

/// Why a frame was refused. Refusal draws nothing; it is never a panic.
#[derive(Debug, PartialEq, Eq)]
pub enum FrameError {
    /// A zero edge has nothing to display.
    ZeroDimension,
    /// An edge is beyond [`MAX_CANVAS_DIM`].
    TooLarge { width: u32, height: u32 },
    /// The buffer length is not `width * height * 4`.
    LengthMismatch { expected: usize, got: usize },
    /// The dirty rectangle is not fully inside the document.
    RegionOutside {
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    },
    /// A base-tile update arrived while a live patch overlay still had pixels
    /// that were not committed to the base tiles.
    PendingPatches,
    /// An exact commit or base update arrived while a scaled preview overlay
    /// was still displayed.
    PendingPreviews,
    /// The requested display tile size was zero.
    ZeroTileSize,
}

/// Side length of one document display tile. Core's pixel store uses 256 px
/// patches; the display grid is deliberately 512 px because the warm 4096 run
/// showed better frame pacing/fewer sprites at 512 than at 256 (the dirty
/// upload is the patch overlay, not this tile). The tiles are document-space,
/// not screen-space, so a window resize or DPI change only changes their
/// destination rectangles.
pub const TILE_SIZE: u32 = 512;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TileRect {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

#[cfg(test)]
fn tile_rects(width: u32, height: u32) -> Vec<TileRect> {
    tile_rects_with_size(width, height, TILE_SIZE)
}

fn tile_rects_with_size(width: u32, height: u32, tile_size: u32) -> Vec<TileRect> {
    debug_assert!(tile_size > 0, "tile size is validated by the constructor");
    let cols = width.div_ceil(tile_size) as usize;
    let rows = height.div_ceil(tile_size) as usize;
    let mut rects = Vec::with_capacity(cols.saturating_mul(rows));
    let mut y = 0;
    while y < height {
        let tile_height = (height - y).min(tile_size);
        let mut x = 0;
        while x < width {
            let tile_width = (width - x).min(tile_size);
            rects.push(TileRect {
                x,
                y,
                width: tile_width,
                height: tile_height,
            });
            x += tile_width;
        }
        y += tile_height;
    }
    rects
}

/// Smallest rectangle covering both inputs. Coordinates are document pixels;
/// saturation keeps a malformed rect from wrapping, while normal callers pass
/// rects already inside the canvas.
pub fn union_rect(a: (u32, u32, u32, u32), b: (u32, u32, u32, u32)) -> (u32, u32, u32, u32) {
    let ax1 = a.0.saturating_add(a.2);
    let ay1 = a.1.saturating_add(a.3);
    let bx1 = b.0.saturating_add(b.2);
    let by1 = b.1.saturating_add(b.3);
    let x0 = a.0.min(b.0);
    let y0 = a.1.min(b.1);
    let x1 = ax1.max(bx1);
    let y1 = ay1.max(by1);
    (x0, y0, x1.saturating_sub(x0), y1.saturating_sub(y0))
}

struct Tile {
    rect: TileRect,
    /// CPU copy in GPUI's BGRA order, retained so an update can rebuild only
    /// the tiles it touches.
    bgra: Vec<u8>,
    image: Arc<RenderImage>,
}

/// One uncommitted dirty rectangle painted over the base tiles. The CPU copy is
/// retained only until `commit_patches` folds it into the base tiles.
struct Patch {
    rect: TileRect,
    bgra: Vec<u8>,
    image: Arc<RenderImage>,
}

/// One scaled live-preview rectangle. The image is preview-sized while the
/// rect is document-space; the display scales the image into the destination.
/// It is display-only and is never folded into the base tiles.
struct PreviewPatch {
    rect: TileRect,
    image: Arc<RenderImage>,
}

fn render_image(width: u32, height: u32, bgra: Vec<u8>) -> Result<Arc<RenderImage>, FrameError> {
    let expected = width as usize * height as usize * 4;
    let got = bgra.len();
    let buffer = ::image::ImageBuffer::from_raw(width, height, bgra)
        .ok_or(FrameError::LengthMismatch { expected, got })?;
    Ok(Arc::new(RenderImage::new(vec![::image::Frame::new(
        buffer,
    )])))
}

/// A validated, displayable base-tile set plus an optional live patch overlay.
///
/// Each base tile is an immutable GPUI snapshot. [`CanvasImage::update_region`]
/// replaces only the tiles intersecting a dirty rectangle;
/// [`CanvasImage::push_patch`] adds a small live overlay without touching the
/// base; [`CanvasImage::commit_patches`] folds the overlay into the base;
/// [`CanvasImage::into_textures`] releases every tile and patch texture when the
/// whole snapshot is replaced. The caller owns atlas eviction: pass every
/// returned texture to `Window::drop_image` before the next paint.
pub struct CanvasImage {
    width: u32,
    height: u32,
    tile_size: u32,
    tiles: Vec<Tile>,
    patches: Vec<Patch>,
    preview_patches: Vec<PreviewPatch>,
}

mod image;

/// The frozen viewport's fit placement for one document in one viewport:
/// `zoom = clamp(min((w - 80)/dw, (h - 80)/dh), 0.05, 10)`, centered on both
/// axes (`useViewportRenderer.ts`). Any non-finite or non-positive input
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
/// exactly the frozen `CameraState` (`viewportCamera.ts`). The camera stays
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

    /// The frozen fit: padding 80, centered, clamped to `[0.05, 10]`.
    pub fn fit(viewport_w: f32, viewport_h: f32, doc_w: f32, doc_h: f32) -> Option<Self> {
        let (x, y, page_w, _) = fit_placement(viewport_w, viewport_h, doc_w, doc_h)?;
        Self::new(x, y, page_w / doc_w)
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

    /// Zoom around a screen anchor: the document point under it stays fixed
    /// (`ViewportCamera.zoomToPoint`).
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

/// A 34x34 layer thumbnail: nearest-sample the layer's RGBA pixels into a
/// BGRA `RenderImage` for the row. Non-raster or empty layers return `None`.
pub(crate) fn thumbnail_image(
    width: u32,
    height: u32,
    rgba: &[u8],
    edge: u32,
) -> Option<Arc<RenderImage>> {
    if width == 0 || height == 0 || edge == 0 || rgba.len() != width as usize * height as usize * 4
    {
        return None;
    }
    let mut bgra = vec![0u8; edge as usize * edge as usize * 4];
    for y in 0..edge {
        let src_y = (y as u64 * height as u64 / edge as u64) as u32;
        for x in 0..edge {
            let src_x = (x as u64 * width as u64 / edge as u64) as u32;
            let src = ((src_y as usize * width as usize) + src_x as usize) * 4;
            let dst = ((y as usize * edge as usize) + x as usize) * 4;
            bgra[dst] = rgba[src + 2];
            bgra[dst + 1] = rgba[src + 1];
            bgra[dst + 2] = rgba[src];
            bgra[dst + 3] = rgba[src + 3];
        }
    }
    render_image(edge, edge, bgra).ok()
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

/// Converts an element-relative placement origin into the window coordinates
/// `paint_image` works in: its destination rect is placed and clipped in
/// window space (`window.rs`: the visible region is
/// `bounds.intersect(&image_bounds)`), so the element's own origin is added
/// once, here, at the paint call.
fn page_origin(bounds_origin_x: f32, bounds_origin_y: f32, x: f32, y: f32) -> (f32, f32) {
    (bounds_origin_x + x, bounds_origin_y + y)
}

/// Snaps one page-relative edge to the device-pixel grid. A zero or non-finite
/// scale is passed through; callers already reject non-finite placements.
fn snap_edge(value: f32, scale: f32) -> f32 {
    if !value.is_finite() || !scale.is_finite() || scale <= 0. {
        return value;
    }
    (value * scale).round() / scale
}

/// Destination rectangle for one tile. The page placement stays continuous, but
/// both sides of every shared boundary are snapped from the same expression, so
/// adjacent tiles land on the same device pixel instead of leaving a seam.
#[allow(clippy::too_many_arguments)] // the placement inputs are a record; a struct would only rename them
fn tile_destination(
    page_x: f32,
    page_y: f32,
    page_w: f32,
    page_h: f32,
    doc_w: f32,
    doc_h: f32,
    tile: TileRect,
    scale: f32,
) -> Option<(f32, f32, f32, f32)> {
    if !page_x.is_finite()
        || !page_y.is_finite()
        || !page_w.is_finite()
        || !page_h.is_finite()
        || !doc_w.is_finite()
        || !doc_h.is_finite()
        || page_w <= 0.
        || page_h <= 0.
        || doc_w <= 0.
        || doc_h <= 0.
    {
        return None;
    }
    let left = snap_edge(page_x + page_w * (tile.x as f32 / doc_w), scale);
    let right = snap_edge(
        page_x + page_w * ((tile.x + tile.width) as f32 / doc_w),
        scale,
    );
    let top = snap_edge(page_y + page_h * (tile.y as f32 / doc_h), scale);
    let bottom = snap_edge(
        page_y + page_h * ((tile.y + tile.height) as f32 / doc_h),
        scale,
    );
    let width = right - left;
    let height = bottom - top;
    if !width.is_finite() || !height.is_finite() || width <= 0. || height <= 0. {
        return None;
    }
    Some((left, top, width, height))
}

/// The document viewport: paints the checkerboard, the base tiles, then any
/// live patch overlays, through the single `paint_image` seam.
///
/// The element takes the canvas slot's geometry (flex_1, min_h 0) and its own
/// background; prepaint resolves the camera (or fit) placement from the
/// element's own bounds, paint snaps every image edge to the shared
/// device-pixel grid and issues one paint call per tile/patch. GPUI batches
/// polychrome sprites per atlas texture, so the image count does not imply one
/// GPU draw call per tile.
pub fn canvas_element(canvas_image: &CanvasImage) -> impl IntoElement {
    canvas_element_inner(canvas_image, None, None, None, CanvasOverlay::default())
}

/// The display seam with its laid-out bounds published to `bounds` and the
/// live camera applied. `None` keeps the animated-fit contract: the placement
/// is recomputed from the element bounds every frame.
///
/// The checker cache is shared across frames so the screen-space pattern is
/// rebuilt only when the viewport size changes; the caller owns atlas eviction
/// for the replaced image (the paint pass drops it through `Window`).
pub(crate) fn canvas_element_with_bounds(
    canvas_image: &CanvasImage,
    bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    camera: Option<Camera>,
    checker: Option<Rc<RefCell<CheckerCache>>>,
    overlay: CanvasOverlay,
) -> impl IntoElement {
    canvas_element_inner(canvas_image, Some(bounds), camera, checker, overlay)
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct NavigatorLayout {
    pub scale: f32,
    pub offset_x: f32,
    pub offset_y: f32,
    pub width: f32,
    pub height: f32,
}

/// The frozen Navigator's 208x88 thumbnail box (`Navigator.tsx`), centred in
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

/// `px-4 pt-4` around the Navigator canvas (`Navigator.tsx`).
pub(crate) const NAV_PAD: f32 = 16.;
const NAV_H: f32 = 185.;
const NAV_HEADER_H: f32 = 38.;

/// Transient canvas chrome painted over the page: the committed/live pixel
/// selection and the crop frame. Coordinates are document space; the paint
/// pass maps them through the same page placement as the tiles.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct CanvasOverlay {
    /// `(x, y, width, height, ellipse)` — `ellipse` still paints a rectangular
    /// marquee because GPUI quads carry no ellipse outline.
    pub selection: Option<(f64, f64, f64, f64, bool)>,
    /// Crop frame `(x, y, width, height)`.
    pub crop: Option<(f64, f64, f64, f64)>,
    /// Move-tool transform frame: four visual corners and eight handle points
    /// in document space.
    pub transform: Option<TransformFrame>,
    /// Smart-guide lines: `(vertical, document coordinate)`.
    pub guides: Vec<(bool, f64)>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TransformFrame {
    pub corners: [[f64; 2]; 4],
    pub handles: [[f64; 2]; 8],
}

fn doc_point_to_window(
    page_x: f32,
    page_y: f32,
    page_w: f32,
    page_h: f32,
    doc_w: f32,
    doc_h: f32,
    point_doc: [f64; 2],
) -> Option<Point<Pixels>> {
    if !page_w.is_finite() || !page_h.is_finite() || page_w <= 0. || page_h <= 0. {
        return None;
    }
    let x = page_x + page_w * (point_doc[0] as f32 / doc_w);
    let y = page_y + page_h * (point_doc[1] as f32 / doc_h);
    if !x.is_finite() || !y.is_finite() {
        return None;
    }
    Some(point(px(x), px(y)))
}

fn doc_rect_to_window(
    page_x: f32,
    page_y: f32,
    page_w: f32,
    page_h: f32,
    doc_w: f32,
    doc_h: f32,
    rect: (f64, f64, f64, f64),
) -> Option<Bounds<Pixels>> {
    if !page_w.is_finite() || !page_h.is_finite() || page_w <= 0. || page_h <= 0. {
        return None;
    }
    let (x, y, width, height) = rect;
    if ![x, y, width, height].iter().all(|value| value.is_finite()) {
        return None;
    }
    let left = page_x + page_w * (x as f32 / doc_w);
    let top = page_y + page_h * (y as f32 / doc_h);
    let right = page_x + page_w * ((x + width) as f32 / doc_w);
    let bottom = page_y + page_h * ((y + height) as f32 / doc_h);
    if !left.is_finite() || !top.is_finite() || right <= left || bottom <= top {
        return None;
    }
    Some(Bounds {
        origin: point(px(left), px(top)),
        size: size(px(right - left), px(bottom - top)),
    })
}

/// One cached screen-space checkerboard image, keyed by logical viewport size.
#[derive(Default)]
pub(crate) struct CheckerCache {
    entry: Option<(u32, u32, Arc<RenderImage>)>,
}

impl CheckerCache {
    fn image_for(&mut self, width: f32, height: f32) -> Option<Arc<RenderImage>> {
        let width = width.ceil();
        let height = height.ceil();
        if !width.is_finite() || !height.is_finite() || width < 1. || height < 1. {
            return None;
        }
        let width = (width as u32).min(MAX_CANVAS_DIM);
        let height = (height as u32).min(MAX_CANVAS_DIM);
        if let Some((w, h, image)) = &self.entry {
            if (*w, *h) == (width, height) {
                return Some(Arc::clone(image));
            }
        }
        let cell = CHECKER_CELL.max(1.) as u32;
        let mut bgra = vec![0u8; width as usize * height as usize * 4];
        for y in 0..height {
            for x in 0..width {
                let light = ((x / cell) + (y / cell)).is_multiple_of(2);
                let color = if light { CHECKER_LIGHT } else { CHECKER_DARK };
                let offset = ((y as usize * width as usize) + x as usize) * 4;
                bgra[offset] = color[2];
                bgra[offset + 1] = color[1];
                bgra[offset + 2] = color[0];
                bgra[offset + 3] = color[3];
            }
        }
        let image = render_image(width, height, bgra).ok()?;
        self.entry = Some((width, height, Arc::clone(&image)));
        Some(image)
    }
}

fn canvas_element_inner(
    canvas_image: &CanvasImage,
    bounds_sink: Option<Rc<Cell<Option<Bounds<Pixels>>>>>,
    camera: Option<Camera>,
    checker: Option<Rc<RefCell<CheckerCache>>>,
    overlay: CanvasOverlay,
) -> impl IntoElement {
    let images: Vec<(TileRect, Arc<RenderImage>)> = canvas_image
        .tiles
        .iter()
        .map(|tile| (tile.rect, Arc::clone(&tile.image)))
        .chain(
            canvas_image
                .patches
                .iter()
                .map(|patch| (patch.rect, Arc::clone(&patch.image))),
        )
        .chain(
            canvas_image
                .preview_patches
                .iter()
                .map(|patch| (patch.rect, Arc::clone(&patch.image))),
        )
        .collect();
    let doc_w = canvas_image.width as f32;
    let doc_h = canvas_image.height as f32;
    let bounds_sink = bounds_sink.clone();
    canvas(
        move |bounds, _window, _cx| {
            if let Some(sink) = &bounds_sink {
                sink.set(Some(bounds));
            }
            camera_placement(
                bounds.size.width.into(),
                bounds.size.height.into(),
                doc_w,
                doc_h,
                camera,
            )
        },
        move |bounds, placement, window, _cx| {
            let Some((x, y, w, h)) = placement else {
                return;
            };
            let (page_x, page_y) =
                page_origin(bounds.origin.x.into(), bounds.origin.y.into(), x, y);
            let scale = window.scale_factor();
            if let Some(checker) = &checker {
                let page_bounds = Bounds {
                    origin: point(px(page_x), px(page_y)),
                    size: size(px(w), px(h)),
                };
                let viewport_w: f32 = bounds.size.width.into();
                let viewport_h: f32 = bounds.size.height.into();
                if let Some(image) = checker.borrow_mut().image_for(viewport_w, viewport_h) {
                    let _ = window.paint_image(
                        page_bounds,
                        bounds,
                        Corners::default(),
                        image,
                        0,
                        false,
                    );
                }
            }
            let mut reported_error = false;
            for (rect, image) in &images {
                let Some((left, top, tile_w, tile_h)) =
                    tile_destination(page_x, page_y, w, h, doc_w, doc_h, *rect, scale)
                else {
                    continue;
                };
                let tile_bounds = Bounds {
                    origin: point(px(left), px(top)),
                    size: size(px(tile_w), px(tile_h)),
                };
                if let Err(error) = window.paint_image(
                    bounds,
                    tile_bounds,
                    Corners::default(),
                    Arc::clone(image),
                    0,
                    false,
                ) {
                    if !reported_error {
                        eprintln!("photrez: canvas image paint failed: {error:#}");
                        reported_error = true;
                    }
                }
            }
            let page_bounds = Bounds {
                origin: point(px(page_x), px(page_y)),
                size: size(px(w), px(h)),
            };
            if let Some(selection) = overlay.selection {
                let rect = (selection.0, selection.1, selection.2, selection.3);
                if let Some(bounds) = doc_rect_to_window(page_x, page_y, w, h, doc_w, doc_h, rect) {
                    window.paint_quad(outline(bounds, hsla(0., 0., 1., 0.9), BorderStyle::Dashed));
                }
            }
            for (vertical, position) in &overlay.guides {
                let guide_color = hsla(0.09, 0.9, 0.55, 0.9);
                if *vertical {
                    if let Some(origin) =
                        doc_point_to_window(page_x, page_y, w, h, doc_w, doc_h, [*position, 0.0])
                    {
                        window.paint_quad(fill(
                            Bounds {
                                origin,
                                size: size(px(1.), px(h)),
                            },
                            guide_color,
                        ));
                    }
                } else if let Some(origin) =
                    doc_point_to_window(page_x, page_y, w, h, doc_w, doc_h, [0.0, *position])
                {
                    window.paint_quad(fill(
                        Bounds {
                            origin,
                            size: size(px(w), px(1.)),
                        },
                        guide_color,
                    ));
                }
            }
            if let Some(frame) = overlay.transform {
                let mut path = PathBuilder::stroke(px(1.5));
                if let Some(first) =
                    doc_point_to_window(page_x, page_y, w, h, doc_w, doc_h, frame.corners[0])
                {
                    path.move_to(first);
                    let mut closed = true;
                    for corner in frame.corners.iter().skip(1) {
                        let Some(point) =
                            doc_point_to_window(page_x, page_y, w, h, doc_w, doc_h, *corner)
                        else {
                            closed = false;
                            break;
                        };
                        path.line_to(point);
                    }
                    if closed {
                        path.close();
                        if let Ok(path) = path.build() {
                            window.paint_path(path, color::ACCENT);
                        }
                    }
                }
                let handle_edge = 6.0f32;
                for handle in frame.handles {
                    let Some(center) =
                        doc_point_to_window(page_x, page_y, w, h, doc_w, doc_h, handle)
                    else {
                        continue;
                    };
                    let bounds = Bounds {
                        origin: point(
                            px(f32::from(center.x) - handle_edge / 2.),
                            px(f32::from(center.y) - handle_edge / 2.),
                        ),
                        size: size(px(handle_edge), px(handle_edge)),
                    };
                    window.paint_quad(fill(bounds, hsla(0., 0., 1., 1.)));
                    window.paint_quad(outline(bounds, color::ACCENT, BorderStyle::Solid));
                }
            }
            if let Some(crop) = overlay.crop {
                if let Some(rect) = doc_rect_to_window(page_x, page_y, w, h, doc_w, doc_h, crop) {
                    let mask = hsla(0., 0., 0., 0.55);
                    let top = Bounds {
                        origin: page_bounds.origin,
                        size: size(page_bounds.size.width, rect.origin.y - page_bounds.origin.y),
                    };
                    let bottom = Bounds {
                        origin: point(page_bounds.origin.x, rect.origin.y + rect.size.height),
                        size: size(
                            page_bounds.size.width,
                            page_bounds.origin.y + page_bounds.size.height
                                - (rect.origin.y + rect.size.height),
                        ),
                    };
                    let left = Bounds {
                        origin: point(page_bounds.origin.x, rect.origin.y),
                        size: size(rect.origin.x - page_bounds.origin.x, rect.size.height),
                    };
                    let right = Bounds {
                        origin: point(rect.origin.x + rect.size.width, rect.origin.y),
                        size: size(
                            page_bounds.origin.x + page_bounds.size.width
                                - (rect.origin.x + rect.size.width),
                            rect.size.height,
                        ),
                    };
                    for band in [top, bottom, left, right] {
                        let visible = band.intersect(&page_bounds);
                        if visible.size.width > px(0.) && visible.size.height > px(0.) {
                            window.paint_quad(fill(visible, mask));
                        }
                    }
                    window.paint_quad(outline(rect, color::ACCENT, BorderStyle::Solid));
                    let guide = hsla(0., 0., 1., 0.25);
                    for step in [1. / 3., 2. / 3.] {
                        let x = rect.origin.x + rect.size.width * step;
                        let y = rect.origin.y + rect.size.height * step;
                        window.paint_quad(fill(
                            Bounds {
                                origin: point(x, rect.origin.y),
                                size: size(px(1.), rect.size.height),
                            },
                            guide,
                        ));
                        window.paint_quad(fill(
                            Bounds {
                                origin: point(rect.origin.x, y),
                                size: size(rect.size.width, px(1.)),
                            },
                            guide,
                        ));
                    }
                }
            }
        },
    )
    .flex_1()
    .min_h(px(0.0))
    .w_full()
    .bg(color::CANVAS)
}

#[cfg(test)]
mod tests;
