//! Pixel work for Crop Apply deletion and Background fill.
//!
//! The crop arm's metadata math lives in `canvas.rs` and stays there. This
//! module carries the raster layers and post-crop transforms that the pixel
//! store bakes onto the new document grid.
//!
//! The engine owns no pixel buffer: the plan produced here is drained by
//! `DocumentPixelStore` right after the command is applied.

/// Layer placement after the crop, in the compositor's own convention
/// (`render::compositor::composite_transformed`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct CropLayerPlacement {
    pub x: f64,
    pub y: f64,
    pub scale_x: f64,
    pub scale_y: f64,
    pub rotation: f64,
    pub flip_h: bool,
    pub flip_v: bool,
}

/// What the store needs to bake one layer.
#[derive(Clone, Debug)]
pub(crate) struct CropBakeLayer {
    pub layer_id: String,
    pub placement: CropLayerPlacement,
    pub adjustment: Option<crate::document::layer_params::BasicAdjustment>,
}

/// The pending pixel half of one crop command.
#[derive(Clone, Debug)]
pub(crate) struct CropBakePlan {
    /// Sequence of the metadata entry this bake belongs to.
    pub entry_seq: u64,
    /// New document size.
    pub doc_width: f64,
    pub doc_height: f64,
    pub layers: Vec<CropBakeLayer>,
    /// `fillBackgroundColor`: paint this colour under the Background layer.
    pub fill: Option<FillBakePlan>,
}

/// The fill half of a destructive crop: a solid colour baked UNDER the
/// Background layer, producing a new document-sized Background bitmap.
#[derive(Clone, Debug)]
pub(crate) struct FillBakePlan {
    pub layer_id: String,
    /// The crop creates a new Background when the document has none.
    pub create_background: bool,
    /// Post-crop placement used to sample the Background before its bitmap is reset.
    pub placement: CropLayerPlacement,
    pub width: u32,
    pub height: u32,
    pub adjustment: Option<crate::document::layer_params::BasicAdjustment>,
    pub visible: bool,
    pub opacity: f64,
    pub blend_mode: Option<crate::document::content::BlendMode>,
    pub color: [u8; 4],
}

/// Composite an already-rasterized Background over the fill color.
pub(crate) fn composite_layer_over_fill(
    mut source: Vec<u8>,
    doc_width: u32,
    doc_height: u32,
    fill: [u8; 4],
    visible: bool,
    opacity: f64,
    blend_mode: Option<&crate::document::content::BlendMode>,
) -> Vec<u8> {
    if source.is_empty() {
        source.resize((doc_width as usize * doc_height as usize) * 4, 0);
        for pixel in source.as_chunks_mut::<4>().0 {
            pixel.copy_from_slice(&fill);
        }
        return source;
    }
    let normal = crate::document::content::BlendMode::Normal;
    let blend_mode = blend_mode.unwrap_or(&normal);
    for dst in source.as_chunks_mut::<4>().0 {
        let src = [dst[0], dst[1], dst[2], dst[3]];
        if !visible {
            dst.copy_from_slice(&fill);
            continue;
        }
        let blended =
            crate::render::compositor::composite_rgba_over(fill, src, opacity, blend_mode);
        dst.copy_from_slice(&blended);
    }
    source
}
