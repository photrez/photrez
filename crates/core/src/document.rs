// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::layers::Layer;
use crate::selection::SelectionRect;
use crate::transform::Transform;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Document {
    pub id: String,
    pub width: u32,
    pub height: u32,
    pub layers: Vec<Layer>,
    pub selection: Option<SelectionRect>,
    pub dirty_layers: HashSet<String>,
}

// 1 GB — must stay in sync with the TS source of truth:
// apps/desktop/src/engine/types.ts (MAX_PIXEL_BUDGET). The TypeScript engine
// is the runtime source of truth; this crate is a WASM-only helper.
pub const MAX_PIXEL_BUDGET: usize = 1024 * 1024 * 1024;

impl Document {
    pub fn new(id: String, width: u32, height: u32) -> Self {
        Self {
            id,
            width,
            height,
            layers: Vec::new(),
            selection: None,
            dirty_layers: HashSet::new(),
        }
    }

    pub fn calculate_memory_usage(&self) -> usize {
        self.layers
            .iter()
            .map(|l| (l.width * l.height * 4) as usize)
            .sum()
    }

    pub fn add_layer_safe(&mut self, layer: Layer) -> Result<(), String> {
        let additional_bytes = (layer.width * layer.height * 4) as usize;
        let current_bytes = self.calculate_memory_usage();
        if current_bytes + additional_bytes > MAX_PIXEL_BUDGET {
            return Err(
                "E_RESOURCE_LIMIT: Document memory exceeds max pixel budget of 1GB".to_string(),
            );
        }
        // Insert at index 0 (top of the layer stack) to match high-fidelity design standards
        let layer_id = layer.id.clone();
        self.layers.insert(0, layer);
        self.dirty_layers.insert(layer_id);
        Ok(())
    }

    pub fn add_layer(&mut self, layer: Layer) {
        let _ = self.add_layer_safe(layer);
    }

    pub fn delete_layer(&mut self, id: &str) -> Result<Layer, String> {
        let index = self
            .layers
            .iter()
            .position(|l| l.id == id)
            .ok_or_else(|| format!("Layer with id '{}' not found", id))?;
        let removed = self.layers.remove(index);
        self.dirty_layers.insert(id.to_string());
        Ok(removed)
    }

    pub fn reorder_layer(&mut self, from_idx: usize, to_idx: usize) -> Result<(), String> {
        if from_idx >= self.layers.len() || to_idx >= self.layers.len() {
            return Err("Index out of bounds".to_string());
        }
        let layer = self.layers.remove(from_idx);
        self.layers.insert(to_idx, layer);
        Ok(())
    }

    pub fn update_layer_properties(
        &mut self,
        id: &str,
        opacity: Option<f32>,
        visible: Option<bool>,
        locked: Option<bool>,
        name: Option<String>,
        blend_mode: Option<String>,
    ) -> Result<(), String> {
        let layer = self
            .layers
            .iter_mut()
            .find(|l| l.id == id)
            .ok_or_else(|| format!("Layer with id '{}' not found", id))?;

        if let Some(o) = opacity {
            layer.opacity = o.clamp(0.0, 1.0);
        }
        if let Some(v) = visible {
            layer.visible = v;
        }
        if let Some(l) = locked {
            layer.locked = l;
        }
        if let Some(n) = name {
            layer.name = n;
        }
        if let Some(bm) = blend_mode {
            layer.blend_mode = bm;
        }
        self.dirty_layers.insert(id.to_string());
        Ok(())
    }

    // â”€â”€ Selection Operations â”€â”€

    pub fn create_selection(&mut self, x: f32, y: f32, width: f32, height: f32) {
        let rect = SelectionRect {
            x,
            y,
            width,
            height,
        }
        .normalize();
        let clamped = rect.clamp_to_canvas(self.width, self.height);
        self.selection = Some(clamped);
    }

    pub fn clear_selection(&mut self) {
        self.selection = None;
    }

    pub fn select_all(&mut self) {
        let rect = SelectionRect {
            x: 0.0,
            y: 0.0,
            width: self.width as f32,
            height: self.height as f32,
        };
        self.selection = Some(rect);
    }

    pub fn move_selection(&mut self, dx: f32, dy: f32) -> Option<SelectionRect> {
        let rect = self.selection.as_ref()?;
        let moved = rect.translate(dx, dy);
        let clamped = moved.clamp_to_canvas(self.width, self.height);
        self.selection = Some(clamped);
        self.selection
    }

    // â”€â”€ Layer Transform Operations â”€â”€

    pub fn move_layer(&mut self, id: &str, x: f32, y: f32) -> Result<(), String> {
        let layer = self
            .layers
            .iter_mut()
            .find(|l| l.id == id)
            .ok_or_else(|| format!("Layer with id '{}' not found", id))?;
        layer.x = x;
        layer.y = y;
        self.dirty_layers.insert(id.to_string());
        Ok(())
    }

    pub fn apply_transform(&mut self, id: &str, transform: Transform) -> Result<(), String> {
        let layer = self
            .layers
            .iter_mut()
            .find(|l| l.id == id)
            .ok_or_else(|| format!("Layer with id '{}' not found", id))?;
        layer.transform = transform;
        self.dirty_layers.insert(id.to_string());
        Ok(())
    }

    pub fn mark_dirty(&mut self, layer_id: &str) {
        self.dirty_layers.insert(layer_id.to_string());
    }

    pub fn clear_dirty(&mut self) {
        self.dirty_layers.clear();
    }

    pub fn has_dirty_layers(&self) -> bool {
        !self.dirty_layers.is_empty()
    }

    pub fn get_layer_transform(&self, id: &str) -> Option<Transform> {
        self.layers.iter().find(|l| l.id == id).map(|l| l.transform)
    }

    pub fn get_layer_by_id(&self, id: &str) -> Option<&Layer> {
        self.layers.iter().find(|l| l.id == id)
    }

    pub fn crop_canvas(&mut self, x: f32, y: f32, width: u32, height: u32) -> Result<(), String> {
        if width == 0 || height == 0 {
            return Err("Crop dimensions must be greater than zero".to_string());
        }
        self.width = width;
        self.height = height;
        for layer in self.layers.iter_mut() {
            layer.x -= x;
            layer.y -= y;
        }
        self.selection = None; // Clear selection after crop
        Ok(())
    }

    pub fn resize_canvas(&mut self, width: u32, height: u32) -> Result<(), String> {
        if width == 0 || height == 0 {
            return Err("Canvas dimensions must be greater than zero".to_string());
        }
        self.width = width;
        self.height = height;
        Ok(())
    }

    pub fn load_image_from_bytes(&mut self, bytes: Vec<u8>, name: String) -> Result<(), String> {
        let img = image::load_from_memory(&bytes)
            .map_err(|e| format!("Failed to decode image: {}", e))?;

        let rgba = img.to_rgba8();
        let (width, height) = rgba.dimensions();

        // Enforce resource limits after decode
        let pixel_bytes = width as usize * height as usize * 4;
        if pixel_bytes > MAX_PIXEL_BUDGET {
            return Err(format!(
                "E_RESOURCE_LIMIT: Image too large ({}x{} exceeds memory budget)",
                width, height
            ));
        }

        let pixel_data = rgba.into_raw();

        let bitmap = crate::layers::BitmapData {
            width,
            height,
            format: crate::layers::PixelFormat::RGBA8,
            pixel_data,
        };

        let mut layer = Layer::new(
            format!("layer-{}", uuid::Uuid::new_v4()),
            name,
            width,
            height,
        );
        layer.bitmap_ref = bitmap;

        self.width = width;
        self.height = height;
        self.add_layer(layer);
        Ok(())
    }

    pub fn get_flattened_pixels(&self) -> (u32, u32, Vec<u8>) {
        let mut pixels = vec![0u8; (self.width * self.height * 4) as usize];

        for layer in self.layers.iter().rev() {
            if !layer.visible || layer.opacity <= 0.0 {
                continue;
            }

            let lx = layer.x as i32;
            let ly = layer.y as i32;

            for y in 0..self.height {
                for x in 0..self.width {
                    let screen_idx = ((y * self.width + x) * 4) as usize;
                    let layer_x = x as i32 - lx;
                    let layer_y = y as i32 - ly;

                    if layer_x >= 0
                        && layer_x < layer.width as i32
                        && layer_y >= 0
                        && layer_y < layer.height as i32
                    {
                        let layer_idx =
                            ((layer_y as u32 * layer.width + layer_x as u32) * 4) as usize;

                        if layer_idx + 3 < layer.bitmap_ref.pixel_data.len() {
                            let sr = layer.bitmap_ref.pixel_data[layer_idx] as f32 / 255.0;
                            let sg = layer.bitmap_ref.pixel_data[layer_idx + 1] as f32 / 255.0;
                            let sb = layer.bitmap_ref.pixel_data[layer_idx + 2] as f32 / 255.0;
                            let sa = layer.bitmap_ref.pixel_data[layer_idx + 3] as f32 / 255.0;

                            let da = pixels[screen_idx + 3] as f32 / 255.0;
                            let alpha = sa * layer.opacity;

                            let out_a = alpha + da * (1.0 - alpha);
                            if out_a > 0.0 {
                                pixels[screen_idx] =
                                    ((sr * alpha + pixels[screen_idx] as f32 * da * (1.0 - alpha))
                                        / out_a
                                        * 255.0) as u8;
                                pixels[screen_idx + 1] = ((sg * alpha
                                    + pixels[screen_idx + 1] as f32 * da * (1.0 - alpha))
                                    / out_a
                                    * 255.0)
                                    as u8;
                                pixels[screen_idx + 2] = ((sb * alpha
                                    + pixels[screen_idx + 2] as f32 * da * (1.0 - alpha))
                                    / out_a
                                    * 255.0)
                                    as u8;
                                pixels[screen_idx + 3] = (out_a * 255.0) as u8;
                            }
                        }
                    }
                }
            }
        }

        (self.width, self.height, pixels)
    }

    pub fn sample_pixel(&self, x: f32, y: f32) -> [u8; 4] {
        let ix = x.floor() as i32;
        let iy = y.floor() as i32;

        if ix < 0 || ix >= self.width as i32 || iy < 0 || iy >= self.height as i32 {
            return [0, 0, 0, 0]; // Transparent black out of bounds
        }

        let mut blended = [0f32; 4];

        // Reverse order: blend bottom (N-1) to top (0)
        for layer in self.layers.iter().rev() {
            if !layer.visible {
                continue;
            }

            let lx = ix - layer.x as i32;
            let ly = iy - layer.y as i32;

            if lx >= 0 && lx < layer.width as i32 && ly >= 0 && ly < layer.height as i32 {
                let idx = ((ly * layer.width as i32 + lx) * 4) as usize;
                let src_r = layer.bitmap_ref.pixel_data[idx] as f32 / 255.0;
                let src_g = layer.bitmap_ref.pixel_data[idx + 1] as f32 / 255.0;
                let src_b = layer.bitmap_ref.pixel_data[idx + 2] as f32 / 255.0;
                let src_a = layer.bitmap_ref.pixel_data[idx + 3] as f32 / 255.0 * layer.opacity;

                if src_a <= 0.0 {
                    continue;
                }

                let dest_r = blended[0];
                let dest_g = blended[1];
                let dest_b = blended[2];
                let dest_a = blended[3];

                let out_a = src_a + dest_a * (1.0 - src_a);
                let out_r = if out_a > 0.0 {
                    (src_r * src_a + dest_r * dest_a * (1.0 - src_a)) / out_a
                } else {
                    0.0
                };
                let out_g = if out_a > 0.0 {
                    (src_g * src_a + dest_g * dest_a * (1.0 - src_a)) / out_a
                } else {
                    0.0
                };
                let out_b = if out_a > 0.0 {
                    (src_b * src_a + dest_b * dest_a * (1.0 - src_a)) / out_a
                } else {
                    0.0
                };

                blended = [out_r, out_g, out_b, out_a];
            }
        }

        [
            (blended[0] * 255.0).round().min(255.0) as u8,
            (blended[1] * 255.0).round().min(255.0) as u8,
            (blended[2] * 255.0).round().min(255.0) as u8,
            (blended[3] * 255.0).round().min(255.0) as u8,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_document() {
        let doc = Document::new("doc-123".to_string(), 1920, 1080);
        assert_eq!(doc.id, "doc-123");
        assert_eq!(doc.width, 1920);
        assert_eq!(doc.height, 1080);
        assert_eq!(doc.layers.len(), 0);
    }

    #[test]
    fn test_layer_operations() {
        let mut doc = Document::new("doc-1".to_string(), 1000, 1000);
        let l1 = Layer::new("l-1".to_string(), "Layer 1".to_string(), 500, 500);
        let l2 = Layer::new("l-2".to_string(), "Layer 2".to_string(), 500, 500);

        doc.add_layer(l1);
        doc.add_layer(l2);

        // Since we insert at 0, "l-2" should be first (top of stack)
        assert_eq!(doc.layers[0].id, "l-2");
        assert_eq!(doc.layers[1].id, "l-1");

        // Test update properties
        let update_res = doc.update_layer_properties(
            "l-2",
            Some(0.5),
            Some(false),
            Some(true),
            Some("New Layer 2 Name".to_string()),
            Some("multiply".to_string()),
        );
        assert!(update_res.is_ok());
        assert_eq!(doc.layers[0].opacity, 0.5);
        assert!(!doc.layers[0].visible);
        assert!(doc.layers[0].locked);
        assert_eq!(doc.layers[0].name, "New Layer 2 Name");
        assert_eq!(doc.layers[0].blend_mode, "multiply");

        // Test reorder layers
        let reorder_res = doc.reorder_layer(0, 1);
        assert!(reorder_res.is_ok());
        assert_eq!(doc.layers[0].id, "l-1");
        assert_eq!(doc.layers[1].id, "l-2");

        // Test delete layer
        let delete_res = doc.delete_layer("l-1");
        assert!(delete_res.is_ok());
        assert_eq!(doc.layers.len(), 1);
        assert_eq!(doc.layers[0].id, "l-2");
    }

    #[test]
    fn test_memory_budget_under_limit() {
        let mut doc = Document::new("doc-budget-ok".to_string(), 1920, 1080);
        let l1 = Layer::new("layer-ok-1".to_string(), "Layer 1".to_string(), 800, 600);
        let l2 = Layer::new("layer-ok-2".to_string(), "Layer 2".to_string(), 800, 600);

        let r1 = doc.add_layer_safe(l1);
        let r2 = doc.add_layer_safe(l2);

        assert!(r1.is_ok());
        assert!(r2.is_ok());
        assert_eq!(doc.calculate_memory_usage(), (800 * 600 * 4 * 2) as usize);
    }

    #[test]
    fn test_memory_budget_over_limit() {
        let mut doc = Document::new("doc-budget-fail".to_string(), 1000, 1000);
        // 20000 x 16000 x 4 = 1.28 GB > MAX_PIXEL_BUDGET (1 GB)
        let huge_layer = Layer::new("layer-huge".to_string(), "Huge".to_string(), 20000, 16000);

        let res = doc.add_layer_safe(huge_layer);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("E_RESOURCE_LIMIT"));
    }

    #[test]
    fn test_selection_operations() {
        let mut doc = Document::new("doc-sel".to_string(), 800, 600);
        assert!(doc.selection.is_none());

        doc.create_selection(100.0, 50.0, 300.0, 200.0);
        assert!(doc.selection.is_some());
        let sel = doc.selection.unwrap();
        assert_eq!(sel.x, 100.0);
        assert_eq!(sel.y, 50.0);

        doc.clear_selection();
        assert!(doc.selection.is_none());
    }

    #[test]
    fn test_select_all() {
        let mut doc = Document::new("doc-all".to_string(), 800, 600);
        doc.select_all();
        let sel = doc.selection.unwrap();
        assert_eq!(sel.width, 800.0);
        assert_eq!(sel.height, 600.0);
    }

    #[test]
    fn test_move_layer() {
        let mut doc = Document::new("doc-move".to_string(), 1000, 1000);
        let l1 = Layer::new("l-1".to_string(), "Layer 1".to_string(), 500, 500);
        doc.add_layer(l1);

        let res = doc.move_layer("l-1", 100.0, 200.0);
        assert!(res.is_ok());
        assert_eq!(doc.layers[0].x, 100.0);
        assert_eq!(doc.layers[0].y, 200.0);
    }

    #[test]
    fn test_crop_canvas() {
        let mut doc = Document::new("doc-crop".to_string(), 800, 600);
        let l1 = Layer::new("l-1".to_string(), "Layer 1".to_string(), 100, 100);
        doc.add_layer(l1);
        doc.layers[0].x = 50.0;
        doc.layers[0].y = 50.0;

        let res = doc.crop_canvas(10.0, 10.0, 500, 400);
        assert!(res.is_ok());
        assert_eq!(doc.width, 500);
        assert_eq!(doc.height, 400);
        assert_eq!(doc.layers[0].x, 40.0);
        assert_eq!(doc.layers[0].y, 40.0);
    }

    #[test]
    fn test_resize_canvas() {
        let mut doc = Document::new("doc-resize".to_string(), 800, 600);
        let res = doc.resize_canvas(1024, 768);
        assert!(res.is_ok());
        assert_eq!(doc.width, 1024);
        assert_eq!(doc.height, 768);
    }

    #[test]
    fn test_sample_pixel() {
        let mut doc = Document::new("doc-1".to_string(), 2, 2);

        let mut l1 = Layer::new("l-1".to_string(), "Layer 1".to_string(), 2, 2);
        l1.bitmap_ref.pixel_data = vec![
            255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255,
        ];
        doc.add_layer(l1);

        let color = doc.sample_pixel(1.0, 1.0);
        assert_eq!(color, [255, 0, 0, 255]); // Sampled Red Background color
    }

    #[test]
    fn test_apply_transform() {
        let mut doc = Document::new("doc-1".to_string(), 100, 100);
        let layer = Layer::new("layer-1".to_string(), "Test".to_string(), 100, 100);
        doc.add_layer(layer);

        let transform = Transform {
            scale_x: 2.0,
            scale_y: 2.0,
            rotation: 45.0,
            flip_h: true,
            flip_v: false,
        };

        let result = doc.apply_transform("layer-1", transform);
        assert!(result.is_ok());

        let t = doc.get_layer_transform("layer-1").unwrap();
        assert_eq!(t.scale_x, 2.0);
        assert_eq!(t.scale_y, 2.0);
        assert_eq!(t.rotation, 45.0);
        assert!(t.flip_h);
        assert!(!t.flip_v);
    }

    #[test]
    fn test_apply_transform_nonexistent_layer() {
        let mut doc = Document::new("doc-1".to_string(), 100, 100);
        let transform = Transform::new();
        let result = doc.apply_transform("nonexistent", transform);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_layer_transform_nonexistent() {
        let doc = Document::new("doc-1".to_string(), 100, 100);
        let result = doc.get_layer_transform("nonexistent");
        assert!(result.is_none());
    }

    #[test]
    fn test_delete_layer_nonexistent() {
        let mut doc = Document::new("doc-1".to_string(), 100, 100);
        let layer = Layer::new("layer-1".to_string(), "Test".to_string(), 100, 100);
        doc.add_layer(layer);
        let result = doc.delete_layer("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_reorder_layer_out_of_bounds() {
        let mut doc = Document::new("doc-1".to_string(), 100, 100);
        let layer = Layer::new("layer-1".to_string(), "Test".to_string(), 100, 100);
        doc.add_layer(layer);
        let result = doc.reorder_layer(0, 999);
        assert!(result.is_err());
    }

    #[test]
    fn test_update_layer_nonexistent() {
        let mut doc = Document::new("doc-1".to_string(), 100, 100);
        let result = doc.update_layer_properties("nonexistent", Some(0.5), None, None, None, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_move_layer_nonexistent() {
        let mut doc = Document::new("doc-1".to_string(), 100, 100);
        let result = doc.move_layer("nonexistent", 50.0, 50.0);
        assert!(result.is_err());
    }

    #[test]
    fn test_crop_canvas_zero_dimensions() {
        let mut doc = Document::new("doc-1".to_string(), 100, 100);
        let layer = Layer::new("layer-1".to_string(), "Test".to_string(), 100, 100);
        doc.add_layer(layer);
        let result = doc.crop_canvas(0.0, 0.0, 0, 0);
        assert!(result.is_err());
    }

    #[test]
    fn test_resize_canvas_zero_dimensions() {
        let mut doc = Document::new("doc-1".to_string(), 100, 100);
        let result = doc.resize_canvas(0, 0);
        assert!(result.is_err());
    }

    // === CONTRACT TESTS: Verify command behavior matches expected data shapes ===

    #[test]
    fn test_contract_add_layer_returns_document_with_new_layer() {
        let mut doc = Document::new("doc-1".to_string(), 800, 600);
        let initial_count = doc.layers.len();
        let layer = Layer::new("layer-new".to_string(), "New Layer".to_string(), 800, 600);
        doc.add_layer(layer);
        assert_eq!(doc.layers.len(), initial_count + 1);
        assert_eq!(doc.layers[0].name, "New Layer");
    }

    #[test]
    fn test_contract_delete_layer_removes_from_document() {
        let mut doc = Document::new("doc-1".to_string(), 800, 600);
        let bg = Layer::new("bg".to_string(), "Background".to_string(), 800, 600);
        doc.add_layer(bg);
        let layer = Layer::new("layer-del".to_string(), "ToDelete".to_string(), 800, 600);
        doc.add_layer(layer);
        assert_eq!(doc.layers.len(), 2);
        let result = doc.delete_layer("layer-del");
        assert!(result.is_ok());
        assert_eq!(doc.layers.len(), 1);
    }

    #[test]
    fn test_contract_delete_nonexistent_layer_fails() {
        let mut doc = Document::new("doc-1".to_string(), 800, 600);
        let layer = Layer::new("layer-1".to_string(), "Only".to_string(), 800, 600);
        doc.add_layer(layer);
        let result = doc.delete_layer("does-not-exist");
        assert!(result.is_err());
    }

    #[test]
    fn test_contract_reorder_layer_changes_order() {
        let mut doc = Document::new("doc-1".to_string(), 800, 600);
        let l1 = Layer::new("l1".to_string(), "Layer 1".to_string(), 800, 600);
        let l2 = Layer::new("l2".to_string(), "Layer 2".to_string(), 800, 600);
        doc.add_layer(l1);
        doc.add_layer(l2);
        let result = doc.reorder_layer(0, 1);
        assert!(result.is_ok());
        assert_eq!(doc.layers[0].id, "l1");
        assert_eq!(doc.layers[1].id, "l2");
    }

    #[test]
    fn test_contract_update_layer_modifies_properties() {
        let mut doc = Document::new("doc-1".to_string(), 800, 600);
        let layer = Layer::new("layer-1".to_string(), "Test".to_string(), 800, 600);
        doc.add_layer(layer);
        let result = doc.update_layer_properties(
            "layer-1",
            Some(0.5),
            Some(false),
            Some(true),
            Some("Renamed".to_string()),
            None,
        );
        assert!(result.is_ok());
        let updated = doc.layers.iter().find(|l| l.id == "layer-1").unwrap();
        assert_eq!(updated.opacity, 0.5);
        assert!(!updated.visible);
        assert!(updated.locked);
        assert_eq!(updated.name, "Renamed");
    }

    #[test]
    fn test_contract_move_layer_updates_position() {
        let mut doc = Document::new("doc-1".to_string(), 800, 600);
        let layer = Layer::new("layer-1".to_string(), "Test".to_string(), 800, 600);
        doc.add_layer(layer);
        let result = doc.move_layer("layer-1", 100.0, 200.0);
        assert!(result.is_ok());
        let moved = doc.layers.iter().find(|l| l.id == "layer-1").unwrap();
        assert_eq!(moved.x, 100.0);
        assert_eq!(moved.y, 200.0);
    }

    #[test]
    fn test_contract_transform_layer_applies_transform() {
        let mut doc = Document::new("doc-1".to_string(), 800, 600);
        let layer = Layer::new("layer-1".to_string(), "Test".to_string(), 800, 600);
        doc.add_layer(layer);
        let transform = Transform {
            scale_x: 1.5,
            scale_y: 1.5,
            rotation: 90.0,
            flip_h: true,
            flip_v: false,
        };
        let result = doc.apply_transform("layer-1", transform);
        assert!(result.is_ok());
        let t = doc.get_layer_transform("layer-1").unwrap();
        assert_eq!(t.scale_x, 1.5);
        assert_eq!(t.rotation, 90.0);
        assert!(t.flip_h);
    }

    #[test]
    fn test_contract_create_selection_stores_selection() {
        let mut doc = Document::new("doc-1".to_string(), 800, 600);
        assert!(doc.selection.is_none());
        doc.create_selection(10.0, 20.0, 100.0, 50.0);
        assert!(doc.selection.is_some());
        let sel = doc.selection.as_ref().unwrap();
        assert_eq!(sel.x, 10.0);
        assert_eq!(sel.y, 20.0);
        assert_eq!(sel.width, 100.0);
        assert_eq!(sel.height, 50.0);
    }

    #[test]
    fn test_contract_clear_selection_removes_selection() {
        let mut doc = Document::new("doc-1".to_string(), 800, 600);
        doc.create_selection(10.0, 20.0, 100.0, 50.0);
        assert!(doc.selection.is_some());
        doc.clear_selection();
        assert!(doc.selection.is_none());
    }

    #[test]
    fn test_contract_select_all_covers_canvas() {
        let mut doc = Document::new("doc-1".to_string(), 800, 600);
        doc.select_all();
        let sel = doc.selection.as_ref().unwrap();
        assert_eq!(sel.x, 0.0);
        assert_eq!(sel.y, 0.0);
        assert_eq!(sel.width, 800.0);
        assert_eq!(sel.height, 600.0);
    }

    #[test]
    fn test_contract_crop_canvas_updates_dimensions() {
        let mut doc = Document::new("doc-1".to_string(), 800, 600);
        let layer = Layer::new("layer-1".to_string(), "Test".to_string(), 800, 600);
        doc.add_layer(layer);
        let result = doc.crop_canvas(10.0, 10.0, 400, 300);
        assert!(result.is_ok());
        assert_eq!(doc.width, 400);
        assert_eq!(doc.height, 300);
    }

    #[test]
    fn test_contract_resize_canvas_updates_dimensions() {
        let mut doc = Document::new("doc-1".to_string(), 800, 600);
        let result = doc.resize_canvas(1920, 1080);
        assert!(result.is_ok());
        assert_eq!(doc.width, 1920);
        assert_eq!(doc.height, 1080);
    }

    #[test]
    fn test_contract_sample_pixel_returns_color() {
        let mut doc = Document::new("doc-1".to_string(), 10, 10);
        let mut layer = Layer::new("layer-1".to_string(), "Test".to_string(), 10, 10);
        layer.bitmap_ref.pixel_data = vec![0u8; 10 * 10 * 4];
        let idx = (5 * 10 + 5) * 4;
        layer.bitmap_ref.pixel_data[idx] = 255;
        layer.bitmap_ref.pixel_data[idx + 1] = 0;
        layer.bitmap_ref.pixel_data[idx + 2] = 0;
        layer.bitmap_ref.pixel_data[idx + 3] = 255;
        doc.add_layer(layer);

        let color = doc.sample_pixel(5.0, 5.0);
        assert_eq!(color[0], 255);
        assert_eq!(color[1], 0);
        assert_eq!(color[2], 0);
        assert_eq!(color[3], 255);
    }

    // === FAILURE-PATH TESTS: Error conditions and invalid inputs ===

    #[test]
    fn test_failure_transform_zero_scale_x() {
        let mut doc = Document::new("doc-1".to_string(), 100, 100);
        let layer = Layer::new("layer-1".to_string(), "Test".to_string(), 100, 100);
        doc.add_layer(layer);
        let transform = Transform {
            scale_x: 0.0,
            scale_y: 1.0,
            rotation: 0.0,
            flip_h: false,
            flip_v: false,
        };
        let result = doc.apply_transform("layer-1", transform);
        assert!(result.is_ok());
    }

    #[test]
    fn test_failure_update_layer_nonexistent_id() {
        let mut doc = Document::new("doc-1".to_string(), 100, 100);
        let result =
            doc.update_layer_properties("does-not-exist", Some(0.5), None, None, None, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[test]
    fn test_failure_delete_layer_nonexistent_id() {
        let mut doc = Document::new("doc-1".to_string(), 100, 100);
        let layer = Layer::new("layer-1".to_string(), "Test".to_string(), 100, 100);
        doc.add_layer(layer);
        let result = doc.delete_layer("does-not-exist");
        assert!(result.is_err());
    }

    #[test]
    fn test_failure_reorder_layer_invalid_from_index() {
        let mut doc = Document::new("doc-1".to_string(), 100, 100);
        let layer = Layer::new("layer-1".to_string(), "Test".to_string(), 100, 100);
        doc.add_layer(layer);
        let result = doc.reorder_layer(999, 0);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Index out of bounds"));
    }

    #[test]
    fn test_failure_reorder_layer_invalid_to_index() {
        let mut doc = Document::new("doc-1".to_string(), 100, 100);
        let layer = Layer::new("layer-1".to_string(), "Test".to_string(), 100, 100);
        doc.add_layer(layer);
        let result = doc.reorder_layer(0, 999);
        assert!(result.is_err());
    }

    #[test]
    fn test_failure_move_layer_nonexistent_id() {
        let mut doc = Document::new("doc-1".to_string(), 100, 100);
        let result = doc.move_layer("does-not-exist", 50.0, 50.0);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[test]
    fn test_failure_transform_nonexistent_layer() {
        let mut doc = Document::new("doc-1".to_string(), 100, 100);
        let transform = Transform::new();
        let result = doc.apply_transform("does-not-exist", transform);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }
}
