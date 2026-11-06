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
//! - The page placement: the viewport fit formula, recovered from the
//!   original renderer's placement code and
//!   pinned against the capture `tests/baseline/shell-canvas.png` (the
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
//! - The 32px blurred shadow under the page: this paint path has no blur
//!   primitive, so the artboard ships with its 1px border and 1px ring only.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::prelude::*;
use gpui_kit::{
    canvas, fill, hsla, outline, point, px, size, BorderStyle, Bounds, Corners, PathBuilder,
    Pixels, Point, RenderImage,
};

use crate::tokens::color;
pub use camera::*;
pub(crate) use navigator::*;
use painted::*;
pub use tiles::*;

/// Product bound for one document edge - the mirror of `photrez-core`'s
/// private `MAX_CANVAS_DIM` (`crates/core/src/document.rs`), which the
/// document dialog's validation contract also enforces.
pub const MAX_CANVAS_DIM: u32 = 16384;

/// One checkerboard cell, in screen pixels.
pub const CHECKER_CELL: f32 = 8.;
/// Checker cell greys, sampled from the captured checkerboard
/// (0.78 and 0.55 relative luminance).
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

mod camera;
mod image;
mod navigator;
mod painted;
mod tiles;

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
            painted_artboard_chrome(window, page_bounds);
            if let Some(selection) = overlay.selection {
                // Selection marching ants: a 1.5 px accent dash
                // under a 1 px white dash offset by one period, so the marquee
                // stays visible on both light and dark content. Each layer is
                // one tessellated polyline — one path per layer, not per edge.
                let ants = |window: &mut gpui_kit::Window, ring: &[(f32, f32)]| {
                    painted_dashed_polyline(window, ring, 1.5, 4.0, 4.0, 0.0, color::ACCENT);
                    painted_dashed_polyline(
                        window,
                        ring,
                        1.0,
                        4.0,
                        4.0,
                        4.0,
                        hsla(0., 0., 1., 0.9),
                    );
                };
                if selection.angle.abs() < 1e-6 {
                    let rect = (selection.x, selection.y, selection.w, selection.h);
                    if let Some(bounds) =
                        doc_rect_to_window(page_x, page_y, w, h, doc_w, doc_h, rect)
                    {
                        if selection.ellipse {
                            let center = (
                                f32::from(bounds.origin.x) + f32::from(bounds.size.width) / 2.0,
                                f32::from(bounds.origin.y) + f32::from(bounds.size.height) / 2.0,
                            );
                            painted_dashed_ellipse(
                                window,
                                center,
                                f32::from(bounds.size.width) / 2.0,
                                f32::from(bounds.size.height) / 2.0,
                                1.5,
                                4.0,
                                4.0,
                                0.0,
                                color::ACCENT,
                            );
                            painted_dashed_ellipse(
                                window,
                                center,
                                f32::from(bounds.size.width) / 2.0,
                                f32::from(bounds.size.height) / 2.0,
                                1.0,
                                4.0,
                                4.0,
                                4.0,
                                hsla(0., 0., 1., 0.9),
                            );
                        } else {
                            let left = f32::from(bounds.origin.x);
                            let top = f32::from(bounds.origin.y);
                            let right = left + f32::from(bounds.size.width);
                            let bottom = top + f32::from(bounds.size.height);
                            let ring = [
                                (left, top),
                                (right, top),
                                (right, bottom),
                                (left, bottom),
                                (left, top),
                            ];
                            ants(window, &ring);
                        }
                    }
                } else {
                    let corners = crate::app::transform_geometry::rotated_rect_corners(
                        selection.x,
                        selection.y,
                        selection.w,
                        selection.h,
                        selection.angle,
                    );
                    let mut screen: Vec<(f32, f32)> = corners
                        .iter()
                        .filter_map(|corner| {
                            doc_point_to_window(
                                page_x,
                                page_y,
                                w,
                                h,
                                doc_w,
                                doc_h,
                                [corner.x, corner.y],
                            )
                            .map(|p| (f32::from(p.x), f32::from(p.y)))
                        })
                        .collect();
                    if screen.len() == 4 {
                        if let Some(&first_corner) = screen.first() {
                            screen.push(first_corner);
                        }
                        ants(window, &screen);
                    }
                }
                if selection.edit_mode {
                    let (top_mid, normal) = crate::app::transform_geometry::selection_pin(
                        selection.x,
                        selection.y,
                        selection.w,
                        selection.h,
                        selection.angle,
                    );
                    let mapped_top = doc_point_to_window(
                        page_x,
                        page_y,
                        w,
                        h,
                        doc_w,
                        doc_h,
                        [top_mid.x, top_mid.y],
                    );
                    if let Some(top) = mapped_top {
                        let scale = (w / doc_w).max(1e-6);
                        let unit = (normal.x as f32, normal.y as f32);
                        let anchor = (
                            f32::from(top.x) + unit.0 * 4.0,
                            f32::from(top.y) + unit.1 * 4.0,
                        );
                        let pin = (
                            f32::from(top.x) + unit.0 * 24.0 * scale,
                            f32::from(top.y) + unit.1 * 24.0 * scale,
                        );
                        let rim = (pin.0 - unit.0 * 5.0, pin.1 - unit.1 * 5.0);
                        painted_dashed_line(window, anchor, rim, 1.0, 2.0, 2.0, color::ACCENT);
                        painted_circle(window, pin, 5.0, None, hsla(0., 0., 1., 1.));
                        painted_circle(window, pin, 5.0, Some(1.5), color::ACCENT);
                    }
                    // Selection edit mode: 8 px corner squares
                    // and 6 px edge squares on the rotated frame.
                    let corners = crate::app::transform_geometry::rotated_rect_corners(
                        selection.x,
                        selection.y,
                        selection.w,
                        selection.h,
                        selection.angle,
                    );
                    let mapped: Vec<(f32, f32)> = corners
                        .iter()
                        .filter_map(|corner| {
                            doc_point_to_window(
                                page_x,
                                page_y,
                                w,
                                h,
                                doc_w,
                                doc_h,
                                [corner.x, corner.y],
                            )
                            .map(|p| (f32::from(p.x), f32::from(p.y)))
                        })
                        .collect();
                    if mapped.len() == 4 {
                        for index in 0..4 {
                            painted_handle(window, mapped[index], 8.0);
                            let next = mapped[(index + 1) % 4];
                            painted_handle(
                                window,
                                (
                                    (mapped[index].0 + next.0) / 2.0,
                                    (mapped[index].1 + next.1) / 2.0,
                                ),
                                6.0,
                            );
                        }
                    }
                }
            }
            if let Some(sprite) = overlay.rotate_cursor {
                painted_rotate_cursor(
                    window,
                    (sprite.position[0], sprite.position[1]),
                    sprite.angle,
                );
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
            if let Some(highlight) = overlay.hover_highlight {
                if let Some(rect) =
                    doc_rect_to_window(page_x, page_y, w, h, doc_w, doc_h, highlight)
                {
                    window.paint_quad(outline(
                        rect,
                        color::ACCENT.opacity(0.8),
                        BorderStyle::Solid,
                    ));
                }
            }
            if let Some(cursor) = overlay.brush_cursor {
                if doc_w > 0. && doc_h > 0. {
                    if let Some(center) =
                        doc_point_to_window(page_x, page_y, w, h, doc_w, doc_h, cursor.center)
                    {
                        let cx: f32 = center.x.into();
                        let cy: f32 = center.y.into();
                        let rx = (cursor.radii[0] * (w / doc_w) as f64) as f32;
                        let ry = (cursor.radii[1] * (h / doc_h) as f64) as f32;
                        let (sin, cos) = cursor.rotation.sin_cos();
                        let (sin, cos) = (sin as f32, cos as f32);
                        let ring = |width: f32| {
                            let mut path = PathBuilder::stroke(px(width));
                            for step in 0..=48 {
                                let t = step as f32 / 48.0 * std::f32::consts::TAU;
                                let (st, ct) = t.sin_cos();
                                let local_x = rx * ct;
                                let local_y = ry * st;
                                let x = cx + local_x * cos - local_y * sin;
                                let y = cy + local_x * sin + local_y * cos;
                                if step == 0 {
                                    path.move_to(point(px(x), px(y)));
                                } else {
                                    path.line_to(point(px(x), px(y)));
                                }
                            }
                            path.close();
                            path.build().ok()
                        };
                        if let Some(path) = ring(1.5) {
                            window.paint_path(path, hsla(0., 0., 0., 0.5));
                        }
                        if let Some(path) = ring(1.0) {
                            window.paint_path(path, hsla(0., 0., 1., 1.));
                        }
                        if cursor.crosshair {
                            let mut cross = PathBuilder::stroke(px(1.0));
                            cross.move_to(point(px(cx - 4.0), px(cy)));
                            cross.line_to(point(px(cx + 4.0), px(cy)));
                            cross.move_to(point(px(cx), px(cy - 4.0)));
                            cross.line_to(point(px(cx), px(cy + 4.0)));
                            if let Ok(path) = cross.build() {
                                window.paint_path(path, hsla(0., 0., 1., 1.));
                            }
                        }
                    }
                }
            }
            if let Some(line) = overlay.gradient_line {
                if let (Some(start), Some(end)) = (
                    doc_point_to_window(page_x, page_y, w, h, doc_w, doc_h, line.start),
                    doc_point_to_window(page_x, page_y, w, h, doc_w, doc_h, line.end),
                ) {
                    let s = (f32::from(start.x), f32::from(start.y));
                    let e = (f32::from(end.x), f32::from(end.y));
                    let white = hsla(0., 0., 1., 1.);
                    if line.radial && doc_w > 0. {
                        let dx = line.end[0] - line.start[0];
                        let dy = line.end[1] - line.start[1];
                        let radius = ((dx * dx + dy * dy).sqrt() * (w / doc_w) as f64) as f32;
                        painted_circle(window, s, radius, Some(3.0), hsla(0., 0., 0., 0.4));
                        painted_dashed_circle(
                            window,
                            s,
                            radius,
                            1.5,
                            4.0,
                            4.0,
                            color::ACCENT.opacity(0.9),
                        );
                    }
                    painted_dashed_line(window, s, e, 3.0, 1.0e6, 0.0, hsla(0., 0., 0., 0.5));
                    painted_dashed_line(window, s, e, 2.0, 6.0, 3.0, color::ACCENT);
                    painted_circle(window, s, 6.0, None, color::ACCENT);
                    painted_circle(window, s, 6.0, Some(2.0), white);
                    painted_circle(window, e, 5.0, None, white);
                    painted_circle(window, e, 5.0, Some(2.5), color::ACCENT);
                }
            }
            if let Some(marquee) = overlay.move_marquee {
                if let Some(rect) = doc_rect_to_window(page_x, page_y, w, h, doc_w, doc_h, marquee)
                {
                    window.paint_quad(fill(rect, color::ACCENT.opacity(0.15)));
                    window.paint_quad(outline(rect, color::ACCENT, BorderStyle::Dashed));
                }
            }
            if let Some(frame) = overlay.transform {
                let mut path = PathBuilder::stroke(px(if overlay.transform_controls {
                    1.5
                } else {
                    1.25
                }));
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
                if !overlay.transform_controls {
                    let doc_per_screen = f64::from(doc_w) / f64::from(w).max(1.0);
                    let leg = (8.0 * doc_per_screen).max(1.0);
                    let mut brackets = PathBuilder::stroke(px(1.25));
                    let mut drew = false;
                    for index in 0..4 {
                        let corner = frame.corners[index];
                        let next = frame.corners[(index + 1) % 4];
                        let previous = frame.corners[(index + 3) % 4];
                        let leg_point = |towards: [f64; 2]| step_towards(corner, towards, leg);
                        if let (Some(origin), Some(a), Some(b)) = (
                            doc_point_to_window(page_x, page_y, w, h, doc_w, doc_h, corner),
                            doc_point_to_window(
                                page_x,
                                page_y,
                                w,
                                h,
                                doc_w,
                                doc_h,
                                leg_point(next),
                            ),
                            doc_point_to_window(
                                page_x,
                                page_y,
                                w,
                                h,
                                doc_w,
                                doc_h,
                                leg_point(previous),
                            ),
                        ) {
                            brackets.move_to(origin);
                            brackets.line_to(a);
                            brackets.move_to(origin);
                            brackets.line_to(b);
                            drew = true;
                        }
                    }
                    if drew {
                        if let Ok(path) = brackets.build() {
                            window.paint_path(path, color::ACCENT);
                        }
                    }
                }
                let handle_edge = 8.0f32;
                let visible_handles: &[[f64; 2]] = if overlay.transform_controls {
                    &frame.handles
                } else {
                    &[]
                };
                // The transform overlay draws a rotate pin 20 px above
                // the rotated top edge: a 1.25 px accent stem and a 4.5 px white
                // circle with an accent rim.
                if overlay.transform_controls {
                    let window_corners: Vec<(f32, f32)> = frame
                        .corners
                        .iter()
                        .filter_map(|corner| {
                            doc_point_to_window(page_x, page_y, w, h, doc_w, doc_h, *corner)
                                .map(|p| (f32::from(p.x), f32::from(p.y)))
                        })
                        .collect();
                    if window_corners.len() == 4 {
                        let top_mid = (
                            (window_corners[0].0 + window_corners[1].0) / 2.0,
                            (window_corners[0].1 + window_corners[1].1) / 2.0,
                        );
                        let center = (
                            window_corners.iter().map(|p| p.0).sum::<f32>() / 4.0,
                            window_corners.iter().map(|p| p.1).sum::<f32>() / 4.0,
                        );
                        let edge = (
                            window_corners[1].0 - window_corners[0].0,
                            window_corners[1].1 - window_corners[0].1,
                        );
                        let mut normal = (-edge.1, edge.0);
                        let length = (normal.0 * normal.0 + normal.1 * normal.1).sqrt();
                        if length > f32::EPSILON {
                            normal = (normal.0 / length, normal.1 / length);
                            if normal.0 * (top_mid.0 - center.0) + normal.1 * (top_mid.1 - center.1)
                                < 0.0
                            {
                                normal = (-normal.0, -normal.1);
                            }
                            let pin = (top_mid.0 + normal.0 * 20.0, top_mid.1 + normal.1 * 20.0);
                            let mut stem = PathBuilder::stroke(px(1.25));
                            stem.move_to(point(px(top_mid.0), px(top_mid.1)));
                            stem.line_to(point(px(pin.0), px(pin.1)));
                            if let Ok(path) = stem.build() {
                                window.paint_path(path, color::ACCENT);
                            }
                            painted_circle(window, pin, 4.5, None, hsla(0., 0., 1., 1.));
                            painted_circle(window, pin, 4.5, Some(1.5), color::ACCENT);
                        }
                    }
                }
                for handle in visible_handles {
                    let Some(center) =
                        doc_point_to_window(page_x, page_y, w, h, doc_w, doc_h, *handle)
                    else {
                        continue;
                    };
                    painted_handle(
                        window,
                        (f32::from(center.x), f32::from(center.y)),
                        handle_edge,
                    );
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
                    // The crop overlay: a dark outer hairline, a white
                    // inner one, white thirds at half opacity, and eight
                    // 8 px white handles with a dark rim.
                    window.paint_quad(outline(rect, hsla(0., 0., 0., 0.55), BorderStyle::Solid));
                    let inner = Bounds {
                        origin: point(rect.origin.x + px(1.), rect.origin.y + px(1.)),
                        size: size(rect.size.width - px(2.), rect.size.height - px(2.)),
                    };
                    window.paint_quad(outline(inner, hsla(0., 0., 1., 0.9), BorderStyle::Solid));
                    let guide = hsla(0., 0., 1., 0.5);
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
                    let handle_edge = 8.0f32;
                    let center_x = rect.origin.x + rect.size.width / 2.;
                    let center_y = rect.origin.y + rect.size.height / 2.;
                    let xs = [rect.origin.x, center_x, rect.origin.x + rect.size.width];
                    let ys = [rect.origin.y, center_y, rect.origin.y + rect.size.height];
                    for (column, x) in xs.iter().enumerate() {
                        for (row, y) in ys.iter().enumerate() {
                            if column == 1 && row == 1 {
                                continue;
                            }
                            let bounds = Bounds {
                                origin: point(*x - px(handle_edge / 2.), *y - px(handle_edge / 2.)),
                                size: size(px(handle_edge), px(handle_edge)),
                            };
                            window.paint_quad(fill(bounds, hsla(0., 0., 1., 0.9)));
                            window.paint_quad(outline(
                                bounds,
                                hsla(0., 0., 0., 0.35),
                                BorderStyle::Solid,
                            ));
                        }
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
