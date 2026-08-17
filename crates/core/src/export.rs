// SPDX-License-Identifier: AGPL-3.0-or-later
use serde::{Deserialize, Serialize};
use std::io::Cursor;
use wasm_bindgen::prelude::*;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum ExportFormat {
    PNG,
    JPEG,
    WebP,
}

/// Encode a pre-composited RGBA buffer to a file format. This is the only
/// `#[wasm_bindgen]` entry point the frontend uses (`wasmExport.ts` ->
/// `exportDocument.ts`); the app composites layers on the GPU/CPU side and
/// passes the final RGBA bytes here. Kept free of any `Document`/layer model so
/// the core crate carries no inert state code.
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
