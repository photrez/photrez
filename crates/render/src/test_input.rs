// SPDX-License-Identifier: AGPL-3.0-or-later
//! Map-backed [`RenderInput`] for the compositing tests.
//!
//! The compositor reads pixels through the trait, so a test of blend modes or
//! transforms does not need a document store. The store-backed path keeps its own
//! proof in `photrez-core`:
//! `engine::tests::structural::rasterized_layers_composite_from_the_store`.

use crate::input::{RenderInput, RenderLayerPixels};
use std::collections::HashMap;

/// One layer's committed buffer: width, height, then `width*height*4` RGBA bytes.
type LayerBuffer = (u32, u32, Vec<u8>);

/// The layers of one document, keyed by layer id.
type Layers = HashMap<String, LayerBuffer>;

#[derive(Default)]
pub(crate) struct MapInput {
    documents: HashMap<String, Layers>,
}

impl MapInput {
    pub(crate) fn open_document(&mut self, doc_id: &str) {
        self.documents.entry(doc_id.to_string()).or_default();
    }

    /// Mirrors the store's rule that a buffer is exactly `width*height*4` RGBA
    /// bytes, so a test that seeds a wrong-sized buffer still fails here rather
    /// than silently compositing a short row.
    pub(crate) fn add_layer(
        &mut self,
        doc_id: &str,
        layer_id: &str,
        width: u32,
        height: u32,
        bytes: Vec<u8>,
    ) -> Result<(), String> {
        let expected = width as usize * height as usize * 4;
        if bytes.len() != expected {
            return Err(format!(
                "layer {layer_id} buffer is {} bytes, expected {expected}",
                bytes.len()
            ));
        }
        self.documents
            .entry(doc_id.to_string())
            .or_default()
            .insert(layer_id.to_string(), (width, height, bytes));
        Ok(())
    }
}

impl RenderInput for MapInput {
    fn layer_pixels(&self, document_id: &str, layer_id: &str) -> Option<RenderLayerPixels<'_>> {
        let (width, height, pixels) = self.documents.get(document_id)?.get(layer_id)?;
        Some(RenderLayerPixels {
            width: *width,
            height: *height,
            pixels: pixels.as_slice(),
        })
    }
}
