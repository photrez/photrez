// SPDX-License-Identifier: AGPL-3.0-or-later
//! The read-only pixel source the compositor draws from.
//!
//! `photrez-core` implements [`RenderInput`] for its pixel registry. Compositing
//! takes this instead of the registry so the render path holds no document store,
//! no history, and no way to mutate a canonical buffer.

/// One layer's canonical pixels, as the compositor reads them.
pub struct RenderLayerPixels<'a> {
    pub width: u32,
    pub height: u32,
    pub pixels: &'a [u8],
}

/// Where compositing gets its layer bytes. The borrow lives as long as the
/// input, so a composite reads the committed buffer without copying it.
pub trait RenderInput {
    /// The committed pixels of one layer, or `None` when that layer has no
    /// raster buffer.
    fn layer_pixels(&self, document_id: &str, layer_id: &str) -> Option<RenderLayerPixels<'_>>;
}
