// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::document::Document;
use serde::{Deserialize, Serialize};
use std::io::Cursor;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum ExportFormat {
    PNG,
    JPEG,
    WebP,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportSettings {
    pub format: ExportFormat,
    pub quality: u8, // 1 to 100
}

impl ExportSettings {
    pub fn new(format: ExportFormat, quality: u8) -> Self {
        Self { format, quality }
    }
}

pub fn flatten_document(doc: &Document) -> Vec<u8> {
    let w = doc.width as i32;
    let h = doc.height as i32;
    let mut flattened = vec![0u8; (doc.width * doc.height * 4) as usize];

    // Reverse stack order: composite from bottom layer to top layer
    for layer in doc.layers.iter().rev() {
        if !layer.visible {
            continue;
        }

        let lx_offset = layer.x as i32;
        let ly_offset = layer.y as i32;
        let lw = layer.width as i32;
        let lh = layer.height as i32;
        let layer_opacity = layer.opacity;

        for py in 0..lh {
            let doc_y = ly_offset + py;
            if doc_y < 0 || doc_y >= h {
                continue;
            }

            for px in 0..lw {
                let doc_x = lx_offset + px;
                if doc_x < 0 || doc_x >= w {
                    continue;
                }

                let layer_idx = ((py * lw + px) * 4) as usize;
                let doc_idx = ((doc_y * w + doc_x) * 4) as usize;

                let src_r = layer.bitmap_ref.pixel_data[layer_idx] as f32 / 255.0;
                let src_g = layer.bitmap_ref.pixel_data[layer_idx + 1] as f32 / 255.0;
                let src_b = layer.bitmap_ref.pixel_data[layer_idx + 2] as f32 / 255.0;
                let src_a =
                    layer.bitmap_ref.pixel_data[layer_idx + 3] as f32 / 255.0 * layer_opacity;

                if src_a <= 0.0 {
                    continue;
                }

                let dest_r = flattened[doc_idx] as f32 / 255.0;
                let dest_g = flattened[doc_idx + 1] as f32 / 255.0;
                let dest_b = flattened[doc_idx + 2] as f32 / 255.0;
                let dest_a = flattened[doc_idx + 3] as f32 / 255.0;

                // Porter-Duff alpha compositing
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

                flattened[doc_idx] = (out_r * 255.0).round().min(255.0) as u8;
                flattened[doc_idx + 1] = (out_g * 255.0).round().min(255.0) as u8;
                flattened[doc_idx + 2] = (out_b * 255.0).round().min(255.0) as u8;
                flattened[doc_idx + 3] = (out_a * 255.0).round().min(255.0) as u8;
            }
        }
    }
    flattened
}

pub fn export_document(doc: &Document, settings: &ExportSettings) -> Result<Vec<u8>, String> {
    let mut flattened = flatten_document(doc);

    // Composite JPEGs on white solid background
    if let ExportFormat::JPEG = settings.format {
        for i in (0..flattened.len()).step_by(4) {
            let r = flattened[i] as f32 / 255.0;
            let g = flattened[i + 1] as f32 / 255.0;
            let b = flattened[i + 2] as f32 / 255.0;
            let a = flattened[i + 3] as f32 / 255.0;

            let out_r = r * a + 1.0 * (1.0 - a);
            let out_g = g * a + 1.0 * (1.0 - a);
            let out_b = b * a + 1.0 * (1.0 - a);

            flattened[i] = (out_r * 255.0).round().min(255.0) as u8;
            flattened[i + 1] = (out_g * 255.0).round().min(255.0) as u8;
            flattened[i + 2] = (out_b * 255.0).round().min(255.0) as u8;
            flattened[i + 3] = 255;
        }
    }

    let mut encoded_bytes = Vec::new();

    let img_buffer =
        image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(doc.width, doc.height, flattened)
            .ok_or_else(|| {
                "Failed to create ImageBuffer from flattened pixel vector".to_string()
            })?;

    match settings.format {
        ExportFormat::PNG => {
            img_buffer
                .write_to(
                    &mut Cursor::new(&mut encoded_bytes),
                    image::ImageFormat::Png,
                )
                .map_err(|e| format!("Failed to encode PNG: {}", e))?;
        }
        ExportFormat::JPEG => {
            // Convert to RGB first for standard JPEGs
            let rgb_buffer = image::ImageBuffer::<image::Rgb<u8>, _>::from_raw(
                doc.width,
                doc.height,
                img_buffer
                    .pixels()
                    .flat_map(|p| [p[0], p[1], p[2]])
                    .collect::<Vec<u8>>(),
            )
            .ok_or_else(|| "Failed to create RGB buffer".to_string())?;

            let quality = settings.quality.clamp(1, 100);
            let mut cursor = Cursor::new(&mut encoded_bytes);
            let mut encoder =
                image::codecs::jpeg::JpegEncoder::new_with_quality(&mut cursor, quality);
            encoder
                .encode_image(&rgb_buffer)
                .map_err(|e| format!("Failed to encode JPEG: {}", e))?;
        }
        ExportFormat::WebP => {
            img_buffer
                .write_to(
                    &mut Cursor::new(&mut encoded_bytes),
                    image::ImageFormat::WebP,
                )
                .map_err(|e| format!("Failed to encode WebP: {}", e))?;
        }
    }

    Ok(encoded_bytes)
}

use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn encode_image_wasm(
    width: u32,
    height: u32,
    rgba_bytes: &[u8],
    format_str: &str,
    quality: u8,
) -> Result<Vec<u8>, JsValue> {
    if rgba_bytes.len() != (width as usize * height as usize * 4) {
        return Err(JsValue::from_str(
            "Invalid pixel buffer length for width and height",
        ));
    }

    let format = match format_str.to_lowercase().as_str() {
        "png" => ExportFormat::PNG,
        "jpeg" | "jpg" => ExportFormat::JPEG,
        "webp" => ExportFormat::WebP,
        _ => return Err(JsValue::from_str("Unsupported export format")),
    };

    let mut pixels = rgba_bytes.to_vec();

    // Composite JPEGs on white solid background if background is transparent
    if let ExportFormat::JPEG = format {
        for i in (0..pixels.len()).step_by(4) {
            let r = pixels[i] as f32 / 255.0;
            let g = pixels[i + 1] as f32 / 255.0;
            let b = pixels[i + 2] as f32 / 255.0;
            let a = pixels[i + 3] as f32 / 255.0;

            let out_r = r * a + 1.0 * (1.0 - a);
            let out_g = g * a + 1.0 * (1.0 - a);
            let out_b = b * a + 1.0 * (1.0 - a);

            pixels[i] = (out_r * 255.0).round().min(255.0) as u8;
            pixels[i + 1] = (out_g * 255.0).round().min(255.0) as u8;
            pixels[i + 2] = (out_b * 255.0).round().min(255.0) as u8;
            pixels[i + 3] = 255;
        }
    }

    let mut encoded_bytes = Vec::new();

    let img_buffer = image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(width, height, pixels)
        .ok_or_else(|| JsValue::from_str("Failed to create ImageBuffer from pixel vector"))?;

    match format {
        ExportFormat::PNG => {
            img_buffer
                .write_to(
                    &mut Cursor::new(&mut encoded_bytes),
                    image::ImageFormat::Png,
                )
                .map_err(|e| JsValue::from_str(&format!("Failed to encode PNG: {}", e)))?;
        }
        ExportFormat::JPEG => {
            let rgb_buffer = image::ImageBuffer::<image::Rgb<u8>, _>::from_raw(
                width,
                height,
                img_buffer
                    .pixels()
                    .flat_map(|p| [p[0], p[1], p[2]])
                    .collect::<Vec<u8>>(),
            )
            .ok_or_else(|| JsValue::from_str("Failed to create RGB buffer"))?;

            let clamped_quality = quality.clamp(1, 100);
            let mut cursor = Cursor::new(&mut encoded_bytes);
            let mut encoder =
                image::codecs::jpeg::JpegEncoder::new_with_quality(&mut cursor, clamped_quality);
            encoder
                .encode_image(&rgb_buffer)
                .map_err(|e| JsValue::from_str(&format!("Failed to encode JPEG: {}", e)))?;
        }
        ExportFormat::WebP => {
            img_buffer
                .write_to(
                    &mut Cursor::new(&mut encoded_bytes),
                    image::ImageFormat::WebP,
                )
                .map_err(|e| JsValue::from_str(&format!("Failed to encode WebP: {}", e)))?;
        }
    }

    Ok(encoded_bytes)
}

#[cfg(test)]

mod tests {
    use super::*;
    use crate::document::Document;
    use crate::layers::Layer;

    #[test]
    fn test_document_flattening() {
        let mut doc = Document::new("doc-1".to_string(), 2, 2);

        // Layer 1: Red background [255, 0, 0, 255]
        let mut l1 = Layer::new("l-1".to_string(), "Layer 1".to_string(), 2, 2);
        l1.bitmap_ref.pixel_data = vec![
            255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255,
        ];

        // Layer 2: Semitransparent Blue overlay [0, 0, 255, 128]
        let mut l2 = Layer::new("l-2".to_string(), "Layer 2".to_string(), 2, 2);
        l2.bitmap_ref.pixel_data = vec![
            0, 0, 255, 128, 0, 0, 255, 128, 0, 0, 255, 128, 0, 0, 255, 128,
        ];

        // In photrez convention: layers is sorted index-0 (top) to index-N (bottom).
        doc.layers = vec![l2, l1];

        let flattened = flatten_document(&doc);

        // Expected blended color (Porter-Duff alpha compositing):
        // out_a = 0.5 + 1.0 * 0.5 = 1.0
        // out_r = (0.0 * 0.5 + 255.0 * 1.0 * 0.5) / 1.0 = 127.5 => ~128
        // out_b = (255.0 * 0.5 + 0.0 * 1.0 * 0.5) / 1.0 = 127.5 => ~128
        assert!(flattened[0] >= 127 && flattened[0] <= 128); // Red
        assert_eq!(flattened[1], 0); // Green
        assert!(flattened[2] >= 127 && flattened[2] <= 128); // Blue
        assert_eq!(flattened[3], 255); // Alpha
    }

    #[test]
    fn test_image_export_encoding() {
        let mut doc = Document::new("doc-1".to_string(), 2, 2);
        let mut l1 = Layer::new("l-1".to_string(), "Layer 1".to_string(), 2, 2);
        l1.bitmap_ref.pixel_data = vec![
            255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255,
        ];
        doc.add_layer(l1);

        let settings_png = ExportSettings::new(ExportFormat::PNG, 100);
        let png_bytes = export_document(&doc, &settings_png).unwrap();
        assert_eq!(&png_bytes[0..4], &[0x89, 0x50, 0x4E, 0x47]);

        let settings_jpeg = ExportSettings::new(ExportFormat::JPEG, 85);
        let jpeg_bytes = export_document(&doc, &settings_jpeg).unwrap();
        assert_eq!(&jpeg_bytes[0..2], &[0xFF, 0xD8]);
    }

    #[test]
    fn test_image_export_webp() {
        let mut doc = Document::new("doc-1".to_string(), 10, 10);
        let mut layer = Layer::new("layer-1".to_string(), "Test".to_string(), 10, 10);
        // Fill with opaque white pixels
        for i in (0..layer.bitmap_ref.pixel_data.len()).step_by(4) {
            layer.bitmap_ref.pixel_data[i] = 255;
            layer.bitmap_ref.pixel_data[i + 1] = 255;
            layer.bitmap_ref.pixel_data[i + 2] = 255;
            layer.bitmap_ref.pixel_data[i + 3] = 255;
        }
        doc.add_layer(layer);

        let settings = ExportSettings::new(ExportFormat::WebP, 80);
        let result = export_document(&doc, &settings);
        assert!(result.is_ok());
        let bytes = result.unwrap();
        assert!(bytes.len() > 0);
        // WebP magic bytes: "RIFF" at offset 0
        assert_eq!(&bytes[0..4], b"RIFF");
    }
}
