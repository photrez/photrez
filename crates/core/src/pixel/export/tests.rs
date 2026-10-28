use super::*;

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
    // feature "tiff") on a 4K RGBA buffer. Pure CPU; no GPU needed. Answers the export-format cost question:
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
            let out = encode_image(w, h, &px, fmt, 95).expect("encode");
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
        let out = encode_image(w, h, &px, "webp", 95).expect("webp encode");
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
        let out = encode_image(w, h, &noise, "webp", 95).expect("webp encode");
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        println!(
            "  WEBP[lossless, pseudo-noise 4K]: {:7.2} ms -> {} KB  (large = correct)",
            ms,
            out.len() / 1024
        );
        // A smooth ramp (i*7) — highly compressible,
        // hence the misleading 5 KB. Decode still round-trips exactly.
        let ramp: Vec<u8> = (0..noise.len()).map(|i| (i * 7) as u8).collect();
        let ramp_out = encode_image(w, h, &ramp, "webp", 95).expect("webp encode");
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

    fn encode_test_rgb(format: image::ImageFormat) -> Vec<u8> {
        let pixels = image::ImageBuffer::<image::Rgb<u8>, _>::from_raw(
            2,
            2,
            vec![21, 47, 89, 21, 47, 89, 21, 47, 89, 21, 47, 89],
        )
        .expect("RGB fixture");
        let mut out = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(pixels)
            .write_to(&mut out, format)
            .expect("encode fixture");
        out.into_inner()
    }

    fn set_png_dimensions(png: &mut [u8], width: u32, height: u32) {
        png[16..20].copy_from_slice(&width.to_be_bytes());
        png[20..24].copy_from_slice(&height.to_be_bytes());
        let mut crc = !0u32;
        for byte in &png[12..29] {
            crc ^= u32::from(*byte);
            for _ in 0..8 {
                crc = if crc & 1 == 0 {
                    crc >> 1
                } else {
                    (crc >> 1) ^ 0xedb8_8320
                };
            }
        }
        png[29..33].copy_from_slice(&(!crc).to_be_bytes());
    }

    #[test]
    fn dimensions_probe_matches() {
        let meta = png_dimensions(&tiny_png()).expect("dims");
        assert_eq!(meta.width(), 2);
        assert_eq!(meta.height(), 2);
    }

    #[test]
    fn decode_into_buffer_pixel_exact() {
        let mut buf = vec![0u8; 2 * 2 * 4];
        let meta = decode_png_into(&tiny_png(), &mut buf).expect("decode");
        assert_eq!((meta.width(), meta.height()), (2, 2));
        assert_eq!(
            &buf,
            &[255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 0, 0, 0, 0]
        );
    }

    #[test]
    fn rejects_truncated_and_undersized_buffer() {
        assert!(png_dimensions(b"not a png").is_err());
        let mut small = vec![0u8; 4]; // too small on purpose
        assert!(decode_png_into(&tiny_png(), &mut small).is_err());
    }

    #[test]
    fn undersized_buffer_is_rejected_from_ihdr_before_body_decode() {
        let png = tiny_png();
        let ihdr_end = 8 + 4 + 4 + 13 + 4;
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(&png[12..16], b"IHDR");

        let truncated_png = png[..ihdr_end].to_vec();
        let mut destination = vec![0xA5; 15];
        let result = decode_png_into(&truncated_png, &mut destination);

        assert_eq!(result.err().as_deref(), Some("buffer too small: 15 < 16"));
        assert_eq!(destination, vec![0xA5; 15]);
    }

    #[test]
    fn overflowing_ihdr_rgba_size_returns_error_without_panic() {
        let mut png = tiny_png();
        set_png_dimensions(&mut png, u32::MAX, u32::MAX);

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            decode_png_into(&png, &mut [])
        }));
        assert!(matches!(result, Ok(Err(ref error)) if error == "PNG dimensions too large"));
    }

    #[test]
    fn bounded_decode_png_pixels_are_exact() {
        let mut dst = vec![0; 2 * 2 * 4];
        let meta =
            decode_image_into(&tiny_png(), image::ImageFormat::Png, &mut dst).expect("decode PNG");

        assert_eq!((meta.width(), meta.height()), (2, 2));
        assert_eq!(
            dst,
            [255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 0, 0, 0, 0]
        );
    }

    #[test]
    fn bounded_decode_dispatches_enabled_formats() {
        for format in [
            image::ImageFormat::Jpeg,
            image::ImageFormat::WebP,
            image::ImageFormat::Gif,
            image::ImageFormat::Bmp,
            image::ImageFormat::Tiff,
        ] {
            let encoded = encode_test_rgb(format);
            let mut dst = vec![0; 2 * 2 * 4];
            let meta = decode_image_into(&encoded, format, &mut dst)
                .unwrap_or_else(|error| panic!("{format:?} decode failed: {error}"));
            assert_eq!((meta.width(), meta.height()), (2, 2), "{format:?}");
        }
    }

    #[test]
    fn bounded_decode_rejects_small_destination_before_body_decode() {
        let png = tiny_png();
        let header_only = &png[..33];
        let mut dst = vec![0xA5; 15];

        let result = decode_image_into(header_only, image::ImageFormat::Png, &mut dst);

        assert_eq!(result.err().as_deref(), Some("buffer too small: 15 < 16"));
        assert_eq!(dst, vec![0xA5; 15]);
    }

    #[test]
    fn bounded_decode_rejects_truncated_input() {
        let png = tiny_png();
        let mut dst = vec![0; 2 * 2 * 4];

        assert!(decode_image_into(&png[..33], image::ImageFormat::Png, &mut dst).is_err());
    }

    #[test]
    fn bounded_decode_rejects_unsupported_format() {
        let error = match decode_image_into(b"not an image", image::ImageFormat::Pnm, &mut []) {
            Err(error) => error,
            Ok(_) => panic!("PNM is not an enabled import format"),
        };

        assert!(error.contains("Unsupported"));
    }

    #[test]
    fn bounded_decode_rejects_overflowing_dimensions_without_panicking() {
        let mut png = tiny_png();
        set_png_dimensions(&mut png, u32::MAX, u32::MAX);

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            decode_image_into(&png, image::ImageFormat::Png, &mut [])
        }));

        assert!(matches!(result, Ok(Err(_))));
    }
}
