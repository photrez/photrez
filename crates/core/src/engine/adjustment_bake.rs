//! Pixel half of a basic-adjustment bake (`applyBasicAdjustment`).
//!
//! The metadata half lives in `apply_adjustment` and stays there. This
//! module answers the pixel question: the layer's canonical buffer is
//! re-materialized from the host's baked pre-image, and the before/after
//! `StateNode` pair is attached to the metadata entry so one undo step
//! restores metadata AND pixels.
//!
//! The engine owns no pixel buffer: the plan produced here is drained by
//! `DocumentPixelStore` right after the command is applied.

/// What the store needs to materialize one adjustment bake.
#[derive(Clone, Debug)]
pub(crate) struct ApplyAdjustmentPlan {
    /// Index of the metadata entry this bake belongs to.
    pub entry_index: usize,
    /// The baked layer.
    pub layer_id: String,
    /// Width of the baked bitmap.
    pub width: u32,
    /// Height of the baked bitmap.
    pub height: u32,
    /// Baked RGBA bytes, row-major top-to-bottom.
    pub rgba: Vec<u8>,
}
