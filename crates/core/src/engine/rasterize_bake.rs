// SPDX-License-Identifier: AGPL-3.0-or-later
//! Pixel half of `RasterizeLayer`, drained by `DocumentPixelStore` right
//! after the command applies — the same seam `CropBakePlan` uses for the
//! destructive crop. The engine owns no pixel buffer, so `apply_rasterize`
//! can only describe the buffer to materialize: the host-rendered bitmap
//! that gives the rasterized parametric layer its canonical pixels.

/// The materialized buffer for one rasterized parametric layer.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RasterizePlan {
    pub(crate) layer_id: String,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) rgba: Vec<u8>,
}
