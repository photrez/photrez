// SPDX-License-Identifier: AGPL-3.0-or-later
//! CPU compositing: the document-to-pixels entry point plus the shape and text
//! rasterizers behind it.
//!
//! `compositor` is the single entry point a host calls; shape and text stay
//! implementation details. The package reads pixels through [`RenderInput`] and
//! never names a pixel registry, a document store, or a history type, so it holds
//! no document state: `photrez-core` owns that and implements the input for its
//! registry. It depends only on `photrez-document` and `photrez-raster`.

pub mod compositor;
pub mod input;
pub(crate) mod shape;
pub(crate) mod text;

// Map-backed input for the compositing tests.
#[cfg(test)]
mod test_input;

pub use input::{RenderInput, RenderLayerPixels};

/// Rasterize one text layer for the editor's live typing preview.
///
/// The text tool paints the session buffer through the same rasterizer the
/// committed layer uses, so the overlay is WYSIWYG and matches the document
/// raster while the buffer lives only in UI state. The host pushes the result
/// as a canvas preview patch; no history entry is recorded.
pub fn rasterize_text_preview(
    data: &photrez_document::layer_params::TextData,
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
    params: &photrez_document::layer_params::ShapeParams,
) -> Option<(u32, u32, Vec<u8>)> {
    shape::rasterize_shape(params)
        .ok()
        .map(|bitmap| (bitmap.width, bitmap.height, bitmap.pixels))
}
