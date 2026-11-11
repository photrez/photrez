//! # document - the demo document and the loaded-document save path
//!
//! Owns: constructing the in-process demo document and writing a loaded
//! document back through the approved `FileIo` + `SaveStream` boundary. Pixels
//! are produced by `photrez-core` primitives - the same region snapshot the
//! display seam uses - never by this module or the views.
//!
//! Must not own: rendering (`display/`), UI state (`state.rs`), or direct file
//! access. Project bytes enter only through the approved `FileIo` boundary.

use std::path::Path;

use photrez_core::document::content::{LayerType, PtzDocumentContent};
use photrez_core::pixel::export::encode_image;
use photrez_core::ptz::file::PtzDocument;
use photrez_core::raster::PixelLayer;

use crate::services::error::ServiceError;
use crate::services::file_io::FileIo;
use crate::services::ptz_limits::{self, MAX_LAYER_DIMENSION};
use crate::services::save_stream::{SaveHandle, SaveStream};

mod loader;

use loader::{integer_dimension, resource_limit, validation};
pub use loader::{load_document_candidate, load_document_candidate_with_progress, LoadedDocument};

#[cfg(test)]
mod tests;

/// The demo document size the app seeds at startup.
pub const DEFAULT_DOC_WIDTH: u32 = 800;
/// Height of that same document.
pub const DEFAULT_DOC_HEIGHT: u32 = 600;

/// The demo document `app::startup` seeds when the app opens with a document:
/// one opaque white layer at 800x600, the size
/// `blank_document_is_800x600_opaque_white` pins.
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

impl LoadedDocument {
    pub(crate) fn save_to(
        &self,
        files: &dyn FileIo,
        path: &Path,
        save_stream: &SaveStream,
    ) -> Result<(), ServiceError> {
        let content = self
            .registry
            .document_content(&self.document_id)
            .ok_or_else(|| ServiceError::Internal("loaded document content is missing".into()))?;
        let document = self.live_document(content)?;
        let document_json = document
            .to_json()
            .map_err(|error| validation(format!("cannot serialize document.json: {error}")))?;
        let layers = self.raster_pngs(content)?;

        let target = files.authorize_save_target(path)?;
        let handle = save_stream.begin(target, document_json.as_bytes())?;
        for (layer_id, png) in &layers {
            if let Err(error) = save_stream.write_layer(handle, layer_id, png) {
                return Err(cleanup_failed_save(save_stream, handle, error));
            }
        }
        if let Err(error) = save_stream.end(handle) {
            return Err(cleanup_failed_save(save_stream, handle, error));
        }
        Ok(())
    }

    /// The file envelope for the live canonical content.
    ///
    /// Session fields (`activeLayerId`, `viewport`, `dirty`) are not document
    /// content: preserve the active layer when it still exists, keep the
    /// loaded viewport until the native viewport owns one, and write
    /// `dirty: false` because a successful save is the clean state.
    fn live_document(&self, content: &PtzDocumentContent) -> Result<PtzDocument, ServiceError> {
        if content.id != self.document_id {
            return Err(validation(
                "canonical content id no longer matches the loaded document",
            ));
        }
        if !content.width.is_finite()
            || !content.height.is_finite()
            || content.width <= 0.0
            || content.height <= 0.0
        {
            return Err(validation("canonical document dimensions are invalid"));
        }
        let mut seen = std::collections::HashSet::with_capacity(content.layers.len());
        for layer in &content.layers {
            if layer.id.is_empty() || !seen.insert(layer.id.as_str()) {
                return Err(validation(
                    "canonical layer ids must be non-empty and unique",
                ));
            }
        }
        let original = self.document_snapshot();
        let mut layers = content.layers.clone();
        // `resourceId` is a live store token, not file metadata: the file
        // envelope has no such field, and writing it would change every
        // unmodified round-trip.
        for layer in &mut layers {
            layer.resource_id = None;
        }
        let active_layer_id = original
            .active_layer_id
            .as_ref()
            .filter(|id| layers.iter().any(|layer| &layer.id == *id))
            .cloned();
        Ok(PtzDocument {
            id: content.id.clone(),
            name: content.name.clone(),
            width: content.width,
            height: content.height,
            layers,
            active_layer_id,
            selection: content.selection.clone(),
            viewport: original.viewport.clone(),
            dirty: false,
        })
    }

    /// Encode every raster layer, in content order. Shape and text layers are
    /// carried by `document.json`; adjustment and group layers never reach a
    /// loaded document because the loader refuses them.
    fn raster_pngs(
        &self,
        content: &PtzDocumentContent,
    ) -> Result<Vec<(String, Vec<u8>)>, ServiceError> {
        let mut layers = Vec::new();
        for layer in &content.layers {
            if !matches!(layer.layer_type, LayerType::Raster) {
                continue;
            }
            ptz_limits::validate_layer_id(&layer.id)?;
            let width = integer_dimension(layer.width, MAX_LAYER_DIMENSION, "raster width")?;
            let height = integer_dimension(layer.height, MAX_LAYER_DIMENSION, "raster height")?;
            if (width == 0) != (height == 0) {
                return Err(validation("raster dimensions must both be zero or nonzero"));
            }
            let canonical = self
                .registry
                .get_layer(&self.document_id, &layer.id)
                .ok_or_else(|| validation("canonical raster pixels are missing"))?;
            if (canonical.width, canonical.height) != (width, height) {
                return Err(validation(
                    "canonical pixel dimensions do not match raster metadata",
                ));
            }
            let expected_size =
                ptz_limits::decoded_rgba_bytes(u64::from(width), u64::from(height))?;
            let expected_size = usize::try_from(expected_size)
                .map_err(|_| resource_limit("canonical raster size does not fit in memory"))?;
            if canonical.pixels.len() != expected_size {
                return Err(validation(
                    "canonical raster byte count does not match its dimensions",
                ));
            }
            if width == 0 {
                continue;
            }
            let png = encode_image(width, height, &canonical.pixels, "png", 100)
                .map_err(|error| validation(format!("cannot encode raster PNG: {error}")))?;
            layers.push((layer.id.clone(), png));
        }
        Ok(layers)
    }
}

fn cleanup_failed_save(
    save_stream: &SaveStream,
    handle: SaveHandle,
    save_error: ServiceError,
) -> ServiceError {
    match save_stream.cancel(handle) {
        Ok(()) => save_error,
        Err(ServiceError::Conflict(_)) => save_error,
        Err(cleanup_error) => cleanup_error,
    }
}
