// SPDX-License-Identifier: AGPL-3.0-or-later
use serde::{Deserialize, Serialize};
use std::io::Cursor;
use wasm_bindgen::prelude::*;

// ── PNG decode offload (Rust-migration package: measured 1.8x vs browser) ────
// R5 bench (2026-08-21): image-crate png path 145ms @4K vs Chrome
// createImageBitmap 262ms. Decode-into-pinned-buffer shape: the file bytes
// cross the boundary once; decoded RGBA stays in wasm memory for zero-copy
// JS readout via rgba_buffer_view. Two-step (dimensions first) lets TS apply
// its MAX_CANVAS_DIM guard BEFORE allocating pixel memory.

/// Header-only dimension probe. Errors on invalid/truncated PNG.
/// Errors are `String` (wasm-bindgen converts to JS Error; works in native
/// tests where JsValue cannot be constructed).
#[wasm_bindgen]
pub fn png_dimensions_wasm(png_bytes: &[u8]) -> Result<DecodedImageMeta, String> {
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

#[wasm_bindgen]
pub struct DecodedImageMeta {
    width: u32,
    height: u32,
}

#[wasm_bindgen]
impl DecodedImageMeta {
    #[wasm_bindgen(getter)]
    pub fn width(&self) -> u32 {
        self.width
    }
    #[wasm_bindgen(getter)]
    pub fn height(&self) -> u32 {
        self.height
    }
}

/// Full decode of `png_bytes` into the pinned RGBA buffer `buf_id`
/// (must be >= width*height*4; allocate via alloc_rgba_buffer after probing
/// dimensions). Returns the decoded dimensions.
#[wasm_bindgen]
pub fn decode_png_into_wasm(png_bytes: &[u8], buf_id: u32) -> Result<DecodedImageMeta, String> {
    let img = image::load_from_memory_with_format(png_bytes, image::ImageFormat::Png)
        .map_err(|e| format!("PNG decode failed: {}", e))?;
    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();
    let need = width as usize * height as usize * 4;
    crate::kernel::with_buffer_mut(buf_id, |dst| {
        if dst.len() < need {
            return Err(format!("buffer too small: {} < {}", dst.len(), need));
        }
        dst[..need].copy_from_slice(rgba.as_raw());
        Ok(())
    })?;
    Ok(DecodedImageMeta { width, height })
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum ExportFormat {
    PNG,
    JPEG,
    WebP,
    TIFF,
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
        "tiff" => ExportFormat::TIFF,
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
            // NOTE: the `image` crate's WebP support (image-webp 0.2.4) is
            // LOSSLESS-ONLY — it ignores `quality` and always writes a VP8L
            // (lossless) file. That is CORRECT (exact round-trip, no data loss),
            // but produces large files and does not honor the quality slider.
            // The frontend therefore routes WebP to the browser Canvas
            // convertToBlob (lossy, quality-aware) via an early-return in
            // wasmExport.ts; this Rust path is the lossless fallback and is kept
            // correct. See FEATURES.md WebP note (the earlier "~5 KB defect" was a
            // compressible test-fixture artifact, not data loss).
            img_buffer
                .write_to(
                    &mut Cursor::new(&mut encoded_bytes),
                    image::ImageFormat::WebP,
                )
                .map_err(|e| JsValue::from_str(&format!("Failed to encode WebP: {}", e)))?;
        }
        ExportFormat::TIFF => {
            // TIFF is lossless and preserves alpha; quality is ignored.
            img_buffer
                .write_to(
                    &mut Cursor::new(&mut encoded_bytes),
                    image::ImageFormat::Tiff,
                )
                .map_err(|e| JsValue::from_str(&format!("Failed to encode TIFF: {}", e)))?;
        }
    }

    Ok(encoded_bytes)
}

#[cfg(test)]
mod benchmarks {
    use super::*;

    fn make_rgba(w: u32, h: u32) -> Vec<u8> {
        let mut v = vec![0u8; w as usize * h as usize * 4];
        for (i, b) in v.iter_mut().enumerate() {
            *b = (i * 7) as u8;
        }
        v
    }

    // Measures encode throughput of the existing export path + TIFF (image crate
    // feature "tiff") on a 4K RGBA buffer. Pure CPU; no GPU needed. Answers R2:
    // is adding export formats cheap? Run:
    //   cargo test -p photrez-core --release bench_export_formats_4k -- --nocapture
    #[test]
    fn bench_export_formats_4k() {
        let w = 4000u32;
        let h = 3000u32;
        let px = make_rgba(w, h);
        println!(
            "\n[export-bench] W={w} H={h} px={} (4K RGBA, ~{} MB)",
            w as usize * h as usize,
            (w as usize * h as usize * 4) / 1_000_000
        );
        // Existing supported formats via the production export entry point.
        for fmt in ["png", "jpeg", "webp"] {
            let t0 = std::time::Instant::now();
            let out = encode_image_wasm(w, h, &px, fmt, 95).expect("encode");
            let ms = t0.elapsed().as_secs_f64() * 1000.0;
            println!(
                "  {:<5}: {:7.2} ms  -> {} KB",
                fmt.to_uppercase(),
                ms,
                out.len() / 1024
            );
        }
        // TIFF via image crate directly (feature "tiff") — the candidate new format.
        let t0 = std::time::Instant::now();
        let img = image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(w, h, px.clone()).unwrap();
        let mut out = Vec::new();
        img.write_to(
            &mut std::io::Cursor::new(&mut out),
            image::ImageFormat::Tiff,
        )
        .expect("tiff");
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        println!("  TIFF : {:7.2} ms  -> {} KB", ms, out.len() / 1024);
    }

    // Incompressible pseudo-random RGBA (Knuth multiplicative hash per byte).
    // Used so the WebP lossless encoder cannot cheat via trivial patterns.
    fn pseudo_noise(w: u32, h: u32) -> Vec<u8> {
        let n = (w as usize) * (h as usize) * 4;
        let mut v = Vec::with_capacity(n);
        for i in 0..n {
            let x = (i as u32).wrapping_mul(2654435761).wrapping_add(i as u32);
            v.push((x ^ (x >> 15) ^ (x << 13)) as u8);
        }
        v
    }

    // WebP correctness guard. The `image`/image-webp WebP path is LOSSLESS
    // (VP8L) and ignores `quality`; it is CORRECT (exact round-trip), just large.
    // The earlier "~5 KB WebP = data loss" alarm was a fixture artifact: the
    // `(i*7)` ramp is highly compressible and shrinks to a few KB even losslessly.
    // This guard uses INCOMPRESSIBLE data and asserts the decode equals the
    // original exactly (proving no data loss).
    #[test]
    fn verify_webp_export_valid() {
        let w = 4000u32;
        let h = 3000u32;
        let px = pseudo_noise(w, h);
        let out = encode_image_wasm(w, h, &px, "webp", 95).expect("webp encode");
        // Lossless output of incompressible 4K must be large (MBs), not ~5 KB.
        assert!(
            out.len() > 5_000_000,
            "lossless WebP of 4K noise only {} KB — encoder is broken",
            out.len() / 1024
        );
        // Exact round-trip: decode must equal the original pixels.
        let img = image::load_from_memory(&out).expect("decode webp");
        assert_eq!((img.width(), img.height()), (w, h));
        let dec = img.to_rgba8().into_raw();
        assert_eq!(
            dec, px,
            "lossless WebP round-trip mismatch — real data loss"
        );
    }

    // WebP characterization (image-webp = LOSSLESS-only). Shows the encoder is
    // correct: incompressible 4K -> large file (MBs), and the old "~5 KB defect"
    // was just the compressible ramp fixture. Run:
    //   cargo test -p photrez-core --release bench_webp_lossless -- --nocapture
    #[test]
    fn bench_webp_lossless() {
        let w = 4000u32;
        let h = 3000u32;
        let noise = pseudo_noise(w, h);
        let t0 = std::time::Instant::now();
        let out = encode_image_wasm(w, h, &noise, "webp", 95).expect("webp encode");
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        println!(
            "  WEBP[lossless, pseudo-noise 4K]: {:7.2} ms -> {} KB  (large = correct)",
            ms,
            out.len() / 1024
        );
        // The original B1 fixture was a smooth ramp (i*7) — highly compressible,
        // hence the misleading 5 KB. Decode still round-trips exactly.
        let ramp: Vec<u8> = (0..noise.len()).map(|i| (i * 7) as u8).collect();
        let ramp_out = encode_image_wasm(w, h, &ramp, "webp", 95).expect("webp encode");
        println!(
            "  WEBP[lossless, ramp 4K]       : {} KB  (small because ramp compresses; NOT data loss)",
            ramp_out.len() / 1024
        );
    }
}

#[cfg(test)]
mod png_decode_tests {
    use super::*;

    fn tiny_png() -> Vec<u8> {
        // Encode a 2x2 (red, green / blue, transparent) PNG via the image crate.
        let img = image::RgbaImage::from_fn(2, 2, |x, y| match (x, y) {
            (0, 0) => image::Rgba([255, 0, 0, 255]),
            (1, 0) => image::Rgba([0, 255, 0, 255]),
            (0, 1) => image::Rgba([0, 0, 255, 255]),
            _ => image::Rgba([0, 0, 0, 0]),
        });
        let mut out = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut out, image::ImageFormat::Png)
            .expect("encode");
        out.into_inner()
    }

    #[test]
    fn dimensions_probe_matches() {
        let meta = png_dimensions_wasm(&tiny_png()).expect("dims");
        assert_eq!(meta.width(), 2);
        assert_eq!(meta.height(), 2);
    }

    #[test]
    fn decode_into_buffer_pixel_exact() {
        let buf = crate::kernel::alloc_rgba_buffer(2 * 2 * 4);
        let meta = decode_png_into_wasm(&tiny_png(), buf).expect("decode");
        assert_eq!((meta.width(), meta.height()), (2, 2));
        crate::kernel::with_buffer(buf, |px| {
            assert_eq!(
                px,
                &[255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 0, 0, 0, 0]
            );
        });
        crate::kernel::free_rgba_buffer(buf);
    }

    #[test]
    fn rejects_truncated_and_undersized_buffer() {
        assert!(png_dimensions_wasm(b"not a png").is_err());
        let buf = crate::kernel::alloc_rgba_buffer(4); // too small on purpose
        assert!(decode_png_into_wasm(&tiny_png(), buf).is_err());
        crate::kernel::free_rgba_buffer(buf);
    }
}
