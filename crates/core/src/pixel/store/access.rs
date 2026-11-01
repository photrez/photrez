// SPDX-License-Identifier: AGPL-3.0-or-later
//! Registry access: document and layer lifecycle plus the read accessors.

use super::PixelStoreRegistry;
use crate::pixel::document_store::DocumentPixelStore;
use crate::pixel::layer::PixelLayer;
use crate::ptz::content::{LayerType, PtzDocumentContent};
use std::collections::HashSet;

fn validate_layer_pixels(width: u32, height: u32, byte_len: usize) -> Result<(), String> {
    if (width == 0) != (height == 0) {
        return Err("invalid pixel dimensions".to_string());
    }
    let expected = (width as usize)
        .checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| "pixel buffer size overflow".to_string())?;
    if byte_len != expected {
        return Err("pixel buffer size mismatch".to_string());
    }
    Ok(())
}

fn validate_seed_document(
    doc_id: &str,
    document: &DocumentPixelStore,
    content: &PtzDocumentContent,
) -> Result<(), String> {
    if doc_id.is_empty() || content.id != doc_id {
        return Err("document id mismatch".to_string());
    }

    let mut layer_ids = HashSet::with_capacity(content.layers.len());
    let mut raster_ids = HashSet::with_capacity(content.layers.len());
    for layer in &content.layers {
        if layer.id.is_empty() || !layer_ids.insert(layer.id.as_str()) {
            return Err(format!("invalid or duplicate layer id: {}", layer.id));
        }
        if matches!(&layer.layer_type, LayerType::Raster) {
            let pixels = document
                .layers
                .get(&layer.id)
                .ok_or_else(|| format!("pixel layer not initialized: {}", layer.id))?;
            validate_layer_pixels(pixels.width, pixels.height, pixels.pixels.len())?;
            if layer.width != f64::from(pixels.width) || layer.height != f64::from(pixels.height) {
                return Err(format!("pixel dimensions mismatch: {}", layer.id));
            }
            raster_ids.insert(layer.id.as_str());
        }
    }

    if document.layers.len() != raster_ids.len()
        || document
            .layers
            .keys()
            .any(|layer_id| !raster_ids.contains(layer_id.as_str()))
    {
        return Err("pixel layer membership mismatch".to_string());
    }
    Ok(())
}

impl PixelStoreRegistry {
    /// Seed the existing document namespace's engine from validated content.
    pub fn seed_document(
        &mut self,
        doc_id: &str,
        content: PtzDocumentContent,
    ) -> Result<(), String> {
        let document = self
            .docs
            .get(doc_id)
            .ok_or_else(|| format!("document not initialized: {doc_id}"))?;
        validate_seed_document(doc_id, document, &content)?;
        self.docs
            .get_mut(doc_id)
            .expect("validated document namespace exists")
            .history
            .seed_ptz(content);
        Ok(())
    }

    /// Read the canonical typed content seeded into this document's engine.
    pub fn document_content(&self, doc_id: &str) -> Option<&PtzDocumentContent> {
        self.docs
            .get(doc_id)?
            .history
            .replica
            .as_ref()
            .map(|replica| &replica.doc)
    }

    /// Apply one typed command to the document's native history/pixel store.
    ///
    /// This is the native input/undo seam: it exposes command application, not
    /// the raw `DocumentPixelStore` or its layer maps.
    pub fn apply_command(
        &mut self,
        doc_id: &str,
        envelope: crate::wire::command::CommandEnvelope,
    ) -> Result<crate::wire::command::CommandResult, crate::protocol::ProtocolError> {
        let store = self
            .docs
            .get_mut(doc_id)
            .ok_or_else(|| crate::protocol::ProtocolError {
                code: "E_DOCUMENT".to_string(),
                message: format!("document not open: {doc_id}"),
            })?;
        store.apply_command(envelope)
    }

    /// Document created/opened: ensure a (possibly empty) namespace exists so a
    /// later close can release it deterministically.
    pub fn open_document(&mut self, doc_id: &str) {
        self.docs.entry(doc_id.to_string()).or_default();
    }

    /// Document closed: release ALL of its pixel storage.
    pub fn close_document(&mut self, doc_id: &str) {
        self.docs.remove(doc_id);
    }

    /// Seed a layer's canonical buffer; auto-creates the document namespace.
    pub fn add_layer(
        &mut self,
        doc_id: &str,
        layer_id: &str,
        w: u32,
        h: u32,
        bytes: Vec<u8>,
    ) -> Result<(), String> {
        validate_layer_pixels(w, h, bytes.len())?;
        let doc = self.docs.entry(doc_id.to_string()).or_default();
        doc.init_layer(layer_id, w, h, bytes);
        Ok(())
    }

    pub fn remove_layer(&mut self, doc_id: &str, layer_id: &str) {
        if let Some(doc) = self.docs.get_mut(doc_id) {
            doc.remove_layer(layer_id);
        }
    }

    pub fn resize_layer(
        &mut self,
        doc_id: &str,
        layer_id: &str,
        w: u32,
        h: u32,
        bytes: Vec<u8>,
    ) -> Result<(), String> {
        validate_layer_pixels(w, h, bytes.len())?;
        let doc = self.docs.entry(doc_id.to_string()).or_default();
        doc.resize_layer(layer_id, w, h, bytes);
        Ok(())
    }

    pub fn get_layer(&self, doc_id: &str, layer_id: &str) -> Option<&PixelLayer> {
        self.docs.get(doc_id).and_then(|d| d.layers.get(layer_id))
    }

    #[cfg(any(feature = "legacy-host", test))]
    #[cfg_attr(not(feature = "legacy-host"), allow(dead_code))]
    pub fn get_layer_mut(&mut self, doc_id: &str, layer_id: &str) -> Option<&mut PixelLayer> {
        self.get_layer_mut_inner(doc_id, layer_id)
    }

    #[cfg(any(feature = "legacy-host", test))]
    fn get_layer_mut_inner(&mut self, doc_id: &str, layer_id: &str) -> Option<&mut PixelLayer> {
        self.docs.get_mut(doc_id).and_then(|d| {
            // Desync guard: see `DocumentPixelStore::get_layer_mut`.
            d.state_nodes.remove(layer_id);
            d.layers.get_mut(layer_id)
        })
    }

    pub fn get_epoch(&self, doc_id: &str, layer_id: &str) -> Result<u64, String> {
        self.get_layer(doc_id, layer_id)
            .map(|l| l.epoch())
            .ok_or_else(|| format!("layer not initialized: {layer_id}"))
    }
}
