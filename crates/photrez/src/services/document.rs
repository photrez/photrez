//! # document - the demo document, built through `photrez-core`
//!
//! Owns: constructing the in-process document the shell displays today. Its
//! pixels are produced by `photrez-core` primitives - the same region
//! snapshot the display seam will use for partial updates - never by this
//! module or the views.
//!
//! Must not own: rendering (`render/`), UI state (`state.rs`), or any file
//! I/O (the real open/save path arrives with its own work).

use photrez_core::pixel_store::PixelLayer;

/// Width of the frozen captures' document (the New Document dialog's
/// defaults, as driven by `capture-baseline.ts` `createBlankCanvas`).
pub const DEFAULT_DOC_WIDTH: u32 = 800;
/// Height of the frozen captures' document.
pub const DEFAULT_DOC_HEIGHT: u32 = 600;

/// The blank white document the frozen captures were taken with: one opaque
/// white layer at 800x600, the dialog's defaults.
///
/// Returns `(width, height, rgba)` with `rgba.len() == width * height * 4`,
/// snapshotted through [`PixelLayer::snapshot_region`], the owned-region
/// primitive, so the core-to-display seam is exercised by the same API a
/// dirty-rect update will use.
pub fn blank_white_document() -> (u32, u32, Vec<u8>) {
    let (width, height) = (DEFAULT_DOC_WIDTH, DEFAULT_DOC_HEIGHT);
    let white = vec![255u8; width as usize * height as usize * 4];
    let layer = PixelLayer::new(width, height, white);
    let region = layer.snapshot_region(0, 0, width as usize, height as usize);
    (width, height, region)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The demo document is core-produced, not hand-rolled: it carries the
    /// frozen capture's exact shape and an opaque white fill.
    #[test]
    fn blank_document_is_800x600_opaque_white() {
        let (width, height, rgba) = blank_white_document();
        assert_eq!((width, height), (800, 600));
        assert_eq!(rgba, vec![255u8; 800 * 600 * 4]);
    }
}
