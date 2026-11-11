//! Which page images paint, in what order.

use super::*;

/// The page content images in paint order: base tiles plus the viewport
/// projection, when one exists. Base tiles that the projection covers are
/// replaced by it: the projected pixels already include every visible layer,
/// so painting the stale tile under it would let stale alpha show through
/// transparent projection pixels. Tiles outside the projection still paint,
/// which covers a pan into a region whose projection has not landed yet.
pub(super) fn base_images(canvas_image: &CanvasImage) -> Vec<(TileRect, Arc<RenderImage>)> {
    let projection = canvas_image.projection.as_ref();
    let mut images: Vec<(TileRect, Arc<RenderImage>)> = canvas_image
        .tiles
        .iter()
        .filter(|tile| {
            projection.is_none_or(|projection| !rects_intersect(projection.rect, tile.rect))
        })
        .map(|tile| (tile.rect, Arc::clone(&tile.image)))
        .collect();
    if let Some(projection) = projection {
        images.push((projection.rect, Arc::clone(&projection.image)));
    }
    images
}

#[allow(dead_code)]
pub(super) fn display_images(canvas_image: &CanvasImage) -> Vec<(TileRect, Arc<RenderImage>)> {
    let mut images = base_images(canvas_image);
    images.extend(
        canvas_image
            .patches
            .iter()
            .map(|patch| (patch.rect, Arc::clone(&patch.image))),
    );
    images.extend(
        canvas_image
            .preview_patches
            .iter()
            .map(|patch| (patch.rect, Arc::clone(&patch.image))),
    );
    images
}

pub(super) fn rects_intersect(a: TileRect, b: TileRect) -> bool {
    a.x < b.x.saturating_add(b.width)
        && b.x < a.x.saturating_add(a.width)
        && a.y < b.y.saturating_add(b.height)
        && b.y < a.y.saturating_add(a.height)
}
