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
//! ## What deliberately is not here yet
//!
//! - Pan, zoom and the animated fit: `ImageViewer` is absent from the gpui-kit
//!   build, so the viewport is ours; this landing implements only the default
//!   fit placement.
//! - The artboard chrome beyond the page (the frozen overlay's 1px border,
//!   1px outer ring and 32px soft shadow): it rides the visual close-out.
//! - Partial (dirty-rect) uploads: there is no edit path yet. The first frame
//!   after a document opens is a legitimate full upload; partial uploads
//!   arrive with the edit path.

use std::sync::Arc;

use gpui_kit::prelude::*;
use gpui_kit::{canvas, point, px, size, Bounds, Corners, RenderImage};

use crate::tokens::color;

/// Product bound for one document edge - the mirror of `photrez-core`'s
/// private `MAX_CANVAS_DIM` (`crates/core/src/document.rs`), which the
/// document dialog's validation contract also enforces.
pub const MAX_CANVAS_DIM: u32 = 16384;

/// Total fit padding across both axes (the frozen viewport's own constant).
const FIT_PADDING: f32 = 80.;
/// The frozen viewport's zoom clamp, `[0.05, 10]`.
const ZOOM_MIN: f32 = 0.05;
const ZOOM_MAX: f32 = 10.;

/// Why a frame was refused. Refusal draws nothing; it is never a panic.
#[derive(Debug, PartialEq, Eq)]
pub enum FrameError {
    /// A zero edge has nothing to display.
    ZeroDimension,
    /// An edge is beyond [`MAX_CANVAS_DIM`].
    TooLarge { width: u32, height: u32 },
    /// The buffer length is not `width * height * 4`.
    LengthMismatch { expected: usize, got: usize },
}

/// A validated, displayable snapshot of the document.
pub struct CanvasImage {
    width: u32,
    height: u32,
    image: Arc<RenderImage>,
}

impl CanvasImage {
    /// Converts one owned RGBA8 buffer (row-major, `photrez-core`'s output)
    /// into GPUI's image type, swapping to the BGRA order GPUI's atlas reads.
    pub fn from_rgba(width: u32, height: u32, mut rgba: Vec<u8>) -> Result<Self, FrameError> {
        if width == 0 || height == 0 {
            return Err(FrameError::ZeroDimension);
        }
        if width > MAX_CANVAS_DIM || height > MAX_CANVAS_DIM {
            return Err(FrameError::TooLarge { width, height });
        }
        let expected = width as usize * height as usize * 4;
        let got = rgba.len();
        if got != expected {
            return Err(FrameError::LengthMismatch { expected, got });
        }
        // RGBA -> BGRA, once, at the seam.
        for pixel in rgba.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
        let buffer = match image::ImageBuffer::from_raw(width, height, rgba) {
            Some(buffer) => buffer,
            // `from_raw` fails only on a length mismatch, which was rejected
            // above; kept total so this seam never unwraps.
            None => return Err(FrameError::LengthMismatch { expected, got }),
        };
        Ok(Self {
            width,
            height,
            image: Arc::new(RenderImage::new(vec![image::Frame::new(buffer)])),
        })
    }
}

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
        .clamp(ZOOM_MIN, ZOOM_MAX);
    let w = doc_w * zoom;
    let h = doc_h * zoom;
    Some(((viewport_w - w) / 2., (viewport_h - h) / 2., w, h))
}

/// Converts an element-relative placement origin into the window coordinates
/// `paint_image` works in: its destination rect is placed and clipped in
/// window space (`window.rs`: the visible region is
/// `bounds.intersect(&image_bounds)`), so the element's own origin is added
/// once, here, at the paint call.
fn page_origin(bounds_origin_x: f32, bounds_origin_y: f32, x: f32, y: f32) -> (f32, f32) {
    (bounds_origin_x + x, bounds_origin_y + y)
}

/// The document viewport: one GPUI texture draw per frame.
///
/// The element takes the canvas slot's geometry (flex_1, min_h 0) and its own
/// background; prepaint resolves the fit placement from the element's own
/// bounds, paint performs the single `paint_image` call.
pub fn canvas_element(canvas_image: &CanvasImage) -> impl IntoElement {
    let image = Arc::clone(&canvas_image.image);
    let doc_w = canvas_image.width as f32;
    let doc_h = canvas_image.height as f32;
    canvas(
        move |bounds, _window, _cx| {
            fit_placement(
                bounds.size.width.into(),
                bounds.size.height.into(),
                doc_w,
                doc_h,
            )
        },
        move |bounds, placement, window, _cx| {
            let Some((x, y, w, h)) = placement else {
                return;
            };
            let (page_x, page_y) =
                page_origin(bounds.origin.x.into(), bounds.origin.y.into(), x, y);
            let page = Bounds {
                origin: point(px(page_x), px(page_y)),
                size: size(px(w), px(h)),
            };
            if let Err(error) =
                window.paint_image(bounds, page, Corners::default(), image, 0, false)
            {
                eprintln!("photrez: canvas paint failed: {error:#}");
            }
        },
    )
    .flex_1()
    .min_h(px(0.0))
    .w_full()
    .bg(color::CANVAS)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The frozen capture, measured: the 781x668 `#canvas-container` crop
    /// draws the 800x600 page as a 701x525.75 box at (40, 71.125) - exactly
    /// the fit formula's prediction.
    #[test]
    fn fit_placement_matches_the_frozen_capture() {
        let (x, y, w, h) = fit_placement(781., 668., 800., 600.).expect("fits");
        assert!((x - 40.).abs() < 0.01, "x: {x}");
        assert!((y - 71.125).abs() < 0.01, "y: {y}");
        assert!((w - 701.).abs() < 0.01, "w: {w}");
        assert!((h - 525.75).abs() < 0.01, "h: {h}");
    }

    /// `paint_image` positions its destination rect in window coordinates:
    /// the element-relative placement origin must be offset by the element's
    /// own origin, or the page lands near the window's top-left corner and is
    /// clipped by the element bounds instead of being centered in it.
    #[test]
    fn page_origin_is_offset_into_window_coordinates() {
        let (x, y) = page_origin(52., 127.2, 40., 90.5);
        assert!((x - 92.).abs() < 0.01, "x: {x}");
        assert!((y - 217.7).abs() < 0.01, "y: {y}");
    }

    /// GPUI's atlas reads raw frame bytes as BGRA (its own producers swap
    /// RGBA -> BGRA before `Frame::new`); the seam must hand it BGRA.
    #[test]
    fn rgba_becomes_bgra_in_the_frame() {
        let image =
            CanvasImage::from_rgba(2, 1, vec![255, 0, 0, 255, 0, 0, 255, 255]).expect("valid");
        let bytes = image.image.as_bytes(0).expect("one frame");
        assert_eq!(&bytes[0..4], &[0, 0, 255, 255], "red must land blue-first");
        assert_eq!(&bytes[4..8], &[255, 0, 0, 255], "blue must land red-first");
    }
}
