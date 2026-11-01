// SPDX-License-Identifier: AGPL-3.0-or-later
use serde::{Deserialize, Serialize};
use std::io::Cursor;

// ── PNG decode offload (measured 1.8x over the previous host-side decode) ────
// Bench (2026-08-21): image-crate png path 145ms @4K vs the previous
// host-side decode reference at 262ms. Decode-into-pinned-buffer shape: the file
// bytes cross the host boundary once; decoded RGBA stays in memory for
// zero-copy readout. Two-step (dimensions first) lets the caller apply its own
// max-dimension guard BEFORE allocating pixel memory.

/// Header-only dimension probe. Errors on invalid/truncated PNG.
/// Errors are `String`.
pub fn png_dimensions(png_bytes: &[u8]) -> Result<DecodedImageMeta, String> {
    let reader = image::ImageReader::new(Cursor::new(png_bytes))
        .with_guessed_format()
        .map_err(|e| format!("PNG read failed: {}", e))?;
    let dims = reader
        .into_dimensions()
        .map_err(|e| format!("PNG header parse failed: {}", e))?;
    Ok(DecodedImageMeta {
        width: dims.0,
        height: dims.1,
    })
}

pub struct DecodedImageMeta {
    width: u32,
    height: u32,
}

impl DecodedImageMeta {
    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
}

fn png_ihdr_dimensions(png_bytes: &[u8]) -> Option<(u32, u32)> {
    if png_bytes.get(..8)? != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    let ihdr = png_bytes.get(8..33)?;
    if u32::from_be_bytes(ihdr[0..4].try_into().ok()?) != 13 || &ihdr[4..8] != b"IHDR" {
        return None;
    }

    let chunk_crc = png_bytes.get(12..29)?.iter().fold(!0u32, |mut crc, byte| {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & 0u32.wrapping_sub(crc & 1));
        }
        crc
    }) ^ !0u32;
    if u32::from_be_bytes(ihdr[21..25].try_into().ok()?) != chunk_crc {
        return None;
    }

    let width = u32::from_be_bytes(ihdr[8..12].try_into().ok()?);
    let height = u32::from_be_bytes(ihdr[12..16].try_into().ok()?);
    (width != 0 && height != 0).then_some((width, height))
}

/// Full decode of `png_bytes` into `dst` (must be >= width*height*4). Returns
/// the decoded dimensions. Out-parameter shape keeps buffer ownership with the
/// caller.
pub fn decode_png_into(png_bytes: &[u8], dst: &mut [u8]) -> Result<DecodedImageMeta, String> {
    if let Some((width, height)) = png_ihdr_dimensions(png_bytes) {
        let need = (width as usize)
            .checked_mul(height as usize)
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or_else(|| "PNG dimensions too large".to_string())?;
        if dst.len() < need {
            return Err(format!("buffer too small: {} < {}", dst.len(), need));
        }
    }

    let img = image::load_from_memory_with_format(png_bytes, image::ImageFormat::Png)
        .map_err(|e| format!("PNG decode failed: {}", e))?;
    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();
    let pixels = rgba.as_raw();
    let need = pixels.len();
    if dst.len() < need {
        return Err(format!("buffer too small: {} < {}", dst.len(), need));
    }
    dst[..need].copy_from_slice(pixels);
    Ok(DecodedImageMeta { width, height })
}

/// Header-only dimensions for every supported import format. This allocates no
/// pixel memory; callers size their destination from the result and enforce
/// their own product limits before decoding.
pub fn image_dimensions(
    image_bytes: &[u8],
    format: image::ImageFormat,
) -> Result<DecodedImageMeta, String> {
    if !is_supported_image_format(format) {
        return Err(format!("Unsupported image format: {format:?}"));
    }
    if format == image::ImageFormat::Png {
        let (width, height) = png_ihdr_dimensions(image_bytes)
            .ok_or_else(|| "Image dimensions probe failed: invalid PNG header".to_string())?;
        return Ok(DecodedImageMeta { width, height });
    }
    let mut probe = image::ImageReader::with_format(Cursor::new(image_bytes), format);
    probe.no_limits();
    let (width, height) = probe
        .into_dimensions()
        .map_err(|e| format!("Image dimensions probe failed: {e}"))?;
    Ok(DecodedImageMeta { width, height })
}

fn is_supported_image_format(format: image::ImageFormat) -> bool {
    matches!(
        format,
        image::ImageFormat::Png
            | image::ImageFormat::Jpeg
            | image::ImageFormat::WebP
            | image::ImageFormat::Gif
            | image::ImageFormat::Bmp
            | image::ImageFormat::Tiff
    )
}

/// Core pixel decode primitive; this does not provide production import coverage.
pub fn decode_image_into(
    image_bytes: &[u8],
    format: image::ImageFormat,
    dst: &mut [u8],
) -> Result<DecodedImageMeta, String> {
    let (width, height) = {
        let meta = image_dimensions(image_bytes, format)?;
        (meta.width(), meta.height())
    };
    let needed = (width as usize)
        .checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| "Image dimensions too large".to_string())?;
    if dst.len() < needed {
        return Err(format!("buffer too small: {} < {}", dst.len(), needed));
    }

    let destination_limit =
        u64::try_from(dst.len()).map_err(|_| "destination buffer too large".to_string())?;
    let allocation_budget = dst
        .len()
        .checked_add(image_bytes.len())
        .ok_or_else(|| "decode allocation limit too large".to_string())?;
    let max_alloc = u64::try_from(allocation_budget)
        .map_err(|_| "decode allocation limit too large".to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(width);
    limits.max_image_height = Some(height);
    limits.max_alloc = Some(max_alloc);

    let mut reader = image::ImageReader::with_format(Cursor::new(image_bytes), format);
    reader.limits(limits);
    let decoder = reader
        .into_decoder()
        .map_err(|e| format!("Image decoder setup failed: {e}"))?;
    let decoded_bytes = image::ImageDecoder::total_bytes(&decoder);
    if decoded_bytes > destination_limit {
        return Err(format!(
            "decoded image allocation too large: {decoded_bytes} > {destination_limit}"
        ));
    }

    let rgba = image::DynamicImage::from_decoder(decoder)
        .map_err(|e| format!("Image decode failed: {e}"))?
        .to_rgba8();
    let pixels = rgba.as_raw();
    if dst.len() < pixels.len() {
        return Err(format!(
            "buffer too small: {} < {}",
            dst.len(),
            pixels.len()
        ));
    }
    dst[..pixels.len()].copy_from_slice(pixels);
    Ok(DecodedImageMeta {
        width: rgba.width(),
        height: rgba.height(),
    })
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum ExportFormat {
    PNG,
    JPEG,
    WebP,
    TIFF,
}

/// Encode a pre-composited RGBA buffer to a file format. The caller composites
/// layers and passes the final RGBA bytes here. Kept free
/// of any `Document`/layer model so the core crate carries no inert state code.
pub fn encode_image(
    width: u32,
    height: u32,
    rgba_bytes: &[u8],
    format_str: &str,
    quality: u8,
) -> Result<Vec<u8>, String> {
    encode_image_inner(width, height, rgba_bytes, format_str, quality)
}

fn encode_image_inner(
    width: u32,
    height: u32,
    rgba_bytes: &[u8],
    format_str: &str,
    quality: u8,
) -> Result<Vec<u8>, String> {
    if rgba_bytes.len() != (width as usize * height as usize * 4) {
        return Err("Invalid pixel buffer length for width and height".to_string());
    }

    let format = match format_str.to_lowercase().as_str() {
        "png" => ExportFormat::PNG,
        "jpeg" | "jpg" => ExportFormat::JPEG,
        "webp" => ExportFormat::WebP,
        "tiff" => ExportFormat::TIFF,
        _ => return Err("Unsupported export format".to_string()),
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
        .ok_or_else(|| "Failed to create ImageBuffer from pixel vector".to_string())?;

    match format {
        ExportFormat::PNG => {
            img_buffer
                .write_to(
                    &mut Cursor::new(&mut encoded_bytes),
                    image::ImageFormat::Png,
                )
                .map_err(|e| format!("Failed to encode PNG: {}", e))?;
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
            .ok_or_else(|| "Failed to create RGB buffer".to_string())?;

            let clamped_quality = quality.clamp(1, 100);
            let mut cursor = Cursor::new(&mut encoded_bytes);
            let mut encoder =
                image::codecs::jpeg::JpegEncoder::new_with_quality(&mut cursor, clamped_quality);
            encoder
                .encode_image(&rgb_buffer)
                .map_err(|e| format!("Failed to encode JPEG: {}", e))?;
        }
        ExportFormat::WebP => {
            // NOTE: the `image` crate's WebP support (image-webp 0.2.4) is
            // LOSSLESS-ONLY — it ignores `quality` and always writes a VP8L
            // (lossless) file. That is CORRECT (exact round-trip, no data loss),
            // but produces large files and does not honor the quality slider.
            // The frontend therefore routes WebP to the browser Canvas
            // convertToBlob (lossy, quality-aware) via an early-return in
            // the frontend export module; this Rust path is the lossless fallback and is kept
            // correct. See FEATURES.md WebP note (the earlier "~5 KB defect" was a
            // compressible test-fixture artifact, not data loss).
            img_buffer
                .write_to(
                    &mut Cursor::new(&mut encoded_bytes),
                    image::ImageFormat::WebP,
                )
                .map_err(|e| format!("Failed to encode WebP: {}", e))?;
        }
        ExportFormat::TIFF => {
            // TIFF is lossless and preserves alpha; quality is ignored.
            img_buffer
                .write_to(
                    &mut Cursor::new(&mut encoded_bytes),
                    image::ImageFormat::Tiff,
                )
                .map_err(|e| format!("Failed to encode TIFF: {}", e))?;
        }
    }

    Ok(encoded_bytes)
}

#[cfg(test)]
mod tests;
