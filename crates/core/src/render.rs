// SPDX-License-Identifier: AGPL-3.0-or-later
// The public CPU compositor facade plus the crate-private rasterizers and
// render-side DTOs. `compositor` is the single document-to-pixels entry point
// the native host may call; shape and text stay implementation details.

pub mod compositor;
pub(crate) mod layer;
pub(crate) mod shape;
pub(crate) mod text;

pub(crate) use layer::*;

/// Rasterize one text layer for the editor's live typing preview.
///
/// The text tool paints the session buffer through the same rasterizer the
/// committed layer uses, so the overlay is WYSIWYG and matches the document
/// raster while the buffer lives only in UI state. The host pushes the result
/// as a canvas preview patch; no history entry is recorded.
pub fn rasterize_text_preview(
    data: &crate::ptz::layer_params::TextData,
    font_data: Option<&[u8]>,
) -> Option<(u32, u32, Vec<u8>)> {
    text::rasterize_text(data, font_data, 1.0)
        .ok()
        .map(|bitmap| (bitmap.width, bitmap.height, bitmap.pixels))
}

/// Rasterize one shape layer's params — the host-side half of
/// `RasterizeLayer`'s pixel materialization (the shape rasterizer stays an
/// implementation detail; the compositor is the normal document-to-pixels
/// entry point, but a rasterize command must carry the buffer itself).
pub fn rasterize_shape_preview(
    params: &crate::ptz::layer_params::ShapeParams,
) -> Option<(u32, u32, Vec<u8>)> {
    shape::rasterize_shape(params)
        .ok()
        .map(|bitmap| (bitmap.width, bitmap.height, bitmap.pixels))
}
